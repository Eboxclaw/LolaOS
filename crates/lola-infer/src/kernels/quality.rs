//! Quality-gate instruments (SB slice, ADR-038/040). The measuring side
//! of "improvements are not always speed": KL divergence, greedy
//! token-identity, and probability drift between a candidate output and
//! a frozen reference. Used by the F1 (fma reassociation), F2 (q8 KV),
//! F5 (NEON exp) and QAD gates.

/// Kullback–Leibler divergence KL(reference ‖ candidate) over two
/// discrete distributions. Both are normalized internally; zero-mass
/// reference entries contribute nothing (0·log0 → 0 convention).
pub fn kl_divergence(reference: &[f32], candidate: &[f32]) -> f32 {
    assert_eq!(reference.len(), candidate.len());
    let (mut ref_sum, mut cand_sum) = (0.0f32, 0.0f32);
    for (&r, &c) in reference.iter().zip(candidate.iter()) {
        ref_sum += r;
        cand_sum += c;
    }
    assert!(
        ref_sum > 0.0 && cand_sum > 0.0,
        "distributions must have mass"
    );
    let mut kl = 0.0f32;
    for (&r, &c) in reference.iter().zip(candidate.iter()) {
        let p = r / ref_sum;
        let q = c / cand_sum;
        if p > 0.0 && q > 0.0 {
            kl += p * (p / q).ln();
        }
    }
    kl
}

/// Greedy token-identity: fraction of positions where argmax matches.
pub fn token_identity(reference: &[f32], candidate: &[f32], vocab: usize) -> f32 {
    assert_eq!(reference.len() % vocab, 0);
    assert_eq!(candidate.len(), reference.len());
    let steps = reference.len() / vocab;
    let mut hits = 0usize;
    for step in 0..steps {
        let r = &reference[step * vocab..(step + 1) * vocab];
        let c = &candidate[step * vocab..(step + 1) * vocab];
        let argmax = |slice: &[f32]| {
            slice
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map(|(i, _)| i)
                .unwrap_or(0)
        };
        if argmax(r) == argmax(c) {
            hits += 1;
        }
    }
    hits as f32 / steps as f32
}

/// Maximum absolute drift between paired probabilities (softmax outputs,
/// answer probabilities). Returns (index, |Δ|).
pub fn max_drift(reference: &[f32], candidate: &[f32]) -> (usize, f32) {
    assert_eq!(reference.len(), candidate.len());
    reference
        .iter()
        .zip(candidate.iter())
        .enumerate()
        .map(|(i, (&r, &c))| (i, (r - c).abs()))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or((0, 0.0))
}

/// Softmax over scores into `out` (the reference normalization used by
/// gate comparisons).
pub fn softmax_into(scores: &[f32], out: &mut [f32]) {
    assert_eq!(scores.len(), out.len());
    let max = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut sum = 0.0f32;
    for (s, o) in scores.iter().zip(out.iter_mut()) {
        *o = (*s - max).exp();
        sum += *o;
    }
    for o in out.iter_mut() {
        *o /= sum;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kl_is_zero_for_identical_distributions() {
        let p = [0.1, 0.2, 0.3, 0.4];
        assert!(kl_divergence(&p, &p).abs() < 1e-9);
    }

    #[test]
    fn kl_grows_with_divergence() {
        let reference = [0.25, 0.25, 0.25, 0.25];
        let near = [0.26, 0.24, 0.25, 0.25];
        let far = [0.7, 0.1, 0.1, 0.1];
        let kl_near = kl_divergence(&reference, &near);
        let kl_far = kl_divergence(&reference, &far);
        assert!(kl_near < kl_far);
        assert!(kl_near < 1e-3);
        assert!(kl_far > 0.1);
    }

    #[test]
    fn token_identity_counts_argmax_matches() {
        // Two steps of a 4-wide vocab: first matches, second doesn't.
        let reference = [0.1, 0.6, 0.2, 0.1, 0.3, 0.3, 0.3, 0.1];
        let candidate = [0.2, 0.5, 0.2, 0.1, 0.1, 0.1, 0.1, 0.7];
        assert!((token_identity(&reference, &candidate, 4) - 0.5).abs() < 1e-9);
        assert_eq!(token_identity(&reference, &reference, 4), 1.0);
    }

    #[test]
    fn max_drift_finds_the_worst_pair() {
        let reference = [0.2, 0.5, 0.3];
        let candidate = [0.2, 0.41, 0.33];
        let (idx, drift) = max_drift(&reference, &candidate);
        assert_eq!(idx, 1, "unique worst position: drifts 0.0, 0.09, 0.03");
        assert!((drift - 0.09).abs() < 1e-6);
    }

    #[test]
    fn softmax_normalizes() {
        let scores = [1.0, 2.0, 3.0];
        let mut out = vec![0.0; 3];
        softmax_into(&scores, &mut out);
        let sum: f32 = out.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6);
        assert!(out[2] > out[1] && out[1] > out[0]);
    }
}
