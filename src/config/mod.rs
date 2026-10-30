//! Configuration and instrument-profile persistence.

pub mod profiles;
pub mod settings;

use directories::BaseDirs;
use std::path::PathBuf;

/// Directory in which settings and custom profiles are stored.
///
/// On Linux this is `~/.config/pitchperfect`, on macOS
/// `~/Library/Application Support/pitchperfect`, and on Windows
/// `%APPDATA%\pitchperfect`.
pub fn config_dir() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.config_dir().join("pitchperfect"))
}

pub fn settings_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("settings.toml"))
}

pub fn profiles_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("profiles.toml"))
}
