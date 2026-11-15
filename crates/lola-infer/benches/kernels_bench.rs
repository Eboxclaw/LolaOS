//! S3 kernel benchmarks — the first engine numbers (K1: gated conv-7).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lola_infer::kernels::conv7::{lcg, ConvState, GatedConvKernel, GatedDepthwiseConv7, KERNEL};
use lola_infer::kernels::fused::{apply_head_gate, SandwichNorm};
use lola_infer::kernels::global_attn::{GlobalAttention, GlobalKVState};
use lola_infer::kernels::swa::{lcg_f32, SwaAttention, SwaState};

fn bench_kernels(c: &mut Criterion) {
    // LFM3-scale layer: 2048 → 2048 channels, kernel 7.
    let (in_ch, out_ch) = (2048, 2048);
    let mut rng = lcg(1234);
    let len = out_ch * in_ch * KERNEL;
    let wv: Vec<f32> = (0..len).map(|_| rng() as f32 * 0.02).collect();
    let wg: Vec<f32> = (0..len).map(|_| rng() as f32 * 0.02).collect();
    let kernel = GatedConvKernel::new(in_ch, out_ch, wv, wg);
    let mut state = ConvState::new(&kernel);
    let token: Vec<f32> = (0..in_ch).map(|_| rng() as f32).collect();

    c.bench_function("conv7_step_2048x2048", |b| {
        b.iter(|| {
            let out = kernel.step(black_box(&mut state), black_box(&token));
            black_box(out)
        })
    });

    // The architecture-correct shape: depthwise gated conv k=7.
    let dw = GatedDepthwiseConv7::new(
        2048,
        (0..2048 * KERNEL).map(|_| rng() as f32 * 0.1).collect(),
        (0..2048 * KERNEL).map(|_| rng() as f32 * 0.1).collect(),
    );
    let mut dw_state = ConvState::new(&GatedConvKernel::zeros(2048, 2048));
    let mut dw_out = vec![0.0f32; 2048];
    c.bench_function("conv7_depthwise_step_2048", |b| {
        b.iter(|| {
            dw.step_into(
                black_box(&mut dw_state),
                black_box(&token),
                black_box(&mut dw_out),
            );
            black_box(dw_out[0])
        })
    });

    // K2: SWA-256 attention core (GQA 16/4 × 128), full window.
    let att = SwaAttention::lfm3();
    let mut swa_state = SwaState::new(&att);
    let mut rng32 = lcg_f32(9);
    let (qh, kh, d) = (att.q_heads(), att.kv_heads(), att.head_dim());
    let q: Vec<f32> = (0..qh * d).map(|_| rng32()).collect();
    let k: Vec<f32> = (0..kh * d).map(|_| rng32()).collect();
    let v: Vec<f32> = (0..kh * d).map(|_| rng32()).collect();
    let mut swa_out = vec![0.0f32; qh * d];
    c.bench_function("swa_step_full_window_16x4x128", |b| {
        b.iter(|| {
            att.step(
                black_box(&mut swa_state),
                black_box(&q),
                black_box(&k),
                black_box(&v),
                black_box(&mut swa_out),
            );
            black_box(swa_out[0])
        })
    });

    // K3: global attention (partial RoPE, GQA 16/4 x 128) at 4K context.
    let gatt = GlobalAttention::lfm3(4096);
    let mut gstate = GlobalKVState::new(&gatt);
    let (gqh, gkh, gd) = (gatt.q_heads(), gatt.kv_heads(), gatt.head_dim());
    let mut gq: Vec<f32> = (0..gqh * gd).map(|_| rng32()).collect();
    let mut gk: Vec<f32> = (0..gkh * gd).map(|_| rng32()).collect();
    let gv: Vec<f32> = (0..gkh * gd).map(|_| rng32()).collect();
    let mut gout = vec![0.0f32; gqh * gd];
    // Warm the cache to the full 4K context before measuring.
    for _ in 0..gatt.max_context() {
        gatt.step(&mut gstate, &mut gq, &mut gk, &gv, &mut gout);
    }
    c.bench_function("global_attn_step_4k_16x4x128", |b| {
        b.iter(|| {
            gatt.step(
                black_box(&mut gstate),
                black_box(&mut gq),
                black_box(&mut gk),
                black_box(&gv),
                black_box(&mut gout),
            );
            black_box(gout[0])
        })
    });

    // K4 long-context proof: blocked online softmax at 64K (the shape
    // where materialized scores would cost 256 KB/head/step).
    let att64 = GlobalAttention::lfm3(65_536);
    let mut state64 = GlobalKVState::new(&att64);
    let mut q64: Vec<f32> = (0..gqh * gd).map(|_| rng32()).collect();
    let mut k64: Vec<f32> = (0..gkh * gd).map(|_| rng32()).collect();
    let v64: Vec<f32> = (0..gkh * gd).map(|_| rng32()).collect();
    let mut out64 = vec![0.0f32; gqh * gd];
    let prime_k: Vec<Vec<f32>> = (0..att64.max_context())
        .map(|_| (0..gkh * gd).map(|_| rng32()).collect())
        .collect();
    let prime_v: Vec<Vec<f32>> = (0..att64.max_context())
        .map(|_| (0..gkh * gd).map(|_| rng32()).collect())
        .collect();
    att64.prime_kv(&mut state64, &prime_k, &prime_v);
    c.bench_function("global_attn_step_64k_16x4x128", |b| {
        b.iter(|| {
            att64.step(
                black_box(&mut state64),
                black_box(&mut q64),
                black_box(&mut k64),
                black_box(&v64),
                black_box(&mut out64),
            );
            black_box(out64[0])
        })
    });

    // K4: fused elementwise kernels at 2048 width.
    let mut gate_buf = vec![0.5f32; 2048];
    let gate_logits = vec![0.7f32; 16];
    c.bench_function("head_gate_2048_16heads", |b| {
        b.iter(|| {
            apply_head_gate(black_box(&mut gate_buf), black_box(&gate_logits), 128);
            black_box(gate_buf[0])
        })
    });
    let norm = SandwichNorm::identity(2048);
    let mut norm_buf = vec![0.3f32; 2048];
    c.bench_function("sandwich_norm_pre_2048", |b| {
        b.iter(|| {
            norm.pre(black_box(&mut norm_buf));
            black_box(norm_buf[0])
        })
    });
}

criterion_group!(benches, bench_kernels);
criterion_main!(benches);
