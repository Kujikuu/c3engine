//! C3 Studio: the engine's editor.
//! Usage: c3-studio [project folder] [--map <project map | map/map/xxx.DMap>] [--zoom 0.5]
//!                  [--model <3dobj id>]
//!                  [--new | --import <Conquer 5065 client folder>] [--exec <commands.json>]
//!                  [--browse <content folder>] [--select <content file>]
//! Without a project it opens on the start page (new, open, import, recent); `--new` and
//! `--import` open those dialogs. `--exec` runs a JSON list of `c3_edit` scene commands on the
//! `--map` as one undo step.

mod app;
mod arrange;
mod asset_import;
mod asset_ops;
mod browser;
mod code_view;
mod data_panel;
mod dialogue_panel;
mod doc;
mod dock;
mod editor;
mod gpu;
mod graph_canvas;
mod ground_panel;
mod hierarchy;
mod home;
mod hot_reload;
mod icons;
mod live_scene;
mod map_view;
mod maps;
mod minimap;
mod model_view;
mod models_ws;
mod new_map;
mod node_editor;
mod panels;
mod placing;
mod play;
mod project_settings;
mod scene_inspector;
mod script_panel;
mod selection;
mod shell;
mod stage;
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
