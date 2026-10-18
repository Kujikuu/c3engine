//! File operations in the Project panel, on the project's own assets: import (a file dialog,
//! or files dropped from the system), new folders, rename, move (drag onto a folder) and delete
//! (to the project's trash). Each is an undoable `AssetCommand`; maps, settings and tables that
//! name a renamed asset follow it.

use crate::app::{Focus, Studio};
use crate::browser::AssetKey;
use crate::icons;
use crate::maps::MapRef;
use crate::theme;
use c3_edit::AssetCommand;
use eframe::egui::{self, RichText};
use std::path::PathBuf;

/// An operation asked for in the Project panel.
pub(crate) enum AssetOp {
    /// Opens the rename dialog.
    Rename(AssetKey),
    Delete(AssetKey),
    /// A new folder inside folder `0`.
    NewFolder(AssetKey),
    /// Picks files to import into folder `0`.
    Import(AssetKey),
    /// Imports files (dropped from the system) into folder `1`.
    ImportFiles(Vec<PathBuf>, AssetKey),
    /// Moves `what` into folder `into`.
    Move {
        what: AssetKey,
        into: AssetKey,
    },
    /// Copies the project's map `0` as a new map, then opens the rename dialog on the copy.
    DuplicateMap(String),
    /// Opens the rename dialog for the project's map `0`.
    RenameMap(String),
    DeleteMap(String),
}

/// The rename dialog: the asset and the name being typed.
pub(crate) struct RenameForm {
    target: RenameTarget,
    name: String,
    focused: bool,
}

/// What the rename dialog renames.
enum RenameTarget {
    Asset(AssetKey),
    /// A project map, by name.
    Map(String),
}

impl RenameTarget {
    /// The path shown over the name.
    fn label(&self) -> String {
        match self {
            Self::Asset(key) => key.rel.clone(),
            Self::Map(name) => format!("maps/{name}"),
        }
    }
}

fn parent(rel: &str) -> &str {
    rel.rsplit_once('/').map_or("", |(p, _)| p)
}

fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    }
}

impl Studio {
    /// Whether root `root` of the Project panel is the project's own assets (operations
    /// apply there; a client used in place stays read-only).
    pub(crate) fn editable_root(&self, root: usize) -> bool {
        let (Some(doc), Some(r)) = (&self.assets, self.browser.roots.get(root)) else {
            return false;
        };
        r.dir == doc.db().root()
    }

    /// Does `op`.
    pub(crate) fn asset_op(&mut self, op: AssetOp) {
        match op {
            AssetOp::Rename(key) => {
                let name = key.rel.rsplit('/').next().unwrap_or(&key.rel).to_owned();
                self.rename = Some(RenameForm {
                    target: RenameTarget::Asset(key),
                    name,
                    focused: false,
                });
            }
            AssetOp::DuplicateMap(name) => {
                let Some(project) = &self.project else { return };
                let copy = (2..)
                    .map(|n| format!("{name} {n}"))
                    .find(|n| !project.map_file(n).exists())
                    .unwrap_or_default();
                let command = AssetCommand::CopyMap {
                    from: name.clone(),
                    to: copy.clone(),
                };
                if self.run_asset_commands(&format!("Duplicate map {name}"), vec![command], false) {
                    self.list_map(&copy);
                    self.asset_op(AssetOp::RenameMap(copy));
                }
            }
            AssetOp::RenameMap(name) => {
                self.rename = Some(RenameForm {
                    name: name.clone(),
                    target: RenameTarget::Map(name),
                    focused: false,
                });
            }
            AssetOp::DeleteMap(name) => {
                let command = AssetCommand::DeleteMap { name: name.clone() };
                self.run_asset_commands(&format!("Delete map {name}"), vec![command], true);
            }
            AssetOp::Delete(key) => {
                self.run_asset_commands(
                    &format!("Delete {}", key.rel),
                    vec![AssetCommand::Delete { path: key.rel }],
                    true,
                );
            }
            AssetOp::NewFolder(folder) => {
                let Some(doc) = &self.assets else { return };
                let path = doc.db().free_name(&folder.rel, "New Folder");
                let key = AssetKey {
                    root: folder.root,
                    rel: path.clone(),
                };
                if self.run_asset_commands(
                    "New folder",
                    vec![AssetCommand::CreateFolder { path }],
                    false,
                ) {
                    self.asset_op(AssetOp::Rename(key));
                }
            }
            AssetOp::Import(folder) => {
                if let Some(files) = rfd::FileDialog::new()
                    .set_title("Import files into the project")
                    .pick_files()
                {
                    self.asset_op(AssetOp::ImportFiles(files, folder));
                }
            }
            AssetOp::ImportFiles(files, folder) => {
                let count = files.len();
                let commands = files
                    .into_iter()
                    .map(|f| AssetCommand::Import {
                        source: f.display().to_string(),
                        folder: folder.rel.clone(),
                    })
                    .collect();
                let label = if count == 1 {
                    "Import a file".to_owned()
                } else {
                    format!("Import {count} files")
                };
                self.run_asset_commands(&label, commands, false);
                self.browser.folder = folder;
            }
            AssetOp::Move { what, into } => {
                let name = what.rel.rsplit('/').next().unwrap_or(&what.rel);
                let to = join(&into.rel, name);
                if to != what.rel {
                    self.rename_asset(&what, to);
                }
            }
        }
    }

    /// Renames the project's map `from`; the open map follows.
    fn rename_map(&mut self, from: &str, to: &str) {
        let open = (self.doc.as_ref()).is_some_and(|d| d.map == MapRef::Project(from.to_owned()));
        let command = AssetCommand::RenameMap {
            from: from.to_owned(),
            to: to.to_owned(),
        };
        if self.run_asset_commands(&format!("Rename map {from}"), vec![command], true) && open {
            self.load_map(&MapRef::Project(to.to_owned()));
        }
    }

    fn rename_asset(&mut self, key: &AssetKey, to: String) {
        let command = AssetCommand::Rename {
            from: key.rel.clone(),
            to: to.clone(),
        };
        if self.run_asset_commands(&format!("Rename {}", key.rel), vec![command], true) {
            // The open folder and the selection follow what moved.
            let moved = |rel: &str| {
                (rel == key.rel || rel.starts_with(&format!("{}/", key.rel)))
                    .then(|| format!("{to}{}", &rel[key.rel.len()..]))
            };
            if let Some(rel) = moved(&self.browser.folder.rel) {
                self.browser.folder.rel = rel;
            }
            let selected = self.browser.selected.clone();
            if let Some(rel) = selected.as_ref().and_then(|s| moved(&s.rel)) {
                self.browse_to(&rel, true);
            }
        }
    }

    /// Runs `commands` as one undo step called `label`; `renames` when they move or delete
    /// files that maps may name (refused while the open map has unsaved changes). Returns
    /// whether they ran.
    fn run_asset_commands(
        &mut self,
        label: &str,
        commands: Vec<AssetCommand>,
        renames: bool,
    ) -> bool {
        if renames && self.map_dirty() {
            self.error = Some("Save the open map first: it may name this asset.".into());
            return false;
        }
        let Some(doc) = &mut self.assets else {
            return false;
        };
        doc.begin_group(label);
        let mut failed = None;
        for command in &commands {
            if let Err(e) = doc.execute(command) {
                failed = Some(e.to_string());
                break;
            }
        }
        doc.end_group();
        self.focus = Focus::Assets;
        self.asset_files_changed();
        match failed {
            Some(e) => {
                self.error = Some(e);
                false
            }
            None => {
                self.notify(format!("{label} · Ctrl+Z undoes it"));
                true
            }
        }
    }

    /// Runs a JSON file of asset commands (one, or a list) as one undo step (`--asset-exec`).
    pub(crate) fn run_asset_command_file(&mut self, file: &str) {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum OneOrMany {
            Many(Vec<AssetCommand>),
            One(AssetCommand),
        }
        let commands = std::fs::read_to_string(file)
            .map_err(|e| e.to_string())
            .and_then(|json| serde_json::from_str::<OneOrMany>(&json).map_err(|e| e.to_string()));
        match commands {
            Ok(OneOrMany::Many(commands)) => {
                self.run_asset_commands(&format!("Run {file}"), commands, true);
            }
            Ok(OneOrMany::One(command)) => {
                self.run_asset_commands(&format!("Run {file}"), vec![command], true);
            }
            Err(e) => self.error = Some(format!("{file}: {e}")),
        }
    }

    /// After asset files moved (an operation, or its undo): the Project panel, the map list,
    /// the open map and the settings are read again from disk, as references in them may
    /// have changed. An open map that is gone (renamed or deleted) closes.
    pub(crate) fn asset_files_changed(&mut self) {
        self.reload_all_assets();
        self.refresh_maps();
        if let (Some(doc), Some(project)) = (&self.doc, &self.project)
            && let MapRef::Project(name) = &doc.map
            && !doc.is_dirty()
        {
            let map = doc.map.clone();
            if project.map_file(name).exists() {
                self.load_map(&map);
            } else {
                self.close_map();
            }
        }
        self.reload_settings();
    }

    /// The rename dialog, while it is open.
    pub(crate) fn rename_window(&mut self, ctx: &egui::Context) {
        let Some(form) = &mut self.rename else { return };
        let (mut done, mut cancel) = (false, false);
        egui::Window::new(theme::icon_text(icons::PENCIL_SIMPLE, "Rename"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(form.target.label())
                        .color(theme::TEXT_DIM)
                        .small(),
                );
                let edit = ui.add(egui::TextEdit::singleline(&mut form.name).desired_width(280.0));
                if !form.focused {
                    edit.request_focus();
                    form.focused = true;
                }
                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    done = true;
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    done |= ui.button("Rename").clicked();
                    cancel = ui.button("Cancel").clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Escape));
                });
            });
        if cancel {
            self.rename = None;
        } else if done && let Some(form) = self.rename.take() {
            let name = form.name.trim();
            match form.target {
                RenameTarget::Asset(key) => {
                    let current = key.rel.rsplit('/').next().unwrap_or(&key.rel);
                    if name.is_empty() || name.contains(['/', '\\']) {
                        self.error = Some("A name can't be empty or hold / or \\".into());
                    } else if name != current {
                        let to = join(parent(&key.rel), name);
                        self.rename_asset(&key, to);
                    }
                }
                RenameTarget::Map(from) if name != from => self.rename_map(&from, name),
                RenameTarget::Map(_) => {}
            }
        }
    }
}
