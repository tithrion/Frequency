#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use frequency::gui::TunerApp;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,frequency=info"),
    )
    .init();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([980.0, 700.0])
        .with_min_inner_size([720.0, 560.0])
        .with_title("Frequency");
    if let Some(icon) = frequency::gui::logo::icon_data() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Frequency",
        options,
        Box::new(|cc| Box::new(TunerApp::new(cc))),
    )
}
