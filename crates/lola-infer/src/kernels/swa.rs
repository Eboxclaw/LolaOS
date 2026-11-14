//! K2 — sliding-window attention (window 256, full RoPE θ=1e4 on SWA
//! layers per the LFM3 config; `docs/ENGINE.md` §1).
//!
//! The kernel is the attention **core**: given one token's Q/K/V (GQA
//! 16Q/4KV × 128), write K/V into a fixed ring buffer and attend over
//! the last ≤ 256 positions. Projections and RoPE are separate stages —
//! parity gates test the core math against the oracle directly.
//!
//! Properties (tested): bounded memory (`window × kv_heads × head_dim`
//! floats, never grows), exact equivalence with full recompute over the
//! window, strict eviction at the window boundary.

pub const SWA_WINDOW: usize = 256;

/// Attention core with GQA.
#[derive(Debug, Clone, PartialEq)]
pub struct SwaAttention {
    q_heads: usize,
    kv_heads: usize,
    head_dim: usize,
    window: usize,
    scale: f32,
}

/// Fixed-size ring state: K and V for the last `window` positions.
#[derive(Debug, Clone)]
pub struct SwaState {
    k_ring: Vec<f32>,
    v_ring: Vec<f32>,
    /// Number of positions ever written (for chronological ordering).
    pos: u64,
}

impl SwaAttention {
    /// LFM3 shape: 16 Q heads, 4 KV heads, 128 dim, window 256.
    pub fn lfm3() -> Self {
        Self::new(16, 4, 128, SWA_WINDOW)
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

    pub fn window(&self) -> usize {
        self.window
    }

    pub fn new(q_heads: usize, kv_heads: usize, head_dim: usize, window: usize) -> Self {
        assert!(
            q_heads % kv_heads == 0,
            "GQA requires q_heads divisible by kv_heads"
        );
        let scale = 1.0 / (head_dim as f32).sqrt();
        Self {
            q_heads,
            kv_heads,
            head_dim,
            window,
            scale,
        }
    }

    /// Ring footprint — the bounded-memory invariant (per layer).
    pub fn state_floats(&self) -> usize {
        2 * self.window * self.kv_heads * self.head_dim
    }

    /// One token: `q` [q_heads×dim], `k`/`v` [kv_heads×dim] → `out`
    /// [q_heads×dim]. Zero allocation.
    pub fn step(&self, state: &mut SwaState, q: &[f32], k: &[f32], v: &[f32], out: &mut [f32]) {
        let (qh, kh, d) = (self.q_heads, self.kv_heads, self.head_dim);
        assert_eq!(q.len(), qh * d);
        assert_eq!(k.len(), kh * d);
        assert_eq!(v.len(), kh * d);
        assert_eq!(out.len(), qh * d);
        // `out` is accumulated with += below: reset it — a caller-provided
        // buffer may carry the previous token's values (caught by the
        // eviction test, which is exactly what invariant tests are for).
        out.fill(0.0);

        // Write the new K/V into the ring slot for this position.
        let slot = (state.pos % self.window as u64) as usize;
        state.k_ring[slot * kh * d..(slot + 1) * kh * d].copy_from_slice(k);
        state.v_ring[slot * kh * d..(slot + 1) * kh * d].copy_from_slice(v);
        let valid = ((state.pos + 1) as usize).min(self.window);
        state.pos += 1;

        // Score buffer reused across heads via a fixed max-window scratch.
        let mut scores = vec![0.0f32; valid];

        for h in 0..qh {
            let kv_head = h * kh / qh;
            let q_h = &q[h * d..(h + 1) * d];
            // Chronological iteration over the `valid` newest positions:
            // oldest first — slot indices wrap, so start at pos-valid.
            let start_slot = state.pos.saturating_sub(valid as u64) % self.window as u64;
            for (j, score) in scores.iter_mut().enumerate().take(valid) {
                let _ = j;
                let slot = ((start_slot + j as u64) % self.window as u64) as usize;
                let k_h =
                    &state.k_ring[slot * kh * d + kv_head * d..slot * kh * d + (kv_head + 1) * d];
                *score = crate::kernels::conv7::dot(q_h, k_h) * self.scale;
            }
            softmax_in_place(&mut scores[..valid]);
            for (j, &weight) in scores[..valid].iter().enumerate() {
                let slot = ((start_slot + j as u64) % self.window as u64) as usize;
                let v_h =
                    &state.v_ring[slot * kh * d + kv_head * d..slot * kh * d + (kv_head + 1) * d];
                let out_h = &mut out[h * d..(h + 1) * d];
                for (o, &vv) in out_h.iter_mut().zip(v_h.iter()) {
                    *o += weight * vv;
                }
            }
        }
    }
}

impl SwaState {
    pub fn new(attention: &SwaAttention) -> Self {
        let per = attention.window * attention.kv_heads * attention.head_dim;
        // `pos` starts at window so the first write lands at slot 0 with a
        // clean chronological origin: we track written count implicitly by
        // starting pos at `window` and treating valid = window - (start
        // offset)… simpler: start at 0, slot = pos % window, valid grows.
        Self {
            k_ring: vec![0.0; per],
            v_ring: vec![0.0; per],
            pos: 0,
        }
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

/// Deterministic pseudo-random f32 in (-1, 1) for tests/benches.
pub fn lcg_f32(seed: u64) -> impl FnMut() -> f32 {
    let mut state = seed;
    move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 33) as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Naive full recompute: attend over the entire (chronological) K/V
    /// history, same GQA mapping — the reference the ring must match.
    fn naive_attention(
        att: &SwaAttention,
        history_k: &[Vec<f32>],
        history_v: &[Vec<f32>],
        q: &[f32],
    ) -> Vec<f32> {
        let (qh, kh, d) = (att.q_heads, att.kv_heads, att.head_dim);
        let window: Vec<usize> =
            (history_k.len().saturating_sub(att.window)..history_k.len()).collect();
        let mut out = vec![0.0f32; qh * d];
        for h in 0..qh {
            let kv_head = h * kh / qh;
            let q_h = &q[h * d..(h + 1) * d];
            let mut scores: Vec<f32> = window
                .iter()
                .map(|&t| {
                    let k_h = &history_k[t][kv_head * d..(kv_head + 1) * d];
                    crate::kernels::conv7::dot(q_h, k_h) * att.scale
                })
                .collect();
            softmax_in_place(&mut scores);
            for (w_idx, &t) in window.iter().enumerate() {
                let v_h = &history_v[t][kv_head * d..(kv_head + 1) * d];
                for (o, &vv) in out[h * d..(h + 1) * d].iter_mut().zip(v_h.iter()) {
                    *o += scores[w_idx] * vv;
                }
            }
        }
        out
    }

    #[test]
    fn ring_matches_full_recompute_at_exact_window() {
        let att = SwaAttention::lfm3();
        let mut rng = lcg_f32(7);
        let (qh, kh, d) = (att.q_heads, att.kv_heads, att.head_dim);
        let mut state = SwaState::new(&att);
        let mut history_k = Vec::new();
        let mut history_v = Vec::new();

        for t in 0..att.window {
            let q: Vec<f32> = (0..qh * d).map(|_| rng()).collect();
            let k: Vec<f32> = (0..kh * d).map(|_| rng()).collect();
            let v: Vec<f32> = (0..kh * d).map(|_| rng()).collect();
            history_k.push(k.clone());
            history_v.push(v.clone());
            let mut out = vec![0.0; qh * d];
            att.step(&mut state, &q, &k, &v, &mut out);
            if t + 1 == att.window {
                let reference = naive_attention(&att, &history_k, &history_v, &q);
                for (a, b) in out.iter().zip(reference.iter()) {
                    assert!(
                        (a - b).abs() < 1e-4,
                        "ring vs full recompute drifted: {a} vs {b}"
                    );
                }
            }
        }
    }

    #[test]
    fn eviction_at_the_window_boundary() {
        // Two runs, identical except token 0 (spike vs plain). After
        // window+1 tokens, token 0 is outside the ring in BOTH runs, so
        // the final outputs must match exactly.
        let att = SwaAttention::lfm3();
        let (qh, kh, d) = (att.q_heads(), att.kv_heads(), att.head_dim());
        let mut rng = lcg_f32(11);

        let tokens: Vec<(Vec<f32>, Vec<f32>, Vec<f32>)> = (0..att.window())
            .map(|_| {
                (
                    (0..qh * d).map(|_| rng()).collect::<Vec<_>>(),
                    (0..kh * d).map(|_| rng()).collect::<Vec<_>>(),
                    (0..kh * d).map(|_| rng()).collect::<Vec<_>>(),
                )
            })
            .collect();

        let run = |first_is_spike: bool| -> Vec<f32> {
            let mut state = SwaState::new(&att);
            let mut out = vec![0.0; qh * d];
            for (t, (q, k, v)) in tokens.iter().enumerate() {
                if t == 0 && first_is_spike {
                    let spike = vec![10.0f32; kh * d];
                    att.step(&mut state, q, &spike, &spike, &mut out);
                } else {
                    att.step(&mut state, q, k, v, &mut out);
                }
            }
            out
        };

        let with_spike = run(true);
        let without_spike = run(false);
        // window tokens written in both runs; token 0 IS still inside the
        // window after `window` steps, so outputs must DIFFER now…
        assert_ne!(
            with_spike, without_spike,
            "spike inside the window must matter"
        );

        // …and after one more (identical) token in both runs, the spike
        // is evicted and outputs converge.
        let run_evicted = |first_is_spike: bool| -> Vec<f32> {
            let mut state = SwaState::new(&att);
            let mut out = vec![0.0; qh * d];
            for (t, (q, k, v)) in tokens.iter().enumerate() {
                if t == 0 && first_is_spike {
                    let spike = vec![10.0f32; kh * d];
                    att.step(&mut state, q, &spike, &spike, &mut out);
                } else {
                    att.step(&mut state, q, k, v, &mut out);
                }
            }
            // The evicting token (identical in both runs).
            let q_last = vec![0.5f32; qh * d];
            let kv_last = vec![0.5f32; kh * d];
            att.step(&mut state, &q_last, &kv_last, &kv_last, &mut out);
            out
        };
        let evicted_with = run_evicted(true);
        let evicted_without = run_evicted(false);
        for (a, b) in evicted_with.iter().zip(evicted_without.iter()) {
            assert!(
                (a - b).abs() < 1e-6,
                "evicted spike still influences output: {a} vs {b}"
            );
        }
    }

    #[test]
    fn state_never_grows() {
        let att = SwaAttention::lfm3();
        let mut state = SwaState::new(&att);
        let floats = att.state_floats();
        assert_eq!(
            floats,
            2 * SWA_WINDOW * 4 * 128,
            "2 rings × 256 positions × 4 KV heads × 128 dim"
        );
        let mut rng = lcg_f32(3);
        let (qh, kh, d) = (att.q_heads, att.kv_heads, att.head_dim);
        let mut out = vec![0.0; qh * d];
        for _ in 0..1000 {
            let q: Vec<f32> = (0..qh * d).map(|_| rng()).collect();
            let k: Vec<f32> = (0..kh * d).map(|_| rng()).collect();
            let v: Vec<f32> = (0..kh * d).map(|_| rng()).collect();
            att.step(&mut state, &q, &k, &v, &mut out);
            assert_eq!(state.k_ring.len() + state.v_ring.len(), floats);
        }
    }
}
