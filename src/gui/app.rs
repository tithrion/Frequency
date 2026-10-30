//! Main egui application.

use crate::audio::host::HostInfo;
use crate::audio::{AudioHandle, AudioSelection, AudioStatus};
use crate::config::profiles::{ProfilesStore, GENERIC_PROFILE_NAME};
use crate::config::settings::{Settings, MAX_A4, MAX_SMOOTHING, MIN_A4, MIN_SMOOTHING};
use crate::dsp::{Analysis, AnalyzerHandle};
use crate::gui::editor::EditorState;
use crate::gui::{meter, theme};
use crate::music::TuningTarget;
use eframe::egui::{self, Align, Color32, Layout, Margin, RichText, Stroke};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProfileAction {
    New,
    Edit(usize),
    Duplicate(usize),
    Delete(usize),
    Activate(usize),
}

/// The top-level tuner application.
pub struct TunerApp {
    settings: Settings,
    profiles: ProfilesStore,
    audio: AudioHandle,
    analyzer: AnalyzerHandle,
    status: Arc<Mutex<AudioStatus>>,
    hosts: Vec<HostInfo>,
    selected_host: String,
    selected_device: String,
    show_settings: bool,
    show_profiles: bool,
    confirm_delete: Option<String>,
    editor: EditorState,
    visual_cents: Option<f32>,
    notice: Option<String>,
}

impl TunerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);

        let mut settings = Settings::load();
        settings.sanitize();
        let profiles = ProfilesStore::load();
        if profiles.find(&settings.profile).is_none() {
            settings.profile = GENERIC_PROFILE_NAME.to_string();
        }

        let hosts = crate::audio::host::scan();
        let (selected_host, selected_device) = initial_selection(&hosts, &settings);
        let selection = AudioSelection {
            host: host_option(&selected_host),
            device: device_option(&selected_device),
            preferred_sample_rate: settings.preferred_sample_rate,
        };

        let (feed_tx, feed_rx) = mpsc::channel();
        let analyzer = AnalyzerHandle::spawn(feed_rx);
        analyzer.set_alpha(settings.smoothing);
        analyzer.set_window_size(settings.buffer_size);
        let audio = AudioHandle::spawn(selection, feed_tx);
        let status = audio.status();

        Self {
            settings,
            profiles,
            audio,
            analyzer,
            status,
            hosts,
            selected_host,
            selected_device,
            show_settings: false,
            show_profiles: false,
            confirm_delete: None,
            editor: EditorState::default(),
            visual_cents: None,
            notice: None,
        }
    }

    fn devices_for(&self, host: &str) -> Vec<String> {
        self.hosts
            .iter()
            .find(|candidate| candidate.name == host)
            .map(|candidate| candidate.devices.clone())
            .unwrap_or_default()
    }

    fn persist_settings(&mut self) {
        self.settings.sanitize();
        match self.settings.save() {
            Ok(()) => self.notice = None,
            Err(error) => self.notice = Some(error),
        }
    }

    fn apply_audio_selection(&mut self) {
        let host = host_option(&self.selected_host);
        let device = device_option(&self.selected_device);
        self.settings.host = host.clone();
        self.settings.device = device.clone();
        self.audio.select(AudioSelection {
            host,
            device,
            preferred_sample_rate: self.settings.preferred_sample_rate,
        });
        self.persist_settings();
    }

    fn refresh_audio_devices(&mut self) {
        self.hosts = crate::audio::host::scan();
        let (host, device) = initial_selection(&self.hosts, &self.settings);
        self.selected_host = host;
        self.selected_device = device;
        self.apply_audio_selection();
    }

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top-bar")
            .frame(
                egui::Frame::none()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::symmetric(18.0, 11.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("PitchPerfect")
                            .size(24.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new("TUNER")
                            .size(10.0)
                            .strong()
                            .color(theme::GREEN),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("⚙ Settings").clicked() {
                            self.show_settings = !self.show_settings;
                        }
                        if ui.button("Profiles").clicked() {
                            self.show_profiles = true;
                        }

                        let mut selected = self.settings.profile.clone();
                        egui::ComboBox::from_id_source("profile-select")
                            .width(220.0)
                            .selected_text(selected.clone())
                            .show_ui(ui, |ui| {
                                for profile in self.profiles.profiles() {
                                    ui.selectable_value(
                                        &mut selected,
                                        profile.name.clone(),
                                        profile.name.clone(),
                                    );
                                }
                            });
                        if selected != self.settings.profile {
                            self.settings.profile = selected;
                            self.visual_cents = None;
                            self.persist_settings();
                        }
                        ui.label(RichText::new("Profile").size(12.0).color(theme::DIM));
                    });
                });
            });
    }

    fn bottom_bar(&mut self, ctx: &egui::Context, status: &AudioStatus) {
        let host_names: Vec<String> = self.hosts.iter().map(|host| host.name.clone()).collect();
        let mut host_changed = false;
        let mut device_changed = false;
        let mut refresh = false;
        let mut a4_changed = false;

        egui::TopBottomPanel::bottom("audio-bar")
            .frame(
                egui::Frame::none()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::symmetric(18.0, 10.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Driver").size(12.0).color(theme::DIM));
                    let mut selected_host = self.selected_host.clone();
                    egui::ComboBox::from_id_source("host-select")
                        .width(110.0)
                        .selected_text(selected_host.clone())
                        .show_ui(ui, |ui| {
                            for name in &host_names {
                                ui.selectable_value(&mut selected_host, name.clone(), name.clone());
                            }
                        });
                    if selected_host != self.selected_host {
                        self.selected_host = selected_host;
                        host_changed = true;
                    }

                    ui.separator();
                    ui.label(RichText::new("Device").size(12.0).color(theme::DIM));
                    let devices = self.devices_for(&self.selected_host);
                    let mut selected_device = self.selected_device.clone();
                    egui::ComboBox::from_id_source("device-select")
                        .width(250.0)
                        .selected_text(if selected_device.is_empty() {
                            "No input device".to_string()
                        } else {
                            selected_device.clone()
                        })
                        .show_ui(ui, |ui| {
                            if devices.is_empty() {
                                ui.label(RichText::new("No devices found").color(theme::DIM));
                            }
                            for name in &devices {
                                ui.selectable_value(
                                    &mut selected_device,
                                    name.clone(),
                                    name.clone(),
                                );
                            }
                        });
                    if selected_device != self.selected_device {
                        self.selected_device = selected_device;
                        device_changed = true;
                    }

                    if ui
                        .small_button("⟳")
                        .on_hover_text("Rescan audio hosts and devices")
                        .clicked()
                    {
                        refresh = true;
                    }

                    ui.separator();
                    ui.label(RichText::new("A4").size(12.0).color(theme::DIM));
                    if ui
                        .add(
                            egui::DragValue::new(&mut self.settings.reference_a4)
                                .clamp_range(MIN_A4..=MAX_A4)
                                .speed(0.1)
                                .suffix(" Hz"),
                        )
                        .changed()
                    {
                        a4_changed = true;
                    }

                    ui.separator();
                    let running_colour = if status.running {
                        theme::GREEN
                    } else {
                        theme::AMBER
                    };
                    ui.label(RichText::new("●").color(running_colour).size(11.0));
                    let summary = if status.running {
                        format!(
                            "{} · {} · {} Hz · {} ch",
                            status.host, status.device, status.sample_rate, status.channels
                        )
                    } else {
                        status
                            .error
                            .clone()
                            .unwrap_or_else(|| "Audio input stopped".to_string())
                    };
                    ui.label(RichText::new(summary).size(11.0).color(if status.running {
                        theme::DIM
                    } else {
                        theme::RED
                    }));

                    if let Some(notice) = &status.notice {
                        ui.separator();
                        ui.label(RichText::new(notice).size(11.0).color(theme::AMBER));
                    }
                });

                if let Some(notice) = &self.notice {
                    ui.label(RichText::new(notice).size(11.0).color(theme::RED));
                }
            });

        if refresh {
            self.refresh_audio_devices();
            return;
        }
        if host_changed {
            self.selected_device = self
                .devices_for(&self.selected_host)
                .first()
                .cloned()
                .unwrap_or_default();
            self.apply_audio_selection();
        } else if device_changed {
            self.apply_audio_selection();
        }
        if a4_changed {
            self.persist_settings();
        }
    }

    fn settings_panel(&mut self, ui: &mut egui::Ui) {
        let mut dirty = false;
        let mut restart_audio = false;
        let mut refresh = false;
        let analysis = self.analyzer.latest();
        let status = self.audio.snapshot();

        ui.add_space(4.0);
        ui.heading("Settings");
        ui.add_space(8.0);

        ui.label(
            RichText::new("TUNING")
                .size(10.0)
                .strong()
                .color(theme::DIM),
        );
        ui.add_space(2.0);
        if ui
            .add(
                egui::Slider::new(&mut self.settings.reference_a4, MIN_A4..=MAX_A4)
                    .suffix(" Hz")
                    .fixed_decimals(1),
            )
            .changed()
        {
            dirty = true;
        }
        ui.label(
            RichText::new(
                "Reference pitch for A4. Baroque and period instruments often use 415 Hz.",
            )
            .size(11.0)
            .color(theme::DIM),
        );

        ui.add_space(12.0);
        ui.label(
            RichText::new("PITCH INERTIA")
                .size(10.0)
                .strong()
                .color(theme::DIM),
        );
        ui.add_space(2.0);
        if ui
            .add(
                egui::Slider::new(&mut self.settings.smoothing, MIN_SMOOTHING..=MAX_SMOOTHING)
                    .fixed_decimals(2),
            )
            .changed()
        {
            dirty = true;
            self.analyzer.set_alpha(self.settings.smoothing);
        }
        ui.label(
            RichText::new(format!(
                "α = {:.2}; lower values produce a steadier needle.",
                self.settings.smoothing
            ))
            .size(11.0)
            .color(theme::DIM),
        );

        ui.add_space(12.0);
        ui.label(
            RichText::new("DETECTION WINDOW")
                .size(10.0)
                .strong()
                .color(theme::DIM),
        );
        let mut buffer_size = self.settings.buffer_size;
        egui::ComboBox::from_id_source("buffer-size")
            .selected_text(format!("{buffer_size} samples"))
            .show_ui(ui, |ui| {
                for option in [1024_usize, 2048, 4096, 8192] {
                    ui.selectable_value(&mut buffer_size, option, format!("{option} samples"));
                }
            });
        if buffer_size != self.settings.buffer_size {
            self.settings.buffer_size = buffer_size;
            self.analyzer.set_window_size(buffer_size);
            dirty = true;
        }
        ui.label(
            RichText::new("Larger windows resolve low bass notes; smaller windows react faster.")
                .size(11.0)
                .color(theme::DIM),
        );

        ui.add_space(12.0);
        ui.label(RichText::new("AUDIO").size(10.0).strong().color(theme::DIM));
        ui.horizontal(|ui| {
            ui.label("Preferred sample rate");
            let mut rate = self.settings.preferred_sample_rate;
            egui::ComboBox::from_id_source("sample-rate")
                .selected_text(format!("{rate} Hz"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut rate, 44_100, "44100 Hz");
                    ui.selectable_value(&mut rate, 48_000, "48000 Hz");
                });
            if rate != self.settings.preferred_sample_rate {
                self.settings.preferred_sample_rate = rate;
                dirty = true;
                restart_audio = true;
            }
        });
        if ui.button("Refresh devices").clicked() {
            refresh = true;
        }

        ui.add_space(10.0);
        egui::Frame::group(ui.style())
            .fill(theme::PANEL_RAISED)
            .stroke(Stroke::new(1.0, theme::TRACK))
            .show(ui, |ui| {
                egui::Grid::new("audio-status-grid")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("State").color(theme::DIM));
                        ui.label(
                            RichText::new(if status.running { "running" } else { "stopped" })
                                .color(if status.running {
                                    theme::GREEN
                                } else {
                                    theme::AMBER
                                }),
                        );
                        ui.end_row();
                        ui.label(RichText::new("Host").color(theme::DIM));
                        ui.label(&status.host);
                        ui.end_row();
                        ui.label(RichText::new("Device").color(theme::DIM));
                        ui.label(if status.device.is_empty() {
                            "—"
                        } else {
                            &status.device
                        });
                        ui.end_row();
                        ui.label(RichText::new("Stream").color(theme::DIM));
                        ui.label(format!(
                            "{} Hz · {} ch",
                            status.sample_rate, status.channels
                        ));
                        ui.end_row();
                        ui.label(RichText::new("Detector").color(theme::DIM));
                        ui.label(match analysis.method {
                            Some(method) => format!(
                                "{} · {:.0}% confidence",
                                method.short_name(),
                                analysis.confidence * 100.0
                            ),
                            None => "—".to_string(),
                        });
                        ui.end_row();
                        ui.label(RichText::new("Level").color(theme::DIM));
                        ui.label(format!("{:.4} RMS", analysis.rms));
                        ui.end_row();
                    });
                if let Some(error) = &status.error {
                    ui.label(RichText::new(error).color(theme::RED));
                }
                if let Some(notice) = &status.notice {
                    ui.label(RichText::new(notice).color(theme::AMBER));
                }
            });

        ui.add_space(12.0);
        if ui.button("Save settings now").clicked() {
            self.persist_settings();
        }
        if let Some(path) = crate::config::settings_path() {
            ui.label(RichText::new("settings.toml").size(10.0).color(theme::DIM))
                .on_hover_text(path.display().to_string());
        }

        if dirty {
            self.persist_settings();
        }
        if restart_audio {
            self.apply_audio_selection();
        }
        if refresh {
            self.refresh_audio_devices();
        }
    }

    fn central_panel(
        &mut self,
        ctx: &egui::Context,
        analysis: &Analysis,
        target: Option<&TuningTarget>,
    ) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(28.0, 18.0)),
            )
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.vertical_centered(|ui| {
                    let target_label = target
                        .map(|value| value.label.clone())
                        .unwrap_or_else(|| "Waiting for a note".to_string());
                    ui.label(
                        RichText::new(target_label)
                            .size(18.0)
                            .color(if target.is_some() {
                                theme::DIM
                            } else {
                                theme::TRACK
                            }),
                    );
                    ui.add_space(4.0);

                    let display = target
                        .map(|value| value.display.clone())
                        .unwrap_or_else(|| "—".to_string());
                    ui.label(RichText::new(display).size(96.0).strong().color(
                        if target.is_some() {
                            theme::TEXT
                        } else {
                            theme::TRACK
                        },
                    ));

                    let frequency = target
                        .map(|value| format!("{:.2} Hz", value.frequency))
                        .unwrap_or_else(|| "No signal".to_string());
                    ui.label(RichText::new(frequency).size(23.0).color(theme::DIM));

                    if let (Some(detected), Some(_target)) = (analysis.frequency, target) {
                        let detector = analysis
                            .method
                            .map(|method| format!(" · {}", method.short_name()))
                            .unwrap_or_default();
                        ui.label(
                            RichText::new(format!(
                                "detected {detected:.2} Hz · {:.0}% confidence{detector}",
                                analysis.confidence * 100.0
                            ))
                            .size(12.0)
                            .color(theme::DIM),
                        );
                    } else {
                        ui.add_space(16.0);
                    }

                    ui.add_space(14.0);
                    meter::cents_meter(ui, self.visual_cents, target.is_some());
                    ui.add_space(10.0);

                    let (message, colour) = match self.visual_cents {
                        None => ("Play a steady note".to_string(), theme::DIM),
                        Some(cents) if cents.abs() <= 3.0 => {
                            (format!("In tune · {cents:+.1} cents"), theme::GREEN)
                        }
                        Some(cents) if cents > 0.0 => {
                            (format!("Tune down · {cents:+.1} cents"), theme::AMBER)
                        }
                        Some(cents) => (format!("Tune up · {cents:+.1} cents"), theme::AMBER),
                    };
                    ui.label(RichText::new(message).size(26.0).strong().color(colour));
                    ui.add_space(8.0);

                    if let Some(profile) = self.profiles.find(&self.settings.profile) {
                        let description = if profile.is_generic() {
                            "Chromatic mode · A0 to C8 · nearest semitone (±50 cents)".to_string()
                        } else {
                            let labels: Vec<String> = profile
                                .strings
                                .iter()
                                .map(|spec| spec.display(self.settings.reference_a4))
                                .collect();
                            format!("{} strings · {}", profile.strings.len(), labels.join(" · "))
                        };
                        ui.label(RichText::new(description).size(11.0).color(theme::TRACK));
                    }
                });
            });
    }

    fn profiles_window(&mut self, ctx: &egui::Context) {
        if !self.show_profiles {
            return;
        }
        let mut open = self.show_profiles;
        let mut action = None;
        let builtin_count = self
            .profiles
            .profiles()
            .iter()
            .filter(|p| p.builtin)
            .count();
        let custom_count = self.profiles.profiles().len() - builtin_count;
        let active = self.settings.profile.clone();

        egui::Window::new("Instrument profiles")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(640.0)
            .max_height(520.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("+ New profile").clicked() {
                        action = Some(ProfileAction::New);
                    }
                    ui.label(
                        RichText::new(format!("{builtin_count} built-in · {custom_count} custom"))
                            .size(11.0)
                            .color(theme::DIM),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new("Custom profiles are stored in profiles.toml")
                                .size(10.0)
                                .color(theme::TRACK),
                        );
                    });
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .max_height(390.0)
                    .show(ui, |ui| {
                        for (index, profile) in self.profiles.profiles().iter().enumerate() {
                            egui::Frame::group(ui.style())
                                .fill(if profile.name == active {
                                    Color32::from_rgb(25, 42, 34)
                                } else {
                                    theme::PANEL_RAISED
                                })
                                .stroke(Stroke::new(
                                    1.0,
                                    if profile.name == active {
                                        theme::GREEN_DIM
                                    } else {
                                        theme::TRACK
                                    },
                                ))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        if ui
                                            .selectable_label(
                                                profile.name == active,
                                                RichText::new(&profile.name).strong(),
                                            )
                                            .clicked()
                                        {
                                            action = Some(ProfileAction::Activate(index));
                                        }
                                        if profile.builtin {
                                            ui.label(
                                                RichText::new("built-in")
                                                    .size(9.0)
                                                    .color(theme::DIM),
                                            );
                                        }
                                        if profile.name == active {
                                            ui.label(
                                                RichText::new("active")
                                                    .size(9.0)
                                                    .color(theme::GREEN),
                                            );
                                        }

                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                if !profile.builtin
                                                    && ui.small_button("Delete").clicked()
                                                {
                                                    action = Some(ProfileAction::Delete(index));
                                                }
                                                if !profile.generic
                                                    && ui.small_button("Duplicate").clicked()
                                                {
                                                    action = Some(ProfileAction::Duplicate(index));
                                                }
                                                if !profile.builtin
                                                    && !profile.generic
                                                    && ui.small_button("Edit").clicked()
                                                {
                                                    action = Some(ProfileAction::Edit(index));
                                                }
                                            },
                                        );
                                    });

                                    if profile.generic {
                                        ui.label(
                                            RichText::new(
                                                "Auto-detects every chromatic note from A0 to C8.",
                                            )
                                            .size(11.0)
                                            .color(theme::DIM),
                                        );
                                    } else {
                                        let labels: Vec<String> = profile
                                            .strings
                                            .iter()
                                            .take(12)
                                            .map(|spec| spec.display(self.settings.reference_a4))
                                            .collect();
                                        let suffix = if profile.strings.len() > 12 {
                                            format!(" … +{}", profile.strings.len() - 12)
                                        } else {
                                            String::new()
                                        };
                                        ui.label(
                                            RichText::new(format!(
                                                "{} strings · {}{}",
                                                profile.strings.len(),
                                                labels.join(" · "),
                                                suffix
                                            ))
                                            .size(11.0)
                                            .color(theme::DIM),
                                        );
                                    }
                                });
                            ui.add_space(5.0);
                        }
                    });
            });

        self.show_profiles = open;
        let Some(action) = action else {
            return;
        };
        match action {
            ProfileAction::New => self.editor.open_new(),
            ProfileAction::Edit(index) => {
                if let Some(profile) = self.profiles.profiles().get(index).cloned() {
                    self.editor.open_edit(&profile);
                }
            }
            ProfileAction::Duplicate(index) => {
                if let Some(profile) = self.profiles.profiles().get(index).cloned() {
                    match self.profiles.duplicate_custom(&profile.name) {
                        Ok(name) => {
                            self.notice = None;
                            if let Some(copy) = self.profiles.find_custom(&name).cloned() {
                                self.settings.profile = name;
                                self.visual_cents = None;
                                self.persist_settings();
                                self.editor.open_edit(&copy);
                            }
                        }
                        Err(error) => self.notice = Some(error),
                    }
                }
            }
            ProfileAction::Delete(index) => {
                if let Some(profile) = self.profiles.profiles().get(index) {
                    if !profile.builtin {
                        self.confirm_delete = Some(profile.name.clone());
                    }
                }
            }
            ProfileAction::Activate(index) => {
                if let Some(profile) = self.profiles.profiles().get(index) {
                    if self.settings.profile != profile.name {
                        self.settings.profile = profile.name.clone();
                        self.visual_cents = None;
                        self.persist_settings();
                    }
                }
            }
        }
    }

    fn delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(name) = self.confirm_delete.clone() else {
            return;
        };
        let mut open = true;
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new("Delete profile")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("Delete the custom profile '{name}'?"));
                ui.label(
                    RichText::new("This cannot be undone.")
                        .size(11.0)
                        .color(theme::AMBER),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        confirm = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });

        if confirm {
            match self.profiles.delete_custom(&name) {
                Ok(()) => {
                    self.notice = None;
                    if self.settings.profile == name {
                        self.settings.profile = GENERIC_PROFILE_NAME.to_string();
                        self.visual_cents = None;
                        self.persist_settings();
                    }
                }
                Err(error) => self.notice = Some(error),
            }
            self.confirm_delete = None;
        } else if cancel || !open {
            self.confirm_delete = None;
        }
    }

    fn editor_window(&mut self, ctx: &egui::Context) {
        let mut editor = std::mem::take(&mut self.editor);
        if let Some((profile, original_name)) = editor.show(ctx, self.settings.reference_a4) {
            let mut profile = profile;
            profile.name = profile.name.trim().to_string();
            match self
                .profiles
                .upsert_custom(profile.clone(), original_name.as_deref())
            {
                Ok(()) => {
                    self.settings.profile = profile.name.clone();
                    self.visual_cents = None;
                    self.persist_settings();
                    editor.close();
                }
                Err(error) => editor.set_error(error),
            }
        }
        if editor.is_open() {
            self.editor = editor;
        }
    }
}

impl eframe::App for TunerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(16));

        self.analyzer.set_alpha(self.settings.smoothing);
        self.analyzer.set_window_size(self.settings.buffer_size);

        let analysis = self.analyzer.latest();
        let a4 = self.settings.reference_a4;
        let profile = self
            .profiles
            .find(&self.settings.profile)
            .or_else(|| self.profiles.find(GENERIC_PROFILE_NAME));
        let live_frequency = analysis
            .frequency
            .filter(|_| analysis.is_fresh(Duration::from_millis(250)));
        let target = live_frequency
            .and_then(|frequency| profile.and_then(|profile| profile.target(frequency, a4)));

        let raw_cents = match (live_frequency, target.as_ref()) {
            (Some(frequency), Some(target)) if target.frequency > 0.0 => {
                Some(target.cents(frequency).clamp(-50.0, 50.0))
            }
            _ => None,
        };
        self.visual_cents = match (raw_cents, self.visual_cents) {
            (Some(new), Some(previous)) => {
                if (new - previous).abs() > 25.0 {
                    Some(new)
                } else {
                    Some(previous + (new - previous) * 0.28)
                }
            }
            (Some(new), None) => Some(new),
            (None, _) => None,
        };

        let status = self
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        self.top_bar(ctx);
        self.bottom_bar(ctx, &status);

        if self.show_settings {
            egui::SidePanel::right("settings-panel")
                .resizable(false)
                .default_width(310.0)
                .frame(
                    egui::Frame::none()
                        .fill(theme::PANEL)
                        .inner_margin(Margin::symmetric(16.0, 12.0)),
                )
                .show(ctx, |ui| self.settings_panel(ui));
        }

        self.central_panel(ctx, &analysis, target.as_ref());
        self.profiles_window(ctx);
        self.delete_dialog(ctx);
        self.editor_window(ctx);
    }
}

impl Drop for TunerApp {
    fn drop(&mut self) {
        self.settings.host = host_option(&self.selected_host);
        self.settings.device = device_option(&self.selected_device);
        if let Err(error) = self.settings.save() {
            log::warn!("could not save settings on exit: {error}");
        }
    }
}

fn initial_selection(hosts: &[HostInfo], settings: &Settings) -> (String, String) {
    let requested = settings.host.as_deref().and_then(|requested| {
        hosts
            .iter()
            .find(|host| host.name.eq_ignore_ascii_case(requested))
    });
    let host = requested
        .or_else(|| hosts.iter().find(|host| host.name == "ALSA"))
        .or_else(|| hosts.first());

    let Some(host) = host else {
        return (
            "System default".to_string(),
            settings.device.clone().unwrap_or_default(),
        );
    };

    let device = settings
        .device
        .as_ref()
        .filter(|device| host.devices.iter().any(|candidate| candidate == *device))
        .cloned()
        .or_else(|| host.devices.first().cloned())
        .unwrap_or_default();
    (host.name.clone(), device)
}

fn host_option(host: &str) -> Option<String> {
    (host != "System default").then(|| host.to_string())
}

fn device_option(device: &str) -> Option<String> {
    (!device.trim().is_empty()).then(|| device.to_string())
}
