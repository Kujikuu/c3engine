//! File > New Map: a blank, walkable map of the project, covered with one of its images
//! (or the engine's default ground), ready to paint, populate and play.

use crate::app::Studio;
use crate::icons;
use crate::maps::MapRef;
use crate::theme;
use c3_play::basic::GroundImage;
use eframe::egui::{self, DragValue, RichText};

/// Cells along a side of the smallest and the largest new map.
const SIZE_RANGE: std::ops::RangeInclusive<u32> = 16..=1024;

/// The New Map dialog's fields, while it is open.
pub struct NewMapForm {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// The image (an asset path) repeated over the ground; empty: the engine's.
    pub ground: String,
    /// The project's images, to pick the ground from.
    images: Vec<String>,
}

impl Default for NewMapForm {
    fn default() -> Self {
        Self {
            name: "New Map".into(),
            width: 128,
            height: 128,
            ground: String::new(),
            images: Vec::new(),
        }
    }
}

/// Image files under `dir`, as asset paths (`tiles/grass.png`), sorted.
pub(crate) fn images_in(dir: &std::path::Path) -> Vec<String> {
    const KINDS: [&str; 6] = ["png", "jpg", "jpeg", "bmp", "tga", "dds"];
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(folder) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for path in entries.flatten().map(|e| e.path()) {
            if path.is_dir() {
                stack.push(path);
            } else if (path.extension().and_then(|e| e.to_str()))
                .is_some_and(|e| KINDS.contains(&e.to_ascii_lowercase().as_str()))
                && let Ok(rel) = path.strip_prefix(dir)
            {
                found.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
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
        if let Some(project) = &self.project {
            form.images = images_in(&project.assets_dir());
        }
        self.new_map = Some(form);
    }

    /// Writes a blank `width` × `height` map called `name` into the project, its ground
    /// covered with image `ground` (an asset path; empty: the engine's), and opens it.
    pub(crate) fn create_map(&mut self, name: &str, width: u32, height: u32, ground: &str) {
        let Some(project) = &self.project else {
            return;
        };
        let file = project.map_file(name);
        let cell = (self.applied_settings.as_ref())
            .map_or([64.0, 32.0], |s| s.camera.new_map_cell.map(|v| v as f32));
        let image = if ground.is_empty() {
            GroundImage::BUILTIN
        } else {
            // Squares as wide as the image.
            let Some(size) = (self.game.as_ref()).and_then(|g| g.assets().image(ground)) else {
                self.error = Some(format!("Can't read the ground image {ground}"));
                return;
            };
            GroundImage {
                image: ground,
                tile_size: size.width,
            }
        };
        let scene = c3_play::basic::blank_scene(width, height, cell, image);
        let written = file
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .map_err(|e| e.to_string())
            .and_then(|()| c3_scene::native::save_scene(&scene, &file).map_err(|e| e.to_string()));
        match written {
            Ok(()) => {
                self.refresh_maps();
                self.list_map(name);
                self.refresh_assets();
                self.open_map(&MapRef::Project(name.to_owned()));
            }
            Err(e) => self.error = Some(format!("Can't create the map: {e}")),
        }
    }

    /// Adds map `key` to Project Settings > Maps with the id it plays with now, so that id
    /// stays its own (portals to it keep working whatever maps come later).
    pub(crate) fn list_map(&mut self, key: &str) {
        let (Some(editor), Some(info)) = (&mut self.settings, self.map_list.by_key(key)) else {
            return;
        };
        let mut maps = editor.settings().maps.clone();
        if maps.iter().any(|m| m.key == key) {
            return;
        }
        maps.push(c3_core::settings::MapEntry {
            key: key.to_owned(),
            id: info.id(),
            ..Default::default()
        });
        let command = c3_edit::SettingsCommand::Set {
            path: "maps".into(),
            value: serde_json::to_value(&maps).unwrap_or_default(),
        };
        match editor.doc.execute(&command) {
            Ok(_) => self.settings_changed(),
            Err(e) => self.error = Some(format!("Can't list the new map: {e}")),
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
                        ui.label("Ground");
                        ground_picker(ui, &mut form.ground, &form.images);
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
                        "Every cell walkable, the ground image repeated over it",
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
            let ground = std::mem::take(&mut form.ground);
            self.new_map = None;
            self.create_map(&name, width, height, &ground);
        } else if close {
            self.new_map = None;
        }
    }
}

/// The ground image: the engine's default, or one of `images`.
fn ground_picker(ui: &mut egui::Ui, ground: &mut String, images: &[String]) {
    const DEFAULT: &str = "Default ground";
    let shown = if ground.is_empty() {
        DEFAULT.to_owned()
    } else {
        ground.clone()
    };
    egui::ComboBox::from_id_salt("new map ground")
        .selected_text(shown)
        .width(220.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(ground, String::new(), DEFAULT);
            for image in images {
                ui.selectable_value(ground, image.clone(), image);
            }
        })
        .response
        .on_hover_text("Square, seamless images tile best");
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
