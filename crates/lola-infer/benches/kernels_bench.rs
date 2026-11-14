//! S3 kernel benchmarks — the first engine numbers (K1: gated conv-7).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lola_infer::kernels::conv7::{lcg, ConvState, GatedConvKernel, GatedDepthwiseConv7, KERNEL};

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
}

criterion_group!(benches, bench_kernels);
criterion_main!(benches);
