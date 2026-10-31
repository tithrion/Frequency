//! Micro-benchmark for the detection routines.
//!
//! Run with:
//! `cargo run --release --example benchmark`

use frequency::dsp::pitch::{detect, detect_fft, detect_mpm, detect_yin};
use std::time::Instant;

fn sine(frequency: f32, sample_rate: f32, len: usize) -> Vec<f32> {
    (0..len)
        .map(|index| (std::f32::consts::TAU * frequency * index as f32 / sample_rate).sin() * 0.5)
        .collect()
}

fn bench(name: &str, iterations: usize, mut run: impl FnMut()) {
    // Warm up caches and CPU frequency scaling.
    for _ in 0..10 {
        run();
    }
    let start = Instant::now();
    for _ in 0..iterations {
        run();
    }
    let elapsed = start.elapsed();
    let per_iteration = elapsed.as_secs_f64() * 1000.0 / iterations as f64;
    println!("{name:<12} {per_iteration:8.3} ms/iteration  ({iterations} iterations)");
}

fn main() {
    let sample_rate = 48_000.0;
    let samples = sine(440.0, sample_rate, 4096);
    let iterations = 500;

    println!("Frequency detection benchmark — 4096 samples @ 48 kHz\n");
    bench("detect", iterations, || {
        std::hint::black_box(detect(&samples, sample_rate));
    });
    bench("YIN", iterations, || {
        std::hint::black_box(detect_yin(&samples, sample_rate));
    });
    bench("MPM", iterations, || {
        std::hint::black_box(detect_mpm(&samples, sample_rate));
    });
    bench("FFT", iterations, || {
        std::hint::black_box(detect_fft(&samples, sample_rate));
    });
}
