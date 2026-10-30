//! Small music-theory helpers shared by the DSP, configuration and GUI layers.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Names of the twelve chromatic pitch classes, using sharps.
pub const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// MIDI note number of A0, the lowest note offered in generic mode.
pub const GENERIC_MIN_MIDI: u8 = 21;
/// MIDI note number of C8, the highest note offered in generic mode.
pub const GENERIC_MAX_MIDI: u8 = 108;

/// A note in scientific pitch notation (MIDI note number 0..=127).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Note {
    midi: u8,
}

impl Note {
    /// Construct a note from a MIDI note number. Values above 127 are clamped.
    pub fn from_midi(midi: u8) -> Self {
        Self {
            midi: midi.min(127),
        }
    }

    /// Construct a note from a pitch class (0 = C) and octave.
    pub fn new(pitch_class: u8, octave: i32) -> Option<Self> {
        if pitch_class > 11 {
            return None;
        }
        let midi = (octave + 1) * 12 + pitch_class as i32;
        u8::try_from(midi).ok().map(Self::from_midi)
    }

    pub fn midi(self) -> u8 {
        self.midi
    }

    /// Pitch class index, where C is 0 and B is 11.
    pub fn pitch_class(self) -> usize {
        (self.midi % 12) as usize
    }

    pub fn octave(self) -> i32 {
        self.midi as i32 / 12 - 1
    }

    pub fn name(self) -> &'static str {
        NOTE_NAMES[self.pitch_class()]
    }

    /// Scientific pitch notation, for example `C#3` or `A4`.
    pub fn label(self) -> String {
        format!("{}{}", self.name(), self.octave())
    }

    /// Equal-tempered frequency of this note for the given A4 reference.
    pub fn frequency(self, a4: f32) -> f32 {
        a4 * 2.0_f32.powf((self.midi as f32 - 69.0) / 12.0)
    }

    /// The chromatic note nearest to `frequency`.
    pub fn nearest(frequency: f32, a4: f32) -> Option<Self> {
        if !frequency.is_finite() || frequency <= 0.0 || !a4.is_finite() || a4 <= 0.0 {
            return None;
        }
        let midi = 69.0 + 12.0 * (frequency as f64 / a4 as f64).log2();
        let rounded = midi.round();
        if !rounded.is_finite() {
            return None;
        }
        Some(Self::from_midi(rounded.clamp(0.0, 127.0) as u8))
    }

    /// The nearest note, clamped to the generic tuner range A0..=C8.
    pub fn nearest_in_generic_range(frequency: f32, a4: f32) -> Option<Self> {
        Self::nearest(frequency, a4).map(|note| {
            let midi = note.midi().clamp(GENERIC_MIN_MIDI, GENERIC_MAX_MIDI);
            Self::from_midi(midi)
        })
    }

    /// Parse scientific pitch notation such as `E2`, `C#3`, `Bb4` or `C-1`.
    pub fn parse(input: &str) -> Result<Self, String> {
        let text = input.trim();
        let mut chars = text.chars();
        let letter = chars
            .next()
            .ok_or_else(|| "empty note name".to_string())?
            .to_ascii_uppercase();

        let base = match letter {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return Err(format!("invalid note letter in '{text}'")),
        };

        let rest = chars.as_str();
        let (accidental, rest) = if let Some(stripped) = rest.strip_prefix('#') {
            (1, stripped)
        } else if let Some(stripped) = rest.strip_prefix('\u{266f}') {
            (1, stripped)
        } else if let Some(stripped) = rest.strip_prefix('b') {
            (-1, stripped)
        } else if let Some(stripped) = rest.strip_prefix('\u{266d}') {
            (-1, stripped)
        } else {
            (0, rest)
        };

        let octave: i32 = rest
            .parse()
            .map_err(|_| format!("invalid octave in '{text}'"))?;
        let midi = (octave + 1) * 12 + base + accidental;
        if !(0..=127).contains(&midi) {
            return Err(format!("'{text}' is outside the MIDI range"));
        }
        Ok(Self::from_midi(midi as u8))
    }
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.name(), self.octave())
    }
}

/// Signed distance in cents from `target` to `frequency`.
pub fn cents_between(frequency: f32, target: f32) -> f32 {
    if frequency <= 0.0 || target <= 0.0 || !frequency.is_finite() || !target.is_finite() {
        return f32::NAN;
    }
    1200.0 * (frequency / target).log2()
}

/// A concrete note/string the tuner is currently targeting.
#[derive(Clone, Debug, PartialEq)]
pub struct TuningTarget {
    /// Short label shown above the meter, for example `3 - D3`.
    pub label: String,
    /// Large note name shown in the centre, for example `D3` or `146.83 Hz`.
    pub display: String,
    /// Target frequency in Hz.
    pub frequency: f32,
    /// Note when the target has a named pitch.
    pub note: Option<Note>,
    /// Zero-based string index when tuning an instrument profile.
    pub string_index: Option<usize>,
}

impl TuningTarget {
    pub fn generic(note: Note, a4: f32) -> Self {
        Self {
            label: format!("Chromatic · {}", note.label()),
            display: note.label(),
            frequency: note.frequency(a4),
            note: Some(note),
            string_index: None,
        }
    }

    /// Deviation of a detected frequency from this target.
    pub fn cents(&self, frequency: f32) -> f32 {
        cents_between(frequency, self.frequency)
    }

    /// Frequency reached by moving `cents` away from this target.
    pub fn frequency_at(&self, cents: f32) -> f32 {
        self.frequency * 2.0_f32.powf(cents / 1200.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sharps_flats_and_negative_octaves() {
        assert_eq!(Note::parse("C#3").unwrap().midi(), 49);
        assert_eq!(Note::parse("Db3").unwrap().midi(), 49);
        assert_eq!(Note::parse("Bb4").unwrap().midi(), 70);
        assert_eq!(Note::parse("C-1").unwrap().midi(), 0);
        assert_eq!(Note::parse("A4").unwrap().midi(), 69);
        assert!(Note::parse("H4").is_err());
    }

    #[test]
    fn a4_is_440_hz() {
        let a4 = Note::parse("A4").unwrap();
        assert!((a4.frequency(440.0) - 440.0).abs() < 0.001);
    }

    #[test]
    fn nearest_note_wraps_at_semitone_boundary() {
        let a = Note::parse("A4").unwrap();
        let a_sharp = Note::parse("A#4").unwrap();
        assert_eq!(Note::nearest(440.0 * 1.02, 440.0).unwrap(), a);
        assert_eq!(Note::nearest(440.0 * 1.06, 440.0).unwrap(), a_sharp);
    }
}
