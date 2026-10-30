//! Instrument profiles: built-in tunings plus user-editable TOML profiles.

use crate::config;
use crate::music::{Note, TuningTarget};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const GENERIC_PROFILE_NAME: &str = "Generic";
pub const MIN_STRINGS: usize = 1;
pub const MAX_STRINGS: usize = 99;

/// One string or note target. Exactly one of `note` and `hz` is normally used;
/// when both are present the raw frequency overrides the note name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TargetSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hz: Option<f32>,
}

impl TargetSpec {
    pub fn from_note(note: impl Into<String>) -> Self {
        Self {
            note: Some(note.into()),
            hz: None,
        }
    }

    pub fn from_hz(hz: f32) -> Self {
        Self {
            note: None,
            hz: Some(hz),
        }
    }

    pub fn note(&self) -> Option<Note> {
        self.note.as_deref().and_then(|text| Note::parse(text).ok())
    }

    /// Resolve the target frequency, treating a raw `hz` value as an override.
    pub fn frequency(&self, a4: f32) -> Option<f32> {
        if let Some(hz) = self.hz {
            return (hz.is_finite() && hz > 0.0).then_some(hz);
        }
        self.note().map(|note| note.frequency(a4))
    }

    pub fn display(&self, _a4: f32) -> String {
        if let Some(hz) = self.hz {
            if hz.is_finite() && hz > 0.0 {
                return format!("{hz:.2} Hz");
            }
        }
        match self.note() {
            Some(note) => note.label(),
            None => self
                .note
                .clone()
                .filter(|text| !text.trim().is_empty())
                .unwrap_or_else(|| "?".to_string()),
        }
    }

    pub fn validation_error(&self) -> Option<String> {
        if let Some(hz) = self.hz {
            if !hz.is_finite() || !(1.0..=30_000.0).contains(&hz) {
                return Some("frequency must be between 1 Hz and 30000 Hz".to_string());
            }
            return None;
        }
        match self.note.as_deref() {
            None => Some("enter a note or a frequency".to_string()),
            Some(text) if text.trim().is_empty() => Some("note name is empty".to_string()),
            Some(text) => Note::parse(text)
                .err()
                .map(|_| format!("invalid note '{text}'")),
        }
    }
}

/// An instrument tuning. Built-in profiles are never written to disk; custom
/// profiles are persisted to `profiles.toml`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub strings: Vec<TargetSpec>,
    #[serde(skip)]
    pub builtin: bool,
    #[serde(skip)]
    pub generic: bool,
}

impl Profile {
    pub fn from_notes(name: impl Into<String>, notes: &[&str]) -> Self {
        Self {
            name: name.into(),
            strings: notes
                .iter()
                .map(|note| TargetSpec::from_note(*note))
                .collect(),
            builtin: false,
            generic: false,
        }
    }

    pub fn new_custom(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            strings: vec![TargetSpec::from_note("A4")],
            builtin: false,
            generic: false,
        }
    }

    pub fn is_generic(&self) -> bool {
        self.generic || self.name == GENERIC_PROFILE_NAME
    }

    pub fn duplicate_as_custom(&self, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            strings: self.strings.clone(),
            builtin: false,
            generic: false,
        }
    }

    /// Human-readable label for one string, for example `2 - A2`.
    pub fn string_label(&self, index: usize, a4: f32) -> Option<String> {
        let spec = self.strings.get(index)?;
        Some(format!("{} - {}", index + 1, spec.display(a4)))
    }

    /// Find the string/note closest to `frequency` in cents.
    pub fn target(&self, frequency: f32, a4: f32) -> Option<TuningTarget> {
        if !frequency.is_finite() || frequency <= 0.0 {
            return None;
        }

        if self.is_generic() {
            let note = Note::nearest_in_generic_range(frequency, a4)?;
            return Some(TuningTarget::generic(note, a4));
        }

        let mut best: Option<(usize, f32, f32)> = None; // index, target_hz, abs cents
        for (index, spec) in self.strings.iter().enumerate() {
            let Some(target_hz) = spec.frequency(a4) else {
                continue;
            };
            let distance = crate::music::cents_between(frequency, target_hz).abs();
            if best.is_none_or(|(_, _, previous)| distance < previous) {
                best = Some((index, target_hz, distance));
            }
        }

        if let Some((index, target_hz, _)) = best {
            let spec = &self.strings[index];
            let display = spec.display(a4);
            let note = if spec.hz.is_some() { None } else { spec.note() };
            return Some(TuningTarget {
                label: format!("{} - {}", index + 1, display),
                display,
                frequency: target_hz,
                note,
                string_index: Some(index),
            });
        }

        // A malformed profile should still leave the tuner usable.
        Note::nearest_in_generic_range(frequency, a4).map(|note| TuningTarget::generic(note, a4))
    }

    /// Validation for user-created profiles.
    pub fn validation_error(&self) -> Option<String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Some("profile name is empty".to_string());
        }
        if name == GENERIC_PROFILE_NAME {
            return Some("'Generic' is a reserved profile name".to_string());
        }
        if !(MIN_STRINGS..=MAX_STRINGS).contains(&self.strings.len()) {
            return Some(format!(
                "profile must contain between {MIN_STRINGS} and {MAX_STRINGS} strings"
            ));
        }
        for (index, spec) in self.strings.iter().enumerate() {
            if let Some(error) = spec.validation_error() {
                return Some(format!("string {}: {error}", index + 1));
            }
        }
        None
    }
}

/// On-disk representation of user profiles.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfilesFile {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub profiles: Vec<Profile>,
}

fn default_version() -> u32 {
    1
}

impl Default for ProfilesFile {
    fn default() -> Self {
        Self {
            version: 1,
            profiles: Vec::new(),
        }
    }
}

/// Built-in instrument tunings, newest first is not important because the order
/// is intentional: Generic, guitar, bouzouki, mandolin, bass.
pub fn builtin_profiles() -> Vec<Profile> {
    let mut generic = Profile::from_notes(GENERIC_PROFILE_NAME, &[]);
    generic.builtin = true;
    generic.generic = true;

    let mut profiles = vec![
        generic,
        Profile::from_notes(
            "Guitar (Standard 6-String)",
            &["E2", "A2", "D3", "G3", "B3", "E4"],
        ),
        Profile::from_notes("Bouzouki (Irish/Greek 4-Course)", &["G2", "D3", "A3", "E4"]),
        Profile::from_notes("Mandolin", &["G3", "D4", "A4", "E5"]),
        Profile::from_notes("E-Bass (4-String)", &["E1", "A1", "D2", "G2"]),
    ];
    for profile in &mut profiles {
        profile.builtin = true;
    }
    profiles
}

/// All built-in and custom profiles, plus persistence helpers.
#[derive(Clone, Debug)]
pub struct ProfilesStore {
    profiles: Vec<Profile>,
    path: Option<PathBuf>,
}

impl Default for ProfilesStore {
    fn default() -> Self {
        Self {
            profiles: builtin_profiles(),
            path: None,
        }
    }
}

impl ProfilesStore {
    pub fn load() -> Self {
        Self::load_from(config::profiles_path())
    }

    pub fn load_from(path: Option<PathBuf>) -> Self {
        let mut store = Self {
            profiles: builtin_profiles(),
            path,
        };
        let Some(path) = store.path.as_deref() else {
            return store;
        };
        let Ok(text) = fs::read_to_string(path) else {
            return store;
        };
        let file: ProfilesFile = match toml::from_str(&text) {
            Ok(file) => file,
            Err(error) => {
                log::warn!(
                    "could not parse {}: {error}; custom profiles were not loaded",
                    path.display()
                );
                return store;
            }
        };
        for mut profile in file.profiles {
            profile.builtin = false;
            profile.generic = false;
            profile.name = profile.name.trim().to_string();
            if store.is_name_reserved(&profile.name, None) {
                log::warn!(
                    "skipping profile '{}': name is reserved or duplicated",
                    profile.name
                );
                continue;
            }
            if let Some(error) = profile.validation_error() {
                log::warn!("skipping profile '{}': {error}", profile.name);
                continue;
            }
            store.profiles.push(profile);
        }
        store
    }

    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn find(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.name == name)
    }

    pub fn find_custom(&self, name: &str) -> Option<&Profile> {
        self.profiles
            .iter()
            .find(|profile| profile.name == name && !profile.builtin)
    }

    pub fn custom_profiles(&self) -> impl Iterator<Item = &Profile> {
        self.profiles.iter().filter(|profile| !profile.builtin)
    }

    /// Replace a custom profile (identified by `original_name`) or add a new
    /// one. Built-in names are reserved.
    pub fn upsert_custom(
        &mut self,
        mut profile: Profile,
        original_name: Option<&str>,
    ) -> Result<(), String> {
        profile.builtin = false;
        profile.generic = false;
        profile.name = profile.name.trim().to_string();
        if let Some(error) = profile.validation_error() {
            return Err(error);
        }
        if self.is_name_reserved(&profile.name, original_name) {
            return Err(format!("a profile named '{}' already exists", profile.name));
        }

        if let Some(original) = original_name {
            if let Some(index) = self
                .profiles
                .iter()
                .position(|candidate| candidate.name == original && !candidate.builtin)
            {
                self.profiles[index] = profile;
                return self.save_custom();
            }
        }
        self.profiles.push(profile);
        self.save_custom()
    }

    /// Duplicate a profile under a unique custom name.
    pub fn duplicate_custom(&mut self, source_name: &str) -> Result<String, String> {
        let source = self
            .find(source_name)
            .cloned()
            .ok_or_else(|| format!("profile '{source_name}' not found"))?;
        if source.is_generic() {
            return Err("the Generic profile cannot be duplicated".to_string());
        }
        let base = format!("{} (copy)", source.name);
        let mut name = base.clone();
        let mut counter = 2;
        while self.find(&name).is_some() {
            name = format!("{} {counter}", base);
            counter += 1;
        }
        let duplicate = source.duplicate_as_custom(name.clone());
        self.profiles.push(duplicate);
        self.save_custom()?;
        Ok(name)
    }

    pub fn delete_custom(&mut self, name: &str) -> Result<(), String> {
        let Some(index) = self
            .profiles
            .iter()
            .position(|profile| profile.name == name && !profile.builtin)
        else {
            return Err(format!("custom profile '{name}' not found"));
        };
        self.profiles.remove(index);
        self.save_custom()
    }

    /// Write only custom profiles; built-ins are always regenerated by the
    /// application.
    pub fn save_custom(&self) -> Result<(), String> {
        let Some(path) = self.path.as_deref() else {
            log::warn!("no profiles file is configured; custom profiles will not persist");
            return Ok(());
        };
        let file = ProfilesFile {
            version: 1,
            profiles: self
                .profiles
                .iter()
                .filter(|profile| !profile.builtin)
                .cloned()
                .collect(),
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        }
        let text = toml::to_string_pretty(&file)
            .map_err(|error| format!("could not serialise profiles: {error}"))?;
        fs::write(path, text)
            .map_err(|error| format!("could not write {}: {error}", path.display()))
    }

    fn is_name_reserved(&self, name: &str, original_name: Option<&str>) -> bool {
        let builtin = self
            .profiles
            .iter()
            .any(|profile| profile.builtin && profile.name == name);
        if builtin {
            return true;
        }
        self.profiles.iter().any(|profile| {
            !profile.builtin && profile.name == name && Some(profile.name.as_str()) != original_name
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> ProfilesStore {
        ProfilesStore::default()
    }

    #[test]
    fn builtins_have_expected_strings() {
        let store = store();
        let guitar = store.find("Guitar (Standard 6-String)").unwrap();
        let labels: Vec<_> = guitar
            .strings
            .iter()
            .map(|spec| spec.display(440.0))
            .collect();
        assert_eq!(labels, ["E2", "A2", "D3", "G3", "B3", "E4"]);
        assert_eq!(store.find("E-Bass (4-String)").unwrap().strings.len(), 4);
    }

    #[test]
    fn target_selects_nearest_string() {
        let store = store();
        let guitar = store.find("Guitar (Standard 6-String)").unwrap();
        let d3_hz = Note::parse("D3").unwrap().frequency(440.0);
        let target = guitar.target(d3_hz * 1.01, 440.0).unwrap();
        assert_eq!(target.string_index, Some(2));
        assert_eq!(target.display, "D3");
        assert!(target.cents(d3_hz * 1.01).abs() > 10.0);
    }

    #[test]
    fn generic_profile_uses_chromatic_notes() {
        let store = store();
        let generic = store.find(GENERIC_PROFILE_NAME).unwrap();
        let target = generic
            .target(Note::parse("F#5").unwrap().frequency(440.0), 440.0)
            .unwrap();
        assert_eq!(target.display, "F#5");
        assert!(target.string_index.is_none());
    }

    #[test]
    fn profiles_file_round_trips() {
        let raw = r#"
version = 1
[[profiles]]
name = "Open D"
strings = [{ note = "D2" }, { note = "A2" }, { hz = 293.6648 }]
"#;
        let file: ProfilesFile = toml::from_str(raw).unwrap();
        assert_eq!(file.profiles[0].strings.len(), 3);
        let text = toml::to_string_pretty(&file).unwrap();
        let again: ProfilesFile = toml::from_str(&text).unwrap();
        assert_eq!(again.profiles[0], file.profiles[0]);
    }

    #[test]
    fn reserved_names_are_rejected() {
        let mut store = store();
        let error = store
            .upsert_custom(Profile::new_custom("Generic"), None)
            .unwrap_err();
        assert!(error.contains("reserved") || error.contains("already"));
        assert!(store.duplicate_custom(GENERIC_PROFILE_NAME).is_err());
    }
}
