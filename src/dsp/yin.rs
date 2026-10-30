//! YIN fundamental-frequency estimation.
//!
//! The function computes the difference function, applies cumulative mean
//! normalisation, selects the first confident minimum and refines the lag with
//! parabolic interpolation.

use super::pitch::{PitchEstimate, PitchMethod, DEFAULT_MAX_FREQUENCY, DEFAULT_MIN_FREQUENCY};

/// Convert the supported frequency range into integer lag bounds.
pub(crate) fn lag_limits(sample_rate: f32, len: usize) -> Option<(usize, usize)> {
    let tau_min = ((sample_rate / DEFAULT_MAX_FREQUENCY).floor() as usize).max(2);
    let tau_max = ((sample_rate / DEFAULT_MIN_FREQUENCY).ceil() as usize).min(len / 2);
    (tau_min < tau_max && tau_max >= 2).then_some((tau_min, tau_max))
}

/// Remove the DC component before time-domain period estimation.
pub(crate) fn remove_mean(samples: &[f32]) -> Vec<f32> {
    let mean = samples.iter().sum::<f32>() / samples.len() as f32;
    samples.iter().map(|sample| sample - mean).collect()
}

/// YIN pitch detection on a raw buffer.
pub fn detect_yin(samples: &[f32], sample_rate: f32) -> Option<PitchEstimate> {
    detect_yin_with(samples, sample_rate, 0.15)
}

/// YIN with an explicit cumulative-mean-normalised-difference threshold.
pub fn detect_yin_with(samples: &[f32], sample_rate: f32, threshold: f32) -> Option<PitchEstimate> {
    if samples.len() < 128 || !sample_rate.is_finite() || sample_rate <= 0.0 {
        return None;
    }
    detect_yin_inner(&remove_mean(samples), sample_rate, threshold)
}

pub(crate) fn detect_yin_inner(
    samples: &[f32],
    sample_rate: f32,
    threshold: f32,
) -> Option<PitchEstimate> {
    let n = samples.len();
    let (tau_min, tau_max) = lag_limits(sample_rate, n)?;

    // Reject silence and pure DC: without this guard the normalised difference
    // is a flat zero and the shortest lag would look like a perfect match.
    let energy = samples.iter().map(|sample| sample * sample).sum::<f32>();
    if energy <= 1.0e-10 {
        return None;
    }

    // Difference function d(tau).
    let mut difference = vec![0.0_f32; tau_max + 1];
    for tau in 1..=tau_max {
        let limit = n - tau;
        let mut sum = 0.0_f32;
        for index in 0..limit {
            let delta = samples[index] - samples[index + tau];
            sum += delta * delta;
        }
        difference[tau] = sum;
    }

    // Cumulative mean normalised difference d'(tau).
    let mut cmnd = vec![0.0_f32; tau_max + 1];
    cmnd[0] = 1.0;
    let mut running_sum = 0.0_f32;
    for tau in 1..=tau_max {
        running_sum += difference[tau];
        cmnd[tau] = if running_sum > 1.0e-12 {
            difference[tau] * tau as f32 / running_sum
        } else {
            1.0
        };
    }

    let mut candidate = None;
    let mut tau = tau_min;
    while tau < tau_max {
        if cmnd[tau] < threshold {
            while tau + 1 < tau_max && cmnd[tau + 1] < cmnd[tau] {
                tau += 1;
            }
            candidate = Some(tau);
            break;
        }
        tau += 1;
    }

    let (tau, value) = match candidate {
        Some(tau) => (tau, cmnd[tau]),
        None => {
            let mut best_tau = tau_min;
            let mut best_value = f32::INFINITY;
            for (index, value) in cmnd.iter().enumerate().take(tau_max + 1).skip(tau_min) {
                if *value < best_value {
                    best_value = *value;
                    best_tau = index;
                }
            }
            (best_tau, best_value)
        }
    };

    if tau < 1 || tau >= cmnd.len() {
        return None;
    }

    // Parabolic interpolation around the selected minimum.
    let refined_tau = if tau > 0 && tau + 1 < cmnd.len() {
        let before = cmnd[tau - 1];
        let centre = cmnd[tau];
        let after = cmnd[tau + 1];
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
        confidence: (1.0 - value).clamp(0.0, 1.0),
        method: PitchMethod::Yin,
    })
}
