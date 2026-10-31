//! Configuration and instrument-profile persistence.

pub mod profiles;
pub mod settings;

use directories::BaseDirs;
use std::path::PathBuf;

/// Directory in which settings and custom profiles are stored.
///
/// On Linux this is `~/.config/frequency`, on macOS
/// `~/Library/Application Support/frequency`, and on Windows
/// `%APPDATA%\frequency`.
pub fn config_dir() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.config_dir().join("frequency"))
}

pub fn settings_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("settings.toml"))
}

pub fn profiles_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("profiles.toml"))
}
