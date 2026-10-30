//! Persisted application settings.

use crate::config;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// The reference pitch can be tuned across the range used by historical and
/// modern instruments.
pub const MIN_A4: f32 = 415.0;
pub const MAX_A4: f32 = 466.0;
pub const MIN_SMOOTHING: f32 = 0.05;
pub const MAX_SMOOTHING: f32 = 0.5;
pub const MIN_BUFFER_SIZE: usize = 1024;
pub const MAX_BUFFER_SIZE: usize = 8192;

/// User-visible settings. Unknown or missing TOML keys fall back to defaults so
/// that configuration files remain forwards compatible.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Frequency of A4 in Hz.
    pub reference_a4: f32,
    /// Name of the selected instrument profile.
    pub profile: String,
    /// Exponential-moving-average coefficient for pitch inertia. Lower values
    /// produce a slower, steadier needle.
    pub smoothing: f32,
    /// Number of samples used for one pitch estimate.
    pub buffer_size: usize,
    /// Preferred audio host (for example `ALSA`, `JACK`, `CoreAudio`, `WASAPI`).
    pub host: Option<String>,
    /// Preferred input device name.
    pub device: Option<String>,
    /// Request a fixed sample rate from the device when possible.
    pub preferred_sample_rate: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            reference_a4: 440.0,
            profile: "Generic".to_string(),
            smoothing: 0.25,
            buffer_size: 4096,
            host: None,
            device: None,
            preferred_sample_rate: 48_000,
        }
    }
}

impl Settings {
    /// Load settings from the standard configuration path.
    pub fn load() -> Self {
        match config::settings_path() {
            Some(path) => Self::load_from(&path),
            None => {
                log::warn!("could not determine a configuration directory; using defaults");
                Self::default()
            }
        }
    }

    /// Load settings from an explicit path. Missing or invalid files yield
    /// defaults rather than preventing the tuner from starting.
    pub fn load_from(path: &Path) -> Self {
        let Ok(text) = fs::read_to_string(path) else {
            return Self::default();
        };
        match toml::from_str::<Self>(&text) {
            Ok(mut settings) => {
                settings.sanitize();
                settings
            }
            Err(error) => {
                log::warn!(
                    "could not parse {}: {error}; using defaults",
                    path.display()
                );
                Self::default()
            }
        }
    }

    /// Persist settings to the standard configuration path.
    pub fn save(&self) -> Result<(), String> {
        let path =
            config::settings_path().ok_or_else(|| "no configuration directory".to_string())?;
        self.save_to(&path)
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let mut cleaned = self.clone();
        cleaned.sanitize();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        }
        let text = toml::to_string_pretty(&cleaned)
            .map_err(|error| format!("could not serialise settings: {error}"))?;
        fs::write(path, text)
            .map_err(|error| format!("could not write {}: {error}", path.display()))
    }

    /// Keep all values in safe, musically useful ranges.
    pub fn sanitize(&mut self) {
        if !self.reference_a4.is_finite() {
            self.reference_a4 = 440.0;
        }
        self.reference_a4 = self.reference_a4.clamp(MIN_A4, MAX_A4);

        if !self.smoothing.is_finite() {
            self.smoothing = 0.25;
        }
        self.smoothing = self.smoothing.clamp(MIN_SMOOTHING, MAX_SMOOTHING);

        self.buffer_size = self.buffer_size.clamp(MIN_BUFFER_SIZE, MAX_BUFFER_SIZE);
        if !matches!(self.preferred_sample_rate, 44_100 | 48_000) {
            self.preferred_sample_rate = 48_000;
        }
        if self.profile.trim().is_empty() {
            self.profile = "Generic".to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_clamps_out_of_range_values() {
        let mut settings = Settings {
            reference_a4: 900.0,
            smoothing: 0.0,
            buffer_size: 10,
            preferred_sample_rate: 96_000,
            ..Settings::default()
        };
        settings.sanitize();
        assert_eq!(settings.reference_a4, MAX_A4);
        assert_eq!(settings.smoothing, MIN_SMOOTHING);
        assert_eq!(settings.buffer_size, MIN_BUFFER_SIZE);
        assert_eq!(settings.preferred_sample_rate, 48_000);
    }

    #[test]
    fn toml_round_trip() {
        let settings = Settings::default();
        let text = toml::to_string(&settings).unwrap();
        let parsed: Settings = toml::from_str(&text).unwrap();
        assert!((parsed.reference_a4 - 440.0).abs() < f32::EPSILON);
        assert_eq!(parsed.profile, "Generic");
    }
}
