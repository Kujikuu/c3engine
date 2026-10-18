//! Asset editing: import settings, new folders, renames and moves (the project's references
//! follow), deletions (to the project's trash) and imports, applied by [`AssetDoc`] with undo.
//! The Studio's Project panel, the AI and MCP agents use the same commands.

use crate::history::{History, HistoryEntry};
use crate::{EditError, Outcome};
use c3_core::asset::Importer;
use c3_project::{AssetDb, Project};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

/// A change to the project's assets. Paths are relative to `assets/`, with `/`. Serialized as
/// `{"op": "rename", "from": "props/tree.png", "to": "nature/tree.png"}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AssetCommand {
    /// Sets how the asset at `path` is imported: `{"importer": "image", "grid": {"cell":
    /// [32, 48]}, "animations": {"walk": {"frames": [0, 1, 2, 3], "frame_ms": 120}}}`.
    SetImport { path: String, import: Value },
    /// Creates folder `path`.
    CreateFolder { path: String },
    /// Renames or moves a file or folder; maps, settings and tables that name it follow.
    Rename { from: String, to: String },
    /// Moves a file or folder to the project's trash (undo puts it back).
    Delete { path: String },
    /// Copies the file at `source` (a path on disk) into folder `folder` of the assets.
    Import { source: String, folder: String },
    /// Copies the project's map `from` as a new map `to`.
    CopyMap { from: String, to: String },
    /// Renames the project's map `from` to `to`; its entry in the map list follows, so it
    /// keeps its id.
    RenameMap { from: String, to: String },
    /// Moves the project's map `name` to the trash (undo puts it back).
    DeleteMap { name: String },
}

/// What a command did, enough to undo and redo it.
enum Change {
    Import {
        path: String,
        before: Importer,
        after: Importer,
    },
    Folder(String),
    Rename {
        from: String,
        to: String,
    },
    /// `slot` is where the asset waits in the trash while deleted.
    Delete {
        path: String,
        slot: Option<PathBuf>,
    },
    /// An imported file: deleted on undo (into `slot`), put back on redo.
    Imported {
        path: String,
        slot: Option<PathBuf>,
    },
    /// A map made by copying: trashed on undo (into `slot`), put back on redo.
    MapCopied {
        name: String,
        slot: Option<PathBuf>,
    },
    MapRenamed {
        from: String,
        to: String,
    },
    /// `slot` is where the map waits in the trash while deleted.
    MapDeleted {
        name: String,
        slot: Option<PathBuf>,
    },
}

/// The project's assets open for editing.
pub struct AssetDoc {
    project: Project,
    db: AssetDb,
    history: History<Change>,
}

impl AssetDoc {
    pub fn new(project: Project, db: AssetDb) -> Self {
        Self {
            project,
            db,
            history: History::default(),
        }
    }

    pub fn db(&self) -> &AssetDb {
        &self.db
    }

    /// The database, to refresh assets changed on disk.
    pub fn db_mut(&mut self) -> &mut AssetDb {
        &mut self.db
    }

    /// Applies `command`, recording it for undo. Refused commands change nothing.
    pub fn execute(&mut self, command: &AssetCommand) -> Result<Outcome, EditError> {
        let (summary, change) = match command {
            AssetCommand::SetImport { path, import } => {
                let after = importer(import)?;
                let before = self
                    .db
                    .importer(path)
                    .cloned()
                    .ok_or_else(|| no_asset(path))?;
                if before == after {
                    return Ok(unchanged());
                }
                self.db.set_importer(path, after.clone()).map_err(failed)?;
                (
                    format!("Set how {path} is imported"),
                    Change::Import {
                        path: path.clone(),
                        before,
                        after,
                    },
                )
            }
            AssetCommand::CreateFolder { path } => {
                check_path(path)?;
                if self.db.is_folder(path) {
                    return Ok(unchanged());
                }
                self.db.create_folder(path).map_err(failed)?;
                (
                    format!("Create folder {path}"),
                    Change::Folder(path.clone()),
                )
            }
            AssetCommand::Rename { from, to } => {
                check_path(to)?;
                if from == to {
                    return Ok(unchanged());
                }
                self.rename(from, to)?;
                (
                    format!("Rename {from} to {to}"),
                    Change::Rename {
                        from: from.clone(),
                        to: to.clone(),
                    },
                )
            }
            AssetCommand::Delete { path } => {
                let slot = self
                    .db
                    .delete(path, &self.project.trash_dir())
                    .map_err(failed)?;
                (
                    format!("Delete {path}"),
                    Change::Delete {
                        path: path.clone(),
                        slot: Some(slot),
                    },
                )
            }
            AssetCommand::Import { source, folder } => {
                let path = self
                    .db
                    .import(std::path::Path::new(source), folder)
                    .map_err(failed)?;
                (
                    format!("Import {path}"),
                    Change::Imported { path, slot: None },
                )
            }
            AssetCommand::CopyMap { from, to } => {
                check_map_name(to)?;
                self.project.copy_map(from, to).map_err(failed)?;
                (
                    format!("Copy map {from} as {to}"),
                    Change::MapCopied {
                        name: to.clone(),
                        slot: None,
                    },
                )
            }
            AssetCommand::RenameMap { from, to } => {
                check_map_name(to)?;
                if from == to {
                    return Ok(unchanged());
                }
                self.project.rename_map(from, to).map_err(failed)?;
                (
                    format!("Rename map {from} to {to}"),
                    Change::MapRenamed {
                        from: from.clone(),
                        to: to.clone(),
                    },
                )
            }
            AssetCommand::DeleteMap { name } => {
                let slot = self.project.trash_map(name).map_err(failed)?;
                (
                    format!("Delete map {name}"),
                    Change::MapDeleted {
                        name: name.clone(),
                        slot: Some(slot),
                    },
                )
            }
        };
        self.history.record(summary.clone(), change);
        Ok(Outcome {
            summary,
            changed: true,
        })
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), EditError> {
        self.db.rename(from, to).map_err(failed)?;
        self.project
            .rename_references(from, to)
            .map(|_| ())
            .map_err(failed)
    }

    /// Merges the following commands into one undo step called `label` (a drag).
    pub fn begin_group(&mut self, label: impl Into<String>) {
        self.history.begin_group(label.into());
    }

    pub fn end_group(&mut self) {
        self.history.end_group();
    }

    /// Reverts the last step; returns its label.
    pub fn undo(&mut self) -> Result<Option<String>, EditError> {
        let Some(mut step) = self.history.take_undo() else {
            return Ok(None);
        };
        for change in step.changes.iter_mut().rev() {
            self.revert(change)?;
        }
        let label = step.label.clone();
        self.history.undone(step);
        Ok(Some(label))
    }

    /// Reapplies the last undone step; returns its label.
    pub fn redo(&mut self) -> Result<Option<String>, EditError> {
        let Some(mut step) = self.history.take_redo() else {
            return Ok(None);
        };
        for change in &mut step.changes {
            self.apply(change)?;
        }
        let label = step.label.clone();
        self.history.redone(step);
        Ok(Some(label))
    }

    fn revert(&mut self, change: &mut Change) -> Result<(), EditError> {
        match change {
            Change::Import { path, before, .. } => {
                self.db.set_importer(path, before.clone()).map_err(failed)?;
            }
            Change::Folder(path) => {
                // Only an empty folder goes; one that was filled since stays.
                let _ = std::fs::remove_dir(self.db.file(path));
            }
            Change::Rename { from, to } => self.rename(to, from)?,
            Change::Delete { path, slot } => {
                let at = slot.take().ok_or_else(|| no_asset(path))?;
                self.db.restore(path, &at).map_err(failed)?;
            }
            Change::Imported { path, slot } => {
                *slot = Some(
                    self.db
                        .delete(path, &self.project.trash_dir())
                        .map_err(failed)?,
                );
            }
            Change::MapCopied { name, slot } => {
                *slot = Some(self.project.trash_map(name).map_err(failed)?);
            }
            Change::MapRenamed { from, to } => self.project.rename_map(to, from).map_err(failed)?,
            Change::MapDeleted { name, slot } => {
                let at = slot.take().ok_or_else(|| no_map(name))?;
                self.project.restore_map(name, &at).map_err(failed)?;
            }
        }
        Ok(())
    }

    fn apply(&mut self, change: &mut Change) -> Result<(), EditError> {
        match change {
            Change::Import { path, after, .. } => {
                self.db.set_importer(path, after.clone()).map_err(failed)?;
            }
            Change::Folder(path) => self.db.create_folder(path).map_err(failed)?,
            Change::Rename { from, to } => self.rename(from, to)?,
            Change::Delete { path, slot } => {
                *slot = Some(
                    self.db
                        .delete(path, &self.project.trash_dir())
                        .map_err(failed)?,
                );
            }
            Change::Imported { path, slot } => {
                let at = slot.take().ok_or_else(|| no_asset(path))?;
                self.db.restore(path, &at).map_err(failed)?;
            }
            Change::MapCopied { name, slot } => {
                let at = slot.take().ok_or_else(|| no_map(name))?;
                self.project.restore_map(name, &at).map_err(failed)?;
            }
            Change::MapRenamed { from, to } => self.project.rename_map(from, to).map_err(failed)?,
            Change::MapDeleted { name, slot } => {
                *slot = Some(self.project.trash_map(name).map_err(failed)?);
            }
        }
        Ok(())
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.history.undo_label()
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.history.redo_label()
    }

    pub fn history(&self) -> Vec<HistoryEntry> {
        self.history.entries()
    }
}

/// Import settings from JSON, checked.
fn importer(value: &Value) -> Result<Importer, EditError> {
    let import: Importer = serde_json::from_value(value.clone())
        .map_err(|e| EditError::InvalidAsset(format!("those import settings don't fit: {e}")))?;
    match import.problems().into_iter().next() {
        Some(problem) => Err(EditError::InvalidAsset(problem)),
        None => Ok(import),
    }
}

/// A path inside the assets: relative, `/`-separated, no `..`.
fn check_path(path: &str) -> Result<(), EditError> {
    let bad = path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == ".." || p.starts_with('.'));
    if bad {
        Err(EditError::InvalidAsset(format!(
            "\"{path}\" isn't a path inside the assets (names joined by /)"
        )))
    } else {
        Ok(())
    }
}

fn unchanged() -> Outcome {
    Outcome {
        summary: "Nothing changed".into(),
        changed: false,
    }
}

fn no_map(name: &str) -> EditError {
    EditError::InvalidAsset(format!("the map \"{name}\" is gone"))
}

/// A map name is a plain file name: no folders, nothing Windows refuses.
fn check_map_name(name: &str) -> Result<(), EditError> {
    let plain = !name.is_empty()
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !name
            .chars()
            .any(|c| c.is_control() || r#"/\:*?"<>|"#.contains(c));
    if plain {
        Ok(())
    } else {
        Err(EditError::InvalidAsset(format!(
            "\"{name}\" can't be a map name: a file name, without / \\ : * ? \" < > |"
        )))
    }
}

fn no_asset(path: &str) -> EditError {
    EditError::InvalidAsset(format!("the project has no asset \"{path}\""))
}

fn failed(e: c3_project::Error) -> EditError {
    EditError::InvalidAsset(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_project::Manifest;
    use serde_json::json;
    use std::fs;

    fn project(name: &str) -> Project {
        let dir =
            std::env::temp_dir().join(format!("c3-edit-assets-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        Project::create(&dir, Manifest::new("Test")).unwrap()
    }

    #[test]
    fn renames_carry_references_and_undo() {
        let p = project("rename");
        fs::create_dir_all(p.assets_dir().join("props")).unwrap();
        fs::write(p.assets_dir().join("props/tree.png"), b"png").unwrap();
        let map = p.maps_dir().join("Meadow.ron");
        fs::write(&map, r#"(sprite: "props/tree.png#2")"#).unwrap();
        let (db, _) = AssetDb::of(&p);
        let mut doc = AssetDoc::new(p.clone(), db);
        doc.execute(&AssetCommand::Rename {
            from: "props".into(),
            to: "nature".into(),
        })
        .unwrap();
        assert!(doc.db().meta("nature/tree.png").is_some());
        assert_eq!(
            fs::read_to_string(&map).unwrap(),
            r#"(sprite: "nature/tree.png#2")"#
        );
        doc.undo().unwrap();
        assert!(doc.db().meta("props/tree.png").is_some());
        assert_eq!(
            fs::read_to_string(&map).unwrap(),
            r#"(sprite: "props/tree.png#2")"#
        );
        let _ = fs::remove_dir_all(&p.root);
    }

    #[test]
    fn import_settings_are_checked_and_undone() {
        let p = project("import");
        fs::write(p.assets_dir().join("hero.png"), b"png").unwrap();
        let (db, _) = AssetDb::of(&p);
        let mut doc = AssetDoc::new(p.clone(), db);
        let set = |import| AssetCommand::SetImport {
            path: "hero.png".into(),
            import,
        };
        assert!(
            doc.execute(&set(json!({"importer": "image", "pivot": [3.0, 0.0]})))
                .is_err()
        );
        doc.execute(&set(
            json!({"importer": "image", "grid": {"cell": [32, 48]}}),
        ))
        .unwrap();
        assert!(
            matches!(doc.db().importer("hero.png"), Some(Importer::Image(i)) if i.grid.is_some())
        );
        doc.undo().unwrap();
        assert!(
            matches!(doc.db().importer("hero.png"), Some(Importer::Image(i)) if i.grid.is_none())
        );
        let _ = fs::remove_dir_all(&p.root);
    }

    #[test]
    fn map_copies_renames_and_deletes_undo() {
        let p = project("maps");
        fs::create_dir_all(p.maps_dir()).unwrap();
        fs::write(p.map_file("Meadow"), "scene").unwrap();
        let (db, _) = AssetDb::of(&p);
        let mut doc = AssetDoc::new(p.clone(), db);
        let run = |doc: &mut AssetDoc, c| doc.execute(&c).map(|_| ());
        run(
            &mut doc,
            AssetCommand::CopyMap {
                from: "Meadow".into(),
                to: "Field".into(),
            },
        )
        .unwrap();
        assert!(
            run(
                &mut doc,
                AssetCommand::RenameMap {
                    from: "Field".into(),
                    to: "a/b".into(),
                }
            )
            .is_err()
        );
        run(
            &mut doc,
            AssetCommand::RenameMap {
                from: "Field".into(),
                to: "Plain".into(),
            },
        )
        .unwrap();
        run(
            &mut doc,
            AssetCommand::DeleteMap {
                name: "Meadow".into(),
            },
        )
        .unwrap();
        assert_eq!(p.map_names(), ["Plain"]);
        doc.undo().unwrap();
        doc.undo().unwrap();
        assert_eq!(p.map_names(), ["Field", "Meadow"]);
        doc.undo().unwrap();
        assert_eq!(p.map_names(), ["Meadow"]);
        doc.redo().unwrap();
        doc.redo().unwrap();
        doc.redo().unwrap();
        assert_eq!(p.map_names(), ["Plain"]);
    }

    #[test]
    fn deletes_and_imports_undo_and_redo() {
        let p = project("delete");
        let outside = p.root.join("outside.wav");
        fs::write(&outside, b"wav").unwrap();
        let (db, _) = AssetDb::of(&p);
        let mut doc = AssetDoc::new(p.clone(), db);
        doc.execute(&AssetCommand::Import {
            source: outside.display().to_string(),
            folder: "sounds".into(),
        })
        .unwrap();
        let id = doc.db().meta("sounds/outside.wav").unwrap().id.clone();
        doc.execute(&AssetCommand::Delete {
            path: "sounds/outside.wav".into(),
        })
        .unwrap();
        assert!(doc.db().is_empty());
        doc.undo().unwrap();
        assert_eq!(doc.db().path_of(&id), Some("sounds/outside.wav"));
        doc.undo().unwrap();
        assert!(doc.db().is_empty());
        doc.redo().unwrap();
        assert_eq!(doc.db().path_of(&id), Some("sounds/outside.wav"));
        assert!(
            doc.execute(&AssetCommand::CreateFolder {
                path: "../x".into()
            })
            .is_err()
        );
        let _ = fs::remove_dir_all(&p.root);
    }
}
