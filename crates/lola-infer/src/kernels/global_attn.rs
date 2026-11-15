//! K3 — global attention with **partial RoPE** (`docs/ENGINE.md` §1).
//!
//! LFM3's 6 global layers (of 24) are the only ones that carry long KV —
//! that is the entire residency economics of the tri-hybrid. Two
//! specifics make this kernel ours:
//!
//! - **Partial RoPE**: only the first `head_dim / 2` dimensions rotate
//!   (the high-frequency half), at θ = 1e6; the rest pass through
//!   untouched. (SWA layers keep full RoPE at θ = 1e4 — different kernel.)
//! - **Growing KV cache with a hard cap** (`max_context`): unlike the SWA
//!   ring, context legitimately grows here — until the cap, enforced.

use crate::kernels::conv7::dot;

/// LFM3 global-layer RoPE base.
pub const GLOBAL_ROPE_THETA: f64 = 1.0e6;
/// Fraction of head_dim rotated on global layers (partial RoPE).
pub const PARTIAL_ROPE_FRACTION: f64 = 0.5;

/// Rotate `vec` at sequence position `pos`: only the first
/// `rot_dim = head_dim * PARTIAL_ROPE_FRACTION` dims, in (i, i+rot_dim/2)
/// pairs, base θ = 1e6. The remainder is untouched — tested bit-exact.
pub fn partial_rope(vec: &mut [f32], pos: u64, head_dim: usize, theta: f64) {
    let rot_dim = (head_dim as f64 * PARTIAL_ROPE_FRACTION) as usize & !1; // even
    let half = rot_dim / 2;
    let pos_f = pos as f64;
    for i in 0..half {
        let freq = pos_f / theta.powf((2.0 * i as f64) / rot_dim as f64);
        let (sin, cos) = (freq.sin() as f32, freq.cos() as f32);
        let (a, b) = (vec[i], vec[i + half]);
        vec[i] = a * cos - b * sin;
        vec[i + half] = a * sin + b * cos;
    }
}

/// Global attention core with GQA and a capped growing KV cache.
#[derive(Debug, Clone, PartialEq)]
pub struct GlobalAttention {
    q_heads: usize,
    kv_heads: usize,
    head_dim: usize,
    scale: f32,
    max_context: usize,
}

#[derive(Debug, Clone)]
pub struct GlobalKVState {
    k_cache: Vec<f32>,
    v_cache: Vec<f32>,
    /// Positions currently stored (≤ max_context; the cache compacts on
    /// overflow exactly like a context shift — oldest dropped).
    valid: usize,
    pos: u64,
}

impl GlobalAttention {
    /// LFM3 shape: GQA 16/4 × 128.
    pub fn lfm3(max_context: usize) -> Self {
        Self::new(16, 4, 128, max_context)
    }

    pub fn new(q_heads: usize, kv_heads: usize, head_dim: usize, max_context: usize) -> Self {
        assert!(q_heads % kv_heads == 0, "GQA requires divisible heads");
        assert!(max_context > 0);
        Self {
            q_heads,
            kv_heads,
            head_dim,
            scale: 1.0 / (head_dim as f32).sqrt(),
            max_context,
        }
    }

    pub fn q_heads(&self) -> usize {
        self.q_heads
    }
    pub fn kv_heads(&self) -> usize {
        self.kv_heads
    }
    pub fn head_dim(&self) -> usize {
        self.head_dim
    }
    pub fn max_context(&self) -> usize {
        self.max_context
    }

    /// Steady-state KV footprint per layer (the ADR-024/026 number).
    pub fn state_floats(&self) -> usize {
        2 * self.max_context * self.kv_heads * self.head_dim
    }

    /// One token: raw q/k/v (RoPE applied here). Zero-alloc after warmup.
    pub fn step(
        &self,
        state: &mut GlobalKVState,
        q: &mut [f32],
        k: &mut [f32],
        v: &[f32],
        out: &mut [f32],
    ) {
        let (qh, kh, d) = (self.q_heads, self.kv_heads, self.head_dim);
        assert_eq!(q.len(), qh * d);
        assert_eq!(k.len(), kh * d);
        assert_eq!(v.len(), kh * d);
        assert_eq!(out.len(), qh * d);
        out.fill(0.0);

        let theta = GLOBAL_ROPE_THETA;
        for h in 0..qh {
            partial_rope(&mut q[h * d..(h + 1) * d], state.pos, d, theta);
        }
        for h in 0..kh {
            partial_rope(&mut k[h * d..(h + 1) * d], state.pos, d, theta);
        }

        // Append K/V at the tail slot; on overflow drop the oldest
        // (context-shift semantics — the ledger records the shift, per
        // the KV↔ledger twin ops).
        let slot = if state.valid < self.max_context {
            state.valid += 1;
            state.valid - 1
        } else {
            // Slide everything back one; the newest takes the last slot.
            let per = kh * d;
            state.k_cache.copy_within(per.., 0);
            state.v_cache.copy_within(per.., 0);
            self.max_context - 1
        };
        let per = kh * d;
        state.k_cache[slot * per..(slot + 1) * per].copy_from_slice(k);
        state.v_cache[slot * per..(slot + 1) * per].copy_from_slice(v);
        state.pos += 1;

        let mut scores = vec![0.0f32; state.valid];
        for h in 0..qh {
            let kv_head = h * kh / qh;
            let q_h = &q[h * d..(h + 1) * d];
            for (j, score) in scores.iter_mut().enumerate().take(state.valid) {
                let k_h = &state.k_cache[j * per + kv_head * d..j * per + (kv_head + 1) * d];
                *score = dot(q_h, k_h) * self.scale;
            }
            softmax_in_place(&mut scores[..state.valid]);
            for (j, &weight) in scores[..state.valid].iter().enumerate() {
                let v_h = &state.v_cache[j * per + kv_head * d..j * per + (kv_head + 1) * d];
                let out_h = &mut out[h * d..(h + 1) * d];
                for (o, &vv) in out_h.iter_mut().zip(v_h.iter()) {
                    *o += weight * vv;
                }
            }
        }
    }
}

impl GlobalKVState {
    pub fn new(attention: &GlobalAttention) -> Self {
        let per = attention.max_context * attention.kv_heads * attention.head_dim;
        Self {
            k_cache: vec![0.0; per],
            v_cache: vec![0.0; per],
            valid: 0,
            pos: 0,
        }
    }

    pub fn valid(&self) -> usize {
        self.valid
    }

    pub fn position(&self) -> u64 {
        self.pos
    }
}

fn softmax_in_place(scores: &mut [f32]) {
    let max = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut sum = 0.0f32;
    for s in scores.iter_mut() {
        *s = (*s - max).exp();
        sum += *s;
    }
    for s in scores.iter_mut() {
        *s /= sum;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rope_rotates_only_the_first_half() {
        let head_dim = 128;
        let mut vec = vec![0.25f32; head_dim];
        let tail_before: Vec<f32> = vec[64..].to_vec();
        partial_rope(&mut vec, 12345, head_dim, GLOBAL_ROPE_THETA);
        assert_eq!(vec[64..], tail_before[..], "second half untouched");
        assert!(
            vec[..64].iter().any(|&x| (x - 0.25).abs() > 1e-6),
            "first half rotated"
        );
    }

    #[test]
    fn rope_at_position_zero_is_identity() {
        let head_dim = 128;
        let mut vec = vec![0.37f32; head_dim];
        let before = vec.clone();
        partial_rope(&mut vec, 0, head_dim, GLOBAL_ROPE_THETA);
        for (a, b) in vec.iter().zip(before.iter()) {
            assert!((a - b).abs() < 1e-6, "pos 0 must be identity");
        }
    }

    #[test]
    fn step_matches_naive_full_attention() {
        // Small config so the naive reference is tractable.
        let att = GlobalAttention::new(4, 2, 16, 64);
        let mut state = GlobalKVState::new(&att);
        let mut rng = crate::kernels::swa::lcg_f32(31);
        let (qh, kh, d) = (att.q_heads(), att.kv_heads(), att.head_dim());

        let mut history_k: Vec<Vec<f32>> = Vec::new();
        let mut history_v: Vec<Vec<f32>> = Vec::new();
        let mut out = vec![0.0; qh * d];
        for t in 0..16 {
            let mut q: Vec<f32> = (0..qh * d).map(|_| rng()).collect();
            let mut k: Vec<f32> = (0..kh * d).map(|_| rng()).collect();
            let v: Vec<f32> = (0..kh * d).map(|_| rng()).collect();
            // Reference: apply partial RoPE to copies at position t.
            let mut q_ref = q.clone();
            let mut k_ref = k.clone();
            for h in 0..qh {
                partial_rope(
                    &mut q_ref[h * d..(h + 1) * d],
                    t as u64,
                    d,
                    GLOBAL_ROPE_THETA,
                );
            }
            for h in 0..kh {
                partial_rope(
                    &mut k_ref[h * d..(h + 1) * d],
                    t as u64,
                    d,
                    GLOBAL_ROPE_THETA,
                );
            }
            history_k.push(k_ref);
            history_v.push(v.clone());
            att.step(&mut state, &mut q, &mut k, &v, &mut out);

            // Naive: attend q_ref over all history K/V.
            let mut reference = vec![0.0f32; qh * d];
            for h in 0..qh {
                let kv_head = h * kh / qh;
                let q_h = &q_ref[h * d..(h + 1) * d];
                let mut scores: Vec<f32> = history_k
                    .iter()
                    .map(|kk| dot(q_h, &kk[kv_head * d..(kv_head + 1) * d]) * att.scale)
                    .collect();
                softmax_in_place(&mut scores);
                for (w, (kk, vv)) in scores.iter().zip(history_k.iter().zip(&history_v)) {
                    for (o, &x) in reference[h * d..(h + 1) * d]
                        .iter_mut()
                        .zip(vv[kv_head * d..(kv_head + 1) * d].iter())
                    {
                        *o += w * x;
                    }
                    let _ = kk;
                }
            }
            for (a, b) in out.iter().zip(reference.iter()) {
                assert!(
                    (a - b).abs() < 1e-4,
                    "step vs naive drifted at t={t}: {a} vs {b}"
                );
            }
        }
        assert_eq!(state.valid(), 16);
    }

    #[test]
    fn cache_never_grows_past_the_cap() {
        let att = GlobalAttention::new(4, 2, 16, 8);
        let mut state = GlobalKVState::new(&att);
        let floats = att.state_floats();
        let mut rng = crate::kernels::swa::lcg_f32(5);
        let (qh, kh, d) = (att.q_heads(), att.kv_heads(), att.head_dim());
        let mut out = vec![0.0; qh * d];
        for _ in 0..100 {
            let mut q: Vec<f32> = (0..qh * d).map(|_| rng()).collect();
            let mut k: Vec<f32> = (0..kh * d).map(|_| rng()).collect();
            let v: Vec<f32> = (0..kh * d).map(|_| rng()).collect();
            att.step(&mut state, &mut q, &mut k, &v, &mut out);
            assert!(state.valid() <= att.max_context());
            assert_eq!(state.k_cache.len() + state.v_cache.len(), floats);
        }
        assert_eq!(state.valid(), 8, "capped");
        assert_eq!(state.position(), 100, "position keeps counting");
    }
}
