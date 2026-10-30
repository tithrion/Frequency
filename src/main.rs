#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use pitchperfect::gui::TunerApp;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,pitchperfect=info"),
    )
    .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([980.0, 700.0])
            .with_min_inner_size([720.0, 560.0])
            .with_title("PitchPerfect"),
        ..Default::default()
    };

    eframe::run_native(
        "PitchPerfect",
        options,
        Box::new(|cc| Box::new(TunerApp::new(cc))),
    )
}
