//! Pitch detection: YIN with McLeod Pitch Method and FFT fallbacks.
//!
//! YIN is the primary detector because it is robust on plucked strings and
//! rejects octave errors well. MPM is tried when YIN is not confident, and an
//! FFT peak with parabolic interpolation is the final fallback for signals that
//! defeat both time-domain methods.

use crate::dsp::window;
use crate::dsp::yin::{self, detect_yin_inner};
use rustfft::{num_complex::Complex32, FftPlanner};
use std::fmt;

pub const DEFAULT_MIN_FREQUENCY: f32 = 25.0;
pub const DEFAULT_MAX_FREQUENCY: f32 = 5000.0;

pub use crate::dsp::yin::{detect_yin, detect_yin_with};

/// How the winning pitch estimate was produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PitchMethod {
    Yin,
    Mpm,
    Fft,
}

impl PitchMethod {
    pub fn short_name(self) -> &'static str {
        match self {
            Self::Yin => "YIN",
            Self::Mpm => "MPM",
            Self::Fft => "FFT",
        }
    }
}

impl fmt::Display for PitchMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.short_name())
    }
}

/// A detected pitch and a confidence in the range 0..=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PitchEstimate {
    pub frequency: f32,
    pub confidence: f32,
    pub method: PitchMethod,
}

/// Root-mean-square level of a buffer.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|sample| sample * sample).sum();
    (sum / samples.len() as f32).sqrt()
}

/// Detect the fundamental frequency of a mono buffer.
///
/// `sample_rate` is in Hz and `samples` should contain a recent contiguous
/// window. Returns `None` for silence, noise, or buffers that are too short.
pub fn detect(samples: &[f32], sample_rate: f32) -> Option<PitchEstimate> {
    if samples.len() < 128 || !sample_rate.is_finite() || sample_rate <= 0.0 {
        return None;
    }
    if rms(samples) < 1.0e-4 {
        return None;
    }

    let centered = yin::remove_mean(samples);
    let yin = detect_yin_inner(&centered, sample_rate, 0.15);
    if let Some(estimate) = yin {
        if estimate.confidence >= 0.70 {
            return Some(estimate);
        }
    }

    let mpm = detect_mpm_inner(&centered, sample_rate);
    let fft = detect_fft(samples, sample_rate);

    [yin, mpm, fft]
        .into_iter()
        .flatten()
        .max_by(|left, right| {
            left.confidence
                .partial_cmp(&right.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .filter(|estimate| estimate.confidence >= 0.20)
}

/// McLeod Pitch Method detection on a raw buffer.
pub fn detect_mpm(samples: &[f32], sample_rate: f32) -> Option<PitchEstimate> {
    if samples.len() < 128 || !sample_rate.is_finite() || sample_rate <= 0.0 {
        return None;
    }
    detect_mpm_inner(&yin::remove_mean(samples), sample_rate)
}

fn detect_mpm_inner(samples: &[f32], sample_rate: f32) -> Option<PitchEstimate> {
    let n = samples.len();
    let (tau_min, tau_max) = yin::lag_limits(sample_rate, n)?;

    // Normalised square difference function.
    let mut nsdf = vec![0.0_f32; tau_max + 1];
    for tau in 0..=tau_max {
        let limit = n - tau;
        let mut autocorrelation = 0.0_f64;
        let mut norm = 0.0_f64;
        for index in 0..limit {
            let left = samples[index] as f64;
            let right = samples[index + tau] as f64;
            autocorrelation += left * right;
            norm += left * left + right * right;
        }
        nsdf[tau] = if norm > 1.0e-12 {
            (2.0 * autocorrelation / norm) as f32
        } else {
            0.0
        };
    }

    // Skip the initial falling lobe (the zero-lag maximum) so that the first
    // real period is not mistaken for a quarter-period pitch.
    let first_negative = (1..=tau_max).find(|&tau| nsdf[tau] < 0.0).unwrap_or(0);
    let mut tau = first_negative.max(tau_min);
    let mut peaks: Vec<(usize, f32)> = Vec::new();

    while tau <= tau_max {
        while tau <= tau_max && nsdf[tau] <= 0.0 {
            tau += 1;
        }
        if tau > tau_max {
            break;
        }
        let mut best_tau = tau;
        let mut best_value = nsdf[tau];
        while tau <= tau_max && nsdf[tau] > 0.0 {
            if nsdf[tau] > best_value {
                best_value = nsdf[tau];
                best_tau = tau;
            }
            tau += 1;
        }
        if best_value > 0.05 {
            peaks.push((best_tau, best_value));
        }
    }

    let key_max = peaks
        .iter()
        .map(|(_, value)| *value)
        .fold(0.0_f32, f32::max);
    if key_max <= 0.0 {
        return None;
    }
    let threshold = key_max * 0.9;
    let (tau, value) = peaks
        .iter()
        .copied()
        .find(|(_, value)| *value >= threshold)
        .or_else(|| {
            peaks.iter().copied().max_by(|left, right| {
                left.1
                    .partial_cmp(&right.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        })?;

    let refined_tau = if tau > 0 && tau + 1 < nsdf.len() {
        let before = nsdf[tau - 1];
        let centre = nsdf[tau];
        let after = nsdf[tau + 1];
        let denominator = before - 2.0 * centre + after;
        let offset = if denominator.abs() > 1.0e-12 {
            (0.5 * (before - after) / denominator).clamp(-0.5, 0.5)
        } else {
            0.0
        };
        tau as f32 + offset
    } else {
        tau as f32
    };

    if refined_tau <= 0.0 {
        return None;
    }
    let frequency = sample_rate / refined_tau;
    if !(DEFAULT_MIN_FREQUENCY..=DEFAULT_MAX_FREQUENCY).contains(&frequency) {
        return None;
    }

    Some(PitchEstimate {
        frequency,
        confidence: value.clamp(0.0, 1.0),
        method: PitchMethod::Mpm,
    })
}

/// FFT peak fallback with Hann windowing and parabolic interpolation.
pub fn detect_fft(samples: &[f32], sample_rate: f32) -> Option<PitchEstimate> {
    if samples.len() < 128 || !sample_rate.is_finite() || sample_rate <= 0.0 {
        return None;
    }

    let limit = samples.len().min(8192);
    let n = if limit.is_power_of_two() {
        limit
    } else {
        limit.next_power_of_two() / 2
    };
    if n < 128 {
        return None;
    }

    let start = samples.len() - n;
    let mean = samples[start..].iter().sum::<f32>() / n as f32;
    let window = window::hann(n);
    let mut buffer: Vec<Complex32> = samples[start..]
        .iter()
        .zip(&window)
        .map(|(sample, gain)| Complex32::new((sample - mean) * gain, 0.0))
        .collect();

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    fft.process(&mut buffer);

    let min_bin = ((DEFAULT_MIN_FREQUENCY * n as f32 / sample_rate).floor() as usize).max(1);
    let max_bin = ((DEFAULT_MAX_FREQUENCY * n as f32 / sample_rate).ceil() as usize)
        .min(n / 2 - 2)
        .max(min_bin + 1);
    if max_bin <= min_bin + 1 {
        return None;
    }

    let magnitude = |index: usize| buffer[index].norm();
    let mut peak_bin = min_bin;
    let mut peak_magnitude = 0.0_f32;
    let mut total_magnitude = 0.0_f32;
    for index in min_bin..=max_bin {
        let value = magnitude(index);
        total_magnitude += value;
        if value > peak_magnitude {
            peak_magnitude = value;
            peak_bin = index;
        }
    }
    if peak_magnitude <= 1.0e-9 || total_magnitude <= 0.0 {
        return None;
    }

    let before = magnitude(peak_bin - 1).max(1.0e-12).ln();
    let centre = magnitude(peak_bin).max(1.0e-12).ln();
    let after = magnitude(peak_bin + 1).max(1.0e-12).ln();
    let denominator = before - 2.0 * centre + after;
    let offset = if denominator.abs() > 1.0e-12 {
        (0.5 * (before - after) / denominator).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    let frequency = (peak_bin as f32 + offset) * sample_rate / n as f32;
    if !(DEFAULT_MIN_FREQUENCY..=DEFAULT_MAX_FREQUENCY).contains(&frequency) {
        return None;
    }

    Some(PitchEstimate {
        frequency,
        confidence: (peak_magnitude / total_magnitude * 8.0).clamp(0.0, 1.0),
        method: PitchMethod::Fft,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(frequency: f32, sample_rate: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|index| {
                (std::f32::consts::TAU * frequency * index as f32 / sample_rate).sin() * 0.4
            })
            .collect()
    }

    #[test]
    fn yin_detects_a4() {
        let samples = sine(440.0, 48_000.0, 4096);
        let estimate = detect_yin(&samples, 48_000.0).unwrap();
        assert!((estimate.frequency - 440.0).abs() < 1.0, "{estimate:?}");
        assert!(estimate.confidence > 0.9);
    }

    #[test]
    fn detects_low_bass_e() {
        let samples = sine(41.2034, 48_000.0, 4096);
        let estimate = detect(&samples, 48_000.0).unwrap();
        let cents = 1200.0 * (estimate.frequency / 41.2034).log2();
        assert!(cents.abs() < 4.0, "{estimate:?} ({cents} cents)");
    }

    #[test]
    fn detects_harmonic_rich_tone() {
        let mut samples = Vec::with_capacity(4096);
        for index in 0..4096 {
            let t = index as f32 / 48_000.0;
            let phase = std::f32::consts::TAU * 110.0 * t;
            samples.push(0.4 * phase.sin() + 0.2 * (2.0 * phase).sin() + 0.1 * (3.0 * phase).sin());
        }
        let estimate = detect(&samples, 48_000.0).unwrap();
        let cents = 1200.0 * (estimate.frequency / 110.0).log2();
        assert!(cents.abs() < 5.0, "{estimate:?} ({cents} cents)");
    }

    #[test]
    fn silence_is_rejected() {
        let silence = vec![0.0; 4096];
        assert!(detect(&silence, 48_000.0).is_none());
        assert!(detect_yin(&silence, 48_000.0).is_none());
        assert!(detect_mpm(&silence, 48_000.0).is_none());

        let dc = vec![0.25; 4096];
        assert!(detect(&dc, 48_000.0).is_none());
        assert!(detect_yin(&dc, 48_000.0).is_none());
    }

    #[test]
    fn fft_fallback_reports_nearby_frequency() {
        let samples = sine(440.0, 48_000.0, 4096);
        let estimate = detect_fft(&samples, 48_000.0).unwrap();
        assert!((estimate.frequency - 440.0).abs() < 3.0, "{estimate:?}");
    }
}
