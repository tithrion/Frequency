//! Dark theme and shared palette.

use eframe::egui::{self, Color32, FontId, TextStyle};

pub const BG: Color32 = Color32::from_rgb(14, 17, 22);
pub const PANEL: Color32 = Color32::from_rgb(21, 25, 32);
pub const PANEL_RAISED: Color32 = Color32::from_rgb(29, 35, 44);
pub const TRACK: Color32 = Color32::from_rgb(38, 45, 56);
pub const TEXT: Color32 = Color32::from_rgb(235, 240, 246);
pub const DIM: Color32 = Color32::from_rgb(135, 148, 163);
pub const GREEN: Color32 = Color32::from_rgb(57, 220, 132);
pub const GREEN_DIM: Color32 = Color32::from_rgb(25, 82, 56);
pub const AMBER: Color32 = Color32::from_rgb(255, 176, 46);
pub const AMBER_DIM: Color32 = Color32::from_rgb(92, 63, 20);
pub const RED: Color32 = Color32::from_rgb(255, 99, 99);

/// Accent colour for a cent deviation.
pub fn accent_for(cents: f32) -> Color32 {
    if cents.abs() <= 3.0 {
        GREEN
    } else {
        AMBER
    }
}

pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.window_stroke = egui::Stroke::new(1.0, TRACK);
    visuals.extreme_bg_color = Color32::from_rgb(10, 12, 16);
    visuals.faint_bg_color = PANEL_RAISED;
    visuals.selection.bg_fill = Color32::from_rgb(32, 112, 71);
    visuals.selection.stroke = egui::Stroke::new(1.0, GREEN);

    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = PANEL;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, TRACK);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, DIM);

    visuals.widgets.inactive.bg_fill = PANEL_RAISED;
    visuals.widgets.inactive.weak_bg_fill = PANEL_RAISED;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, TRACK);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT);

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(40, 49, 61);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(40, 49, 61);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, GREEN_DIM);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, GREEN);

    visuals.widgets.active.bg_fill = Color32::from_rgb(31, 88, 60);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(31, 88, 60);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, GREEN);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, TEXT);

    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.slider_width = 180.0;
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(28.0));
    ctx.set_style(style);
}
