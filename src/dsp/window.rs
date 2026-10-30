//! Window functions used by the spectral fallback detector.

/// Symmetric Hann window.
pub fn hann(len: usize) -> Vec<f32> {
    if len == 0 {
        return Vec::new();
    }
    if len == 1 {
        return vec![1.0];
    }
    (0..len)
        .map(|index| {
            let phase = std::f32::consts::TAU * index as f32 / (len - 1) as f32;
            0.5 - 0.5 * phase.cos()
        })
        .collect()
}

/// Symmetric Blackman-Harris window (low sidelobes, wider main lobe).
pub fn blackman_harris(len: usize) -> Vec<f32> {
    if len == 0 {
        return Vec::new();
    }
    if len == 1 {
        return vec![1.0];
    }
    const A0: f32 = 0.35875;
    const A1: f32 = 0.48829;
    const A2: f32 = 0.14128;
    const A3: f32 = 0.01168;
    (0..len)
        .map(|index| {
            let phase = std::f32::consts::TAU * index as f32 / (len - 1) as f32;
            A0 - A1 * phase.cos() + A2 * (2.0 * phase).cos() - A3 * (3.0 * phase).cos()
        })
        .collect()
}

/// Apply a window in place.
pub fn apply_in_place(samples: &mut [f32], window: &[f32]) {
    for (sample, gain) in samples.iter_mut().zip(window) {
        *sample *= gain;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hann_endpoints_are_zero() {
        let window = hann(64);
        assert!(window[0].abs() < 1e-6);
        assert!(window[63].abs() < 1e-6);
        assert!((window[32] - 1.0).abs() < 0.01);
    }

    #[test]
    fn blackman_harris_is_bounded() {
        for value in blackman_harris(128) {
            assert!((-0.01..=1.01).contains(&value));
        }
    }
}
