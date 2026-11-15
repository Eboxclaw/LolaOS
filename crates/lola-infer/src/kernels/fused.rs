//! K4 — fused elementwise kernels: head-wise gate + sandwich norm
//! (`docs/ENGINE.md` §1). Cheap individually; the win is fusing them into
//! neighbors so tensors never round-trip memory. They are exposed as
//! standalone, invariant-tested kernels and compose at the layer level in
//! the K6 scheduler (attn → head gate → post-norm).

/// Head-wise sigmoid gate on attention output: `out_h *= sigmoid(g_h)`.
/// Near-free (one exp per head), but standalone it would cost a full
/// tensor round-trip — the K6 scheduler fuses it into the attention
/// output write.
#[inline]
pub fn apply_head_gate(out: &mut [f32], gate_logits: &[f32], head_dim: usize) {
    assert_eq!(out.len() % head_dim, 0);
    assert_eq!(gate_logits.len(), out.len() / head_dim);
    for (h, &g) in gate_logits.iter().enumerate() {
        let gate = 1.0 / (1.0 + (-g).exp());
        for x in &mut out[h * head_dim..(h + 1) * head_dim] {
            *x *= gate;
        }
    }
}

/// RMSNorm: x * weight / sqrt(mean(x²) + eps).
#[inline]
pub fn rmsnorm(x: &mut [f32], weight: &[f32], eps: f32) {
    assert_eq!(x.len(), weight.len());
    let mean_sq = x.iter().map(|&v| v * v).sum::<f32>() / x.len() as f32;
    let inv = 1.0 / (mean_sq + eps).sqrt();
    for (v, &w) in x.iter_mut().zip(weight.iter()) {
        *v = *v * inv * w;
    }
}

/// Sandwich norm: pre- and post-norm per block (LFM3: `use_sandwich_norm:
/// true`). Applied in place; each stage carries its own weight vector.
#[derive(Debug, Clone, PartialEq)]
pub struct SandwichNorm {
    pub pre_weight: Vec<f32>,
    pub post_weight: Vec<f32>,
    pub eps: f32,
}

impl SandwichNorm {
    pub fn identity(width: usize) -> Self {
        Self {
            pre_weight: vec![1.0; width],
            post_weight: vec![1.0; width],
            eps: 1e-5,
        }
    }

    /// Pre-norm the block input.
    pub fn pre(&self, x: &mut [f32]) {
        rmsnorm(x, &self.pre_weight, self.eps);
    }

    /// Post-norm the block output.
    pub fn post(&self, x: &mut [f32]) {
        rmsnorm(x, &self.post_weight, self.eps);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_gate_identity_when_logits_are_large() {
        let mut out = vec![1.0f32, -2.0, 3.0, 4.0];
        apply_head_gate(&mut out, &[20.0, 20.0], 2); // sigmoid(20) ≈ 1
        assert!((out[0] - 1.0).abs() < 1e-6);
        assert!((out[3] - 4.0).abs() < 1e-5);
    }

    #[test]
    fn head_gate_suppression_is_per_head() {
        let mut out = vec![1.0f32, 1.0, 1.0, 1.0];
        apply_head_gate(&mut out, &[-20.0, 20.0], 2); // head 0 off, head 1 on
        assert!(out[0].abs() < 1e-6 && out[1].abs() < 1e-6);
        assert!((out[2] - 1.0).abs() < 1e-6 && (out[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rmsnorm_unit_rms_with_identity_weight() {
        let mut x = vec![3.0f32, -4.0, 12.0, 0.5];
        let w = vec![1.0f32; 4];
        rmsnorm(&mut x, &w, 1e-5);
        let rms = (x.iter().map(|&v| v * v).sum::<f32>() / x.len() as f32).sqrt();
        assert!((rms - 1.0).abs() < 1e-3, "rms={rms}");
    }

    #[test]
    fn rmsnorm_zero_weight_gives_zero_and_extremes_do_not_overflow() {
        let mut x = vec![1e8f32, -1e8];
        rmsnorm(&mut x, &[0.0, 0.0], 1e-5);
        assert!(x.iter().all(|&v| v == 0.0));

        let mut big = vec![3e38f32, 3e38]; // near-f32-max: squares overflow naive paths
        rmsnorm(&mut big, &[1.0, 1.0], 1e-5);
        assert!(
            big.iter().all(|v| v.is_finite()),
            "must not produce NaN/inf"
        );
    }

    #[test]
    fn sandwich_identity_preserves_direction() {
        let norm = SandwichNorm::identity(8);
        let input = vec![2.0f32, -1.0, 0.5, 3.0, -2.0, 1.0, 0.0, 4.0];
        let mut x = input.clone();
        norm.pre(&mut x);
        // Same sign pattern, different scale (unit-RMS).
        for (a, b) in x.iter().zip(input.iter()) {
            assert_eq!(a.signum(), b.signum());
        }
        norm.post(&mut x);
        for (a, b) in x.iter().zip(input.iter()) {
            assert_eq!(a.signum(), b.signum());
        }
    }
}
