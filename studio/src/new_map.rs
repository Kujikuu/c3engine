//! File > New Map: a blank, walkable map of the project, covered with the engine's default
//! ground, ready to paint, populate and play.

use crate::app::Studio;
use crate::icons;
use crate::maps::MapRef;
use crate::theme;
use eframe::egui::{self, DragValue, RichText};

/// Cells along a side of the smallest and the largest new map.
const SIZE_RANGE: std::ops::RangeInclusive<u32> = 16..=1024;

/// The New Map dialog's fields, while it is open.
pub struct NewMapForm {
    pub name: String,
    pub width: u32,
    pub height: u32,
}

impl Default for NewMapForm {
    fn default() -> Self {
        Self {
            name: "New Map".into(),
            width: 128,
            height: 128,
        }
    }
}

/// A map name that is a plain file name: no folders, nothing Windows refuses.
fn valid_map_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !name
            .chars()
            .any(|c| c.is_control() || r#"/\:*?"<>|"#.contains(c))
}

impl Studio {
    /// Opens the New Map dialog, with `name` filled in when given.
    pub(crate) fn show_new_map(&mut self, name: Option<&str>) {
        let mut form = NewMapForm::default();
        if let Some(name) = name {
            form.name = name.to_owned();
        }
        self.new_map = Some(form);
    }

    /// Writes a blank `width` × `height` map called `name` into the project and opens it.
    pub(crate) fn create_map(&mut self, name: &str, width: u32, height: u32) {
        let Some(project) = &self.project else {
            return;
        };
        let file = project.map_file(name);
        let scene = c3_play::basic::blank_scene(width, height);
        let written = file
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .map_err(|e| e.to_string())
            .and_then(|()| c3_scene::native::save_scene(&scene, &file).map_err(|e| e.to_string()));
        match written {
            Ok(()) => {
                self.refresh_maps();
                self.refresh_assets();
                self.open_map(&MapRef::Project(name.to_owned()));
            }
            Err(e) => self.error = Some(format!("Can't create the map: {e}")),
        }
    }

    /// Draws the New Map dialog while it is open; Create writes the map and opens it.
    pub(crate) fn new_map_dialog(&mut self, ctx: &egui::Context) {
        let (Some(form), Some(project)) = (&mut self.new_map, &self.project) else {
            return;
        };
        let mut create = false;
        let mut close = false;
        egui::Window::new("New Map")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                egui::Grid::new("new map fields")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Name");
                        ui.add(egui::TextEdit::singleline(&mut form.name).desired_width(220.0))
                            .request_focus();
                        ui.end_row();
                        ui.label("Size");
                        ui.horizontal(|ui| {
                            ui.add(DragValue::new(&mut form.width).range(SIZE_RANGE));
                            ui.label("×");
                            ui.add(DragValue::new(&mut form.height).range(SIZE_RANGE));
                            ui.label(RichText::new("cells").color(theme::TEXT_DIM));
                        });
                        ui.end_row();
                    });
                let name = form.name.trim();
                let ok = valid_map_name(name);
                let taken = ok && project.map_file(name).exists();
                let (hint, color) = if !ok {
                    ("A file name: no / \\ : * ? \" < > |", theme::WARNING)
                } else if taken {
                    ("A map with this name exists", theme::WARNING)
                } else {
                    (
                        "Every cell walkable, covered with the default ground",
                        theme::TEXT_DIM,
                    )
                };
                ui.label(RichText::new(hint).small().color(color));
                ui.horizontal(|ui| {
                    let can = ok && !taken;
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    create = ui
                        .add_enabled(
                            can,
                            egui::Button::new(theme::icon_text(icons::PLUS, "Create")),
                        )
                        .clicked()
                        || (can && enter);
                    close = ui.button("Cancel").clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Escape));
                });
            });
        if create {
            let (name, width, height) = (form.name.trim().to_owned(), form.width, form.height);
            self.new_map = None;
            self.create_map(&name, width, height);
        } else if close {
            self.new_map = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_names_are_plain_file_names() {
        assert!(valid_map_name("Twin City") && valid_map_name("arena-2"));
        assert!(!valid_map_name("") && !valid_map_name("a/b") && !valid_map_name("..\\x"));
        assert!(!valid_map_name("what?") && !valid_map_name(".hidden"));
    }
}
