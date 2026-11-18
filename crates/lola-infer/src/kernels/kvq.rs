//! F2 — q8_0 KV-cache quantization for the global-attention layers
//! (`docs/OPTIMIZATIONS.md` F2, ADR-038 gates: memory −73%, quality
//! drift bounded, speed measured).
//!
//! Layout (ours — GGUF-compatible in spirit, f32 scale instead of f16;
//! the GGUF import re-quantizes anyway): blocks of 32 values, one f32
//! scale per block (`d = max|x| / 127`), 127-range i8 values. Steady
//! state: 4 B/elem → 1.125 B/elem (−72%). Decode path never
//! materializes f32 KV: attention reads blocks and dequantizes in
//! registers (`dot_q8_f32`).

/// Q8_0 block element count (GGUF convention).
pub const Q8_BLOCK: usize = 32;
/// Bytes per block in our layout: f32 scale + 32 i8.
pub const Q8_BLOCK_BYTES: usize = Q8_BLOCK + 4;

/// One quantized block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Q8Block {
    pub d: f32,
    pub qs: [i8; Q8_BLOCK],
}

/// Quantize a f32 slice into whole Q8_0 blocks. Length must be a
/// multiple of [`Q8_BLOCK`].
pub fn quantize_q8_0(src: &[f32]) -> Vec<Q8Block> {
    assert!(src.len() % Q8_BLOCK == 0, "length must be block-aligned");
    src.chunks_exact(Q8_BLOCK)
        .map(|chunk| {
            let max_abs = chunk.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
            let d = if max_abs == 0.0 { 1.0 } else { max_abs / 127.0 };
            let mut qs = [0i8; Q8_BLOCK];
            for (i, &x) in chunk.iter().enumerate() {
                qs[i] = (x / d).round() as i8;
            }
            Q8Block { d, qs }
        })
        .collect()
}

/// Dequantize a block into `out` (len 32).
pub fn dequant_block(block: &Q8Block, out: &mut [f32]) {
    for (i, &q) in block.qs.iter().enumerate() {
        out[i] = q as f32 * block.d;
    }
}

/// Dot product of a quantized block against a f32 vector segment,
/// dequantizing in registers — no materialized f32 KV copy.
#[inline]
pub fn dot_q8_f32(block: &Q8Block, f32_slice: &[f32]) -> f32 {
    let mut sum = 0.0f32;
    for (i, &q) in block.qs.iter().enumerate() {
        sum += (q as f32 * block.d) * f32_slice[i];
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernels::quality::max_drift;

    #[test]
    fn round_trip_error_bounded_by_half_scale() {
        let mut rng = crate::kernels::conv7::lcg(77);
        let src: Vec<f32> = (0..Q8_BLOCK * 4).map(|_| rng() as f32 * 5.0).collect();
        let blocks = quantize_q8_0(&src);
        let mut out = vec![0.0f32; src.len()];
        for (b, chunk) in blocks.chunks(1).zip(out.chunks_mut(Q8_BLOCK)) {
            dequant_block(&b[0], chunk);
        }
        let (_, drift) = max_drift(&src, &out);
        // Max error = d/2 where d = max_abs/127; max_abs ~5 ⇒ d ~0.04 ⇒
        // drift ~0.02.
        assert!(drift < 0.05, "q8 round-trip drift {drift}");
    }

    #[test]
    fn memory_ratio_matches_the_design() {
        // 64K positions, 4 KV heads × 128 dim (one layer's K or V).
        let elems = 65_536 * 4 * 128;
        let f32_bytes = elems * 4;
        let q8_bytes = (elems / Q8_BLOCK) * Q8_BLOCK_BYTES;
        let ratio = q8_bytes as f64 / f32_bytes as f64;
        assert!(
            (ratio - 1.125 / 4.0).abs() < 0.01,
            "q8/f32 byte ratio {ratio} ≈ 0.281 (−72%)"
        );
    }

    #[test]
    fn q8_dot_matches_f32_dot_within_tolerance() {
        let mut rng = crate::kernels::conv7::lcg(13);
        let src: Vec<f32> = (0..Q8_BLOCK).map(|_| rng() as f32 * 3.0).collect();
        let other: Vec<f32> = (0..Q8_BLOCK).map(|_| rng() as f32).collect();
        let blocks = quantize_q8_0(&src);
        let q8_dot = dot_q8_f32(&blocks[0], &other);
        let f32_dot: f32 = src.iter().zip(other.iter()).map(|(&a, &b)| a * b).sum();
        assert!(
            (q8_dot - f32_dot).abs() / f32_dot.abs().max(1.0) < 0.02,
            "q8 dot {q8_dot} vs f32 {f32_dot}"
        );
    }
}
