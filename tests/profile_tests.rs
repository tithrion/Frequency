//! Profile, note and settings integration tests.

use frequency::config::profiles::{
    builtin_profiles, Profile, ProfilesFile, ProfilesStore, TargetSpec, GENERIC_PROFILE_NAME,
};
use frequency::config::settings::{Settings, MAX_A4, MIN_A4};
use frequency::music::{cents_between, Note, TuningTarget};

#[test]
fn required_builtin_profiles_exist_with_expected_tunings() {
    let profiles = builtin_profiles();
    let expected = [
        (
            "Guitar (Standard 6-String)",
            vec!["E2", "A2", "D3", "G3", "B3", "E4"],
        ),
        (
            "Bouzouki (Irish/Greek 4-Course)",
            vec!["G2", "D3", "A3", "E4"],
        ),
        ("Mandolin", vec!["G3", "D4", "A4", "E5"]),
        ("E-Bass (4-String)", vec!["E1", "A1", "D2", "G2"]),
    ];

    assert!(profiles
        .iter()
        .any(|profile| profile.name == GENERIC_PROFILE_NAME && profile.generic));
    for (name, notes) in expected {
        let profile = profiles
            .iter()
            .find(|profile| profile.name == name)
            .unwrap_or_else(|| panic!("missing built-in profile {name}"));
        let actual: Vec<String> = profile
            .strings
            .iter()
            .map(|spec| spec.display(440.0))
            .collect();
        assert_eq!(actual, notes, "{name}");
        assert!(profile.builtin);
    }
}

#[test]
fn scientific_pitch_notation_parses_accidentals() {
    assert_eq!(Note::parse("A4").unwrap().label(), "A4");
    assert_eq!(Note::parse("c#3").unwrap().label(), "C#3");
    assert_eq!(Note::parse("Db3").unwrap().label(), "C#3");
    assert_eq!(Note::parse("C-1").unwrap().label(), "C-1");
    assert_eq!(Note::parse("bb4").unwrap().label(), "A#4");
    assert!(Note::parse("banana").is_err());
}

#[test]
fn profile_targets_the_nearest_string() {
    let profile = Profile::from_notes("Test", &["E2", "A2", "D3", "G3"]);
    let d3 = Note::parse("D3").unwrap().frequency(440.0);
    let target = profile.target(d3 * 1.012, 440.0).unwrap();
    assert_eq!(target.string_index, Some(2));
    assert_eq!(target.display, "D3");
    assert_eq!(target.label, "3 - D3");
    assert!(target.cents(d3 * 1.012) > 15.0);
}

#[test]
fn raw_frequency_overrides_and_matches() {
    let profile = Profile {
        name: "Variable".to_string(),
        strings: vec![TargetSpec::from_hz(300.0), TargetSpec::from_hz(600.0)],
        builtin: false,
        generic: false,
    };
    let target = profile.target(603.0, 440.0).unwrap();
    assert_eq!(target.frequency, 600.0);
    assert_eq!(target.display, "600.00 Hz");
    assert!(target.note.is_none());
}

#[test]
fn generic_target_handles_reference_pitch_changes() {
    let profile = Profile::from_notes(GENERIC_PROFILE_NAME, &[]);
    let mut generic = profile;
    generic.generic = true;

    let target = generic.target(415.0, 415.0).unwrap();
    assert_eq!(target.display, "A4");
    assert!(target.cents(415.0).abs() < 0.01);

    let target = generic.target(440.0, 440.0).unwrap();
    assert_eq!(target.note, Some(Note::parse("A4").unwrap()));
}

#[test]
fn custom_profiles_serialise_as_toml() {
    let profile = Profile {
        name: "Open D".to_string(),
        strings: vec![
            TargetSpec::from_note("D2"),
            TargetSpec::from_note("A2"),
            TargetSpec::from_note("D3"),
            TargetSpec::from_note("F#3"),
            TargetSpec::from_note("A3"),
            TargetSpec::from_note("D4"),
        ],
        builtin: false,
        generic: false,
    };
    let file = ProfilesFile {
        version: 1,
        profiles: vec![profile.clone()],
    };
    let text = toml::to_string_pretty(&file).unwrap();
    assert!(text.contains("[[profiles]]"));
    let parsed: ProfilesFile = toml::from_str(&text).unwrap();
    assert_eq!(parsed.profiles[0].name, "Open D");
    assert_eq!(parsed.profiles[0].strings.len(), 6);
}

#[test]
fn profile_validation_enforces_one_to_ninety_nine_strings() {
    let mut profile = Profile::new_custom("My tuning");
    assert!(profile.validation_error().is_none());

    profile.strings.clear();
    assert!(profile.validation_error().is_some());

    profile.strings = vec![TargetSpec::from_note("A4"); 100];
    assert!(profile.validation_error().is_some());

    profile.strings = vec![TargetSpec::from_note("A4"); 99];
    assert!(profile.validation_error().is_none());

    let invalid = Profile {
        name: "Bad".to_string(),
        strings: vec![TargetSpec::from_note("H2")],
        builtin: false,
        generic: false,
    };
    assert!(invalid.validation_error().is_some());
}

#[test]
fn store_keeps_builtins_and_can_duplicate() {
    let mut store = ProfilesStore::default();
    let before = store.profiles().len();
    let name = store
        .duplicate_custom("Guitar (Standard 6-String)")
        .unwrap();
    assert_eq!(store.profiles().len(), before + 1);
    let copy = store.find_custom(&name).unwrap();
    assert_eq!(copy.strings.len(), 6);
    assert!(!copy.builtin);
}

#[test]
fn cent_and_frequency_conversions_are_inverse() {
    let target = TuningTarget {
        label: "A4".to_string(),
        display: "A4".to_string(),
        frequency: 440.0,
        note: Some(Note::parse("A4").unwrap()),
        string_index: None,
    };
    for cents in [-50.0, -3.0, 0.0, 3.0, 50.0, 1200.0] {
        let frequency = target.frequency_at(cents);
        assert!((target.cents(frequency) - cents).abs() < 1e-3);
        assert!((cents_between(frequency, target.frequency) - cents).abs() < 1e-3);
    }
}

#[test]
fn settings_clamp_reference_pitch() {
    let mut settings = Settings {
        reference_a4: 999.0,
        ..Settings::default()
    };
    settings.sanitize();
    assert_eq!(settings.reference_a4, MAX_A4);

    settings.reference_a4 = 1.0;
    settings.sanitize();
    assert_eq!(settings.reference_a4, MIN_A4);
}
