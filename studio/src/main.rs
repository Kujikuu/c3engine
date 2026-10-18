//! C3 Studio: the engine's editor.
//! Usage: c3-studio [project folder] [--map <project map | map/map/xxx.DMap>] [--zoom 0.5]
//!                  [--model <3dobj id>]
//!                  [--new | --import <Conquer 5065 client folder>] [--exec <commands.json>]
//! Without a project it opens on the start page (new, open, import, recent); `--new` and
//! `--import` open those dialogs. `--exec` runs a JSON list of `c3_edit` scene commands on the
//! `--map` as one undo step.

mod app;
mod doc;
mod editor;
mod gpu;
mod home;
mod map_view;
mod maps;
mod model_view;
mod models_ws;
mod panels;
mod theme;
mod tq_plugin;

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
