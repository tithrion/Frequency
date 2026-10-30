//! Instrument profile editor.

use crate::config::profiles::{
    Profile, TargetSpec, GENERIC_PROFILE_NAME, MAX_STRINGS, MIN_STRINGS,
};
use crate::gui::theme;
use crate::music::Note;
use eframe::egui;

#[derive(Clone, Debug)]
struct StringDraft {
    mode_note: bool,
    note: String,
    hz: String,
    error: Option<String>,
}

impl Default for StringDraft {
    fn default() -> Self {
        Self {
            mode_note: true,
            note: "A4".to_string(),
            hz: "440.00".to_string(),
            error: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct EditorState {
    open: bool,
    original_name: Option<String>,
    name: String,
    strings: Vec<StringDraft>,
    error: Option<String>,
    save_error: Option<String>,
    saved_notice: bool,
}

impl EditorState {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn set_error(&mut self, error: String) {
        self.save_error = Some(error);
    }

    pub fn open_new(&mut self) {
        self.open = true;
        self.original_name = None;
        self.name = "My Custom Tuning".to_string();
        self.strings = vec![StringDraft::default()];
        self.error = None;
        self.save_error = None;
        self.saved_notice = false;
    }

    pub fn open_edit(&mut self, profile: &Profile) {
        self.open = true;
        self.original_name = Some(profile.name.clone());
        self.name = profile.name.clone();
        self.strings = profile
            .strings
            .iter()
            .map(|spec| {
                if let Some(hz) = spec.hz {
                    StringDraft {
                        mode_note: false,
                        note: spec.note.clone().unwrap_or_else(|| "A4".to_string()),
                        hz: format!("{hz:.2}"),
                        error: None,
                    }
                } else {
                    StringDraft {
                        mode_note: true,
                        note: spec.note.clone().unwrap_or_else(|| "A4".to_string()),
                        hz: "440.00".to_string(),
                        error: None,
                    }
                }
            })
            .collect();
        self.error = None;
        self.save_error = None;
        self.saved_notice = false;
    }

    fn profile(&self) -> Profile {
        Profile {
            name: self.name.trim().to_string(),
            strings: self
                .strings
                .iter()
                .map(|draft| {
                    if draft.mode_note {
                        TargetSpec::from_note(draft.note.trim().to_string())
                    } else {
                        match draft.hz.trim().parse::<f32>() {
                            Ok(hz) => TargetSpec::from_hz(hz),
                            Err(_) => TargetSpec::from_hz(440.0),
                        }
                    }
                })
                .collect(),
            builtin: false,
            generic: false,
        }
    }

    fn draft_error(draft: &StringDraft) -> Option<String> {
        if draft.mode_note {
            let text = draft.note.trim();
            if text.is_empty() {
                return Some("note name is empty".to_string());
            }
            Note::parse(text)
                .err()
                .map(|_| format!("invalid note '{text}'"))
        } else {
            let text = draft.hz.trim();
            if text.is_empty() {
                return Some("frequency is empty".to_string());
            }
            match text.parse::<f32>() {
                Err(_) => Some(format!("invalid frequency '{text}'")),
                Ok(hz) if !hz.is_finite() || !(1.0..=30_000.0).contains(&hz) => {
                    Some("frequency must be between 1 Hz and 30000 Hz".to_string())
                }
                Ok(_) => None,
            }
        }
    }

    fn validate(&mut self) {
        let mut first_string_error = None;
        for (index, draft) in self.strings.iter_mut().enumerate() {
            draft.error = Self::draft_error(draft);
            if first_string_error.is_none() {
                if let Some(error) = &draft.error {
                    first_string_error = Some(format!("string {}: {error}", index + 1));
                }
            }
        }

        let name = self.name.trim();
        self.error = if name.is_empty() {
            Some("profile name is empty".to_string())
        } else if name == GENERIC_PROFILE_NAME {
            Some("'Generic' is a reserved profile name".to_string())
        } else if !(MIN_STRINGS..=MAX_STRINGS).contains(&self.strings.len()) {
            Some(format!(
                "profile must contain between {MIN_STRINGS} and {MAX_STRINGS} strings"
            ))
        } else {
            first_string_error
        };
    }

    pub fn show(&mut self, ctx: &egui::Context, a4: f32) -> Option<(Profile, Option<String>)> {
        let mut submit: Option<(Profile, Option<String>)> = None;
        if !self.open {
            return None;
        }
        let mut open = self.open;
        let mut close_requested = false;

        egui::Window::new("Profile Editor")
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .default_height(420.0)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Profile name");
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.name)
                                .desired_width(280.0)
                                .hint_text("e.g. Open D"),
                        )
                        .changed()
                    {
                        self.saved_notice = false;
                        self.save_error = None;
                    }
                });
                ui.add_space(8.0);

                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .show(ui, |ui| {
                        let mut index = 0;
                        while index < self.strings.len() {
                            let mut remove = false;
                            egui::Frame::group(ui.style())
                                .fill(theme::PANEL_RAISED)
                                .stroke(egui::Stroke::new(1.0, theme::TRACK))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!("String {}", index + 1))
                                                .strong(),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if self.strings.len() > MIN_STRINGS
                                                    && ui
                                                        .small_button("Remove")
                                                        .on_hover_text("Remove this string")
                                                        .clicked()
                                                {
                                                    remove = true;
                                                }
                                            },
                                        );
                                    });

                                    let mut mode_note = self.strings[index].mode_note;
                                    ui.horizontal(|ui| {
                                        ui.selectable_value(
                                            &mut mode_note,
                                            true,
                                            "Note (Scientific)",
                                        );
                                        ui.selectable_value(
                                            &mut mode_note,
                                            false,
                                            "Raw frequency (Hz)",
                                        );
                                    });
                                    if mode_note != self.strings[index].mode_note {
                                        if !mode_note {
                                            let frequency =
                                                Note::parse(self.strings[index].note.trim())
                                                    .map(|note| note.frequency(a4))
                                                    .unwrap_or(440.0);
                                            self.strings[index].hz = format!("{frequency:.2}");
                                        } else if self.strings[index].note.trim().is_empty() {
                                            self.strings[index].note = "A4".to_string();
                                        }
                                        self.strings[index].mode_note = mode_note;
                                        self.saved_notice = false;
                                        self.save_error = None;
                                    }

                                    if self.strings[index].mode_note {
                                        ui.horizontal(|ui| {
                                            ui.label("Note");
                                            let response = ui
                                                .add(
                                                    egui::TextEdit::singleline(
                                                        &mut self.strings[index].note,
                                                    )
                                                    .desired_width(120.0)
                                                    .hint_text("E2, C#3, Bb4"),
                                                )
                                                .on_hover_text("Examples: E1, A2, D3, G3, B3, E4");
                                            if response.changed() {
                                                self.saved_notice = false;
                                                self.save_error = None;
                                            }
                                            if let Ok(note) =
                                                Note::parse(self.strings[index].note.trim())
                                            {
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "({:.2} Hz @ A4={})",
                                                        note.frequency(a4),
                                                        a4 as i32
                                                    ))
                                                    .color(theme::DIM),
                                                );
                                            }
                                        });
                                    } else {
                                        ui.horizontal(|ui| {
                                            ui.label("Frequency");
                                            if ui
                                                .add(
                                                    egui::TextEdit::singleline(
                                                        &mut self.strings[index].hz,
                                                    )
                                                    .desired_width(120.0)
                                                    .hint_text("440.00"),
                                                )
                                                .changed()
                                            {
                                                self.saved_notice = false;
                                                self.save_error = None;
                                            }
                                            ui.label(egui::RichText::new("Hz").color(theme::DIM));
                                        });
                                    }

                                    if let Some(error) = &self.strings[index].error {
                                        ui.label(egui::RichText::new(error).color(theme::RED));
                                    }
                                });

                            if remove {
                                self.strings.remove(index);
                                self.saved_notice = false;
                                self.save_error = None;
                                self.validate();
                            } else {
                                index += 1;
                            }
                            ui.add_space(6.0);
                        }
                    });

                ui.horizontal(|ui| {
                    if self.strings.len() < MAX_STRINGS && ui.button("Add string").clicked() {
                        self.strings.push(StringDraft::default());
                        self.saved_notice = false;
                        self.save_error = None;
                        self.validate();
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "{} / {} strings",
                            self.strings.len(),
                            MAX_STRINGS
                        ))
                        .color(theme::DIM),
                    );
                });
                ui.add_space(10.0);

                self.validate();
                if let Some(error) = &self.save_error {
                    ui.label(egui::RichText::new(error).color(theme::RED));
                } else if let Some(error) = &self.error {
                    ui.label(egui::RichText::new(error).color(theme::RED));
                } else if self.saved_notice {
                    ui.label(egui::RichText::new("Profile saved").color(theme::GREEN));
                } else {
                    ui.label(egui::RichText::new("Validation OK").color(theme::GREEN_DIM));
                }

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        close_requested = true;
                    }
                    if ui.button("Save profile").clicked() {
                        self.save_error = None;
                        self.validate();
                        if self.error.is_none() {
                            let profile = self.profile();
                            submit = Some((profile, self.original_name.clone()));
                            self.saved_notice = true;
                        }
                    }
                });
            });

        if close_requested {
            self.open = false;
        } else {
            self.open = open;
        }
        submit
    }
}
