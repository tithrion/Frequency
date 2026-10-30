//! DSP and pitch-detection integration tests.

use pitchperfect::dsp::pitch::{detect, detect_fft, detect_mpm, detect_yin};
use pitchperfect::dsp::window;
use pitchperfect::music::{cents_between, Note};

fn sine(frequency: f32, sample_rate: f32, len: usize) -> Vec<f32> {
    (0..len)
        .map(|index| (std::f32::consts::TAU * frequency * index as f32 / sample_rate).sin() * 0.5)
        .collect()
}

fn harmonic(fundamental: f32, sample_rate: f32, len: usize) -> Vec<f32> {
    (0..len)
        .map(|index| {
            let t = index as f32 / sample_rate;
            let phase = std::f32::consts::TAU * fundamental * t;
            0.4 * phase.sin() + 0.18 * (2.0 * phase).sin() + 0.09 * (3.0 * phase).sin()
        })
        .collect()
}

#[test]
fn detects_concert_a_at_both_common_sample_rates() {
    for sample_rate in [44_100.0, 48_000.0] {
        let samples = sine(440.0, sample_rate, 4096);
        let estimate = detect(&samples, sample_rate).expect("A4 should be detected");
        let cents = cents_between(estimate.frequency, 440.0);
        assert!(
            cents.abs() < 2.0,
            "{sample_rate}: {estimate:?} = {cents} cents"
        );
        assert!(estimate.confidence > 0.7, "{estimate:?}");
    }
}

#[test]
fn detects_low_e_bass_with_4096_sample_window() {
    let e1 = Note::parse("E1").unwrap().frequency(440.0);
    let samples = sine(e1, 48_000.0, 4096);
    let estimate = detect(&samples, 48_000.0).expect("E1 should be detected");
    let cents = cents_between(estimate.frequency, e1);
    assert!(cents.abs() < 5.0, "{estimate:?} = {cents} cents");
}

#[test]
fn detects_low_a0_at_both_common_sample_rates() {
    let a0 = Note::parse("A0").unwrap().frequency(440.0);
    for sample_rate in [44_100.0, 48_000.0] {
        let samples = sine(a0, sample_rate, 4096);
        let estimate = detect(&samples, sample_rate).expect("A0 should be detected");
        let cents = cents_between(estimate.frequency, a0);
        assert!(
            cents.abs() < 10.0,
            "{sample_rate}: {estimate:?} = {cents} cents"
        );
    }
}

#[test]
fn detects_each_standard_guitar_string() {
    let sample_rate = 48_000.0;
    for note in ["E2", "A2", "D3", "G3", "B3", "E4"] {
        let frequency = Note::parse(note).unwrap().frequency(440.0);
        let samples = harmonic(frequency, sample_rate, 4096);
        let estimate = detect(&samples, sample_rate).expect("guitar note should be detected");
        let cents = cents_between(estimate.frequency, frequency);
        assert!(cents.abs() < 5.0, "{note}: {estimate:?} = {cents} cents");
    }
}

#[test]
fn yin_and_mpm_agree_on_a4() {
    let samples = sine(440.0, 48_000.0, 4096);
    let yin = detect_yin(&samples, 48_000.0).unwrap();
    let mpm = detect_mpm(&samples, 48_000.0).unwrap();
    assert!(cents_between(yin.frequency, 440.0).abs() < 2.0);
    assert!(cents_between(mpm.frequency, 440.0).abs() < 2.0);
}

#[test]
fn fft_fallback_finds_a4() {
    let samples = sine(440.0, 48_000.0, 4096);
    let estimate = detect_fft(&samples, 48_000.0).unwrap();
    assert!(cents_between(estimate.frequency, 440.0).abs() < 10.0);
}

#[test]
fn silence_and_noise_are_rejected() {
    assert!(detect(&vec![0.0; 4096], 48_000.0).is_none());

    let mut noise = Vec::with_capacity(4096);
    let mut state = 0x1234_5678_u32;
    for _ in 0..4096 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let value = ((state >> 8) as f32 / (u32::MAX >> 8) as f32) * 2.0 - 1.0;
        noise.push(value * 0.05);
    }
    let estimate = detect(&noise, 48_000.0);
    if let Some(estimate) = estimate {
        assert!(
            estimate.confidence < 0.7,
            "white noise should not be a confident pitch: {estimate:?}"
        );
    }
}

#[test]
fn windowing_helpers_are_sane() {
    let hann = window::hann(1024);
    assert_eq!(hann.len(), 1024);
    assert!(hann.iter().all(|value| (0.0..=1.0001).contains(value)));
    let mut samples = vec![1.0_f32; 1024];
    window::apply_in_place(&mut samples, &hann);
    assert!(samples[0].abs() < 1e-5);
    assert!((samples[512] - 1.0).abs() < 1e-3);
}
