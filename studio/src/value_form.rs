//! Pieces the asset editors (effects, animation sets) build their forms from: one undo step
//! per edit, combo pickers, labelled grids and the project files to pick from.

use crate::theme;
use eframe::egui::{self, RichText};

/// Widgets of one edit, so a drag or a field's typing is one undo step.
#[derive(Default)]
pub(crate) struct Touch {
    pub changed: bool,
    pub started: bool,
    pub finished: bool,
}

impl Touch {
    pub fn track(&mut self, r: egui::Response) -> egui::Response {
        self.changed |= r.changed();
        self.started |= r.drag_started() || r.gained_focus();
        self.finished |= r.drag_stopped() || r.lost_focus();
        r
    }

    /// A one-click change (a button, a pick): its own step.
    pub fn click(&mut self) {
        self.changed = true;
        self.started = true;
        self.finished = true;
    }
}

/// A combo box over `options`; returns a newly picked one.
pub(crate) fn pick(
    ui: &mut egui::Ui,
    id: &str,
    current: &str,
    options: &[String],
) -> Option<String> {
    let shown = if current.is_empty() { "none" } else { current };
    let mut picked = None;
    egui::ComboBox::from_id_salt(("value pick", id, ui.next_auto_id()))
        .selected_text(shown)
        .width(170.0)
        .show_ui(ui, |ui| {
            for option in options {
                if ui.selectable_label(option == current, option).clicked() {
                    picked = Some(option.clone());
                }
            }
        });
    picked.filter(|p| p != current)
}

pub(crate) fn grid(ui: &mut egui::Ui, id: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([10.0, 5.0])
        .show(ui, body);
}

pub(crate) fn field(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.label(RichText::new(label).color(theme::TEXT_DIM));
    add(ui);
    ui.end_row();
}

/// The file's opening comment lines (`#`), kept above the settings when it is written back.
pub(crate) fn comment_header(text: &str) -> String {
    (text.lines())
        .take_while(|l| l.trim_start().starts_with('#'))
        .map(|l| format!("{l}\n"))
        .collect()
}

/// glTF models under `dir`, as asset paths.
pub(crate) fn models_in(dir: &std::path::Path) -> Vec<String> {
    files_in(dir, &["gltf", "glb"])
}

/// Sounds under `dir`, as asset paths.
pub(crate) fn sounds_in(dir: &std::path::Path) -> Vec<String> {
    files_in(dir, &["wav", "ogg", "mp3"])
}

/// Files under `dir` with one of `extensions` (lower case), as sorted asset paths.
fn files_in(dir: &std::path::Path, extensions: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(folder) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for path in entries.flatten().map(|e| e.path()) {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase);
            if path.is_dir() {
                stack.push(path);
            } else if ext.is_some_and(|e| extensions.contains(&e.as_str()))
                && let Ok(rel) = path.strip_prefix(dir)
            {
                found.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}
