//! C3 Studio: the engine's editor. v0: client map browser/viewer.
//! Usage: c3-studio [client_dir] [--map map/map/xxx.DMap] [--zoom 0.5]

mod app;
mod map_view;
mod panels;
mod theme;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 560.0])
            .with_title("C3 Studio"),
        ..Default::default()
    };
    eframe::run_native(
        "C3 Studio",
        options,
        Box::new(|cc| Ok(Box::new(app::Studio::new(cc)))),
    )
}
