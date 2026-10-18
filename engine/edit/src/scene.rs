//! Scene (map) editing: commands, the document that applies them, and its undo history.

use crate::history::{History, HistoryEntry};
use crate::{EditError, Outcome};
use c3_scene::{Cell, CellPos, Portal, Scene};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Most cells one command may change.
pub const MAX_AREA_CELLS: usize = 1 << 20;

/// A set of cells. Cells are `[x, y]` grid coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum Area {
    /// These cells.
    Cells { cells: Vec<CellPos> },
    /// A square of `2·radius + 1` cells per side around `center` (a diamond on screen).
    Square { center: CellPos, radius: i32 },
    /// Every cell from `min` to `max`, inclusive.
    Rect { min: CellPos, max: CellPos },
}

impl Area {
    /// The cells covered, clipped to `scene`'s grid.
    fn cells(&self, scene: &Scene) -> Result<Vec<CellPos>, EditError> {
        let in_grid = |c: &CellPos| scene.grid.index(*c).is_some();
        let (min, max) = match self {
            Self::Cells { cells } => {
                if cells.len() > MAX_AREA_CELLS {
                    return Err(EditError::AreaTooLarge(cells.len()));
                }
                return Ok(cells.iter().copied().filter(in_grid).collect());
            }
            Self::Square { center, radius } => {
                if *radius < 0 {
                    return Err(EditError::InvalidArea("negative radius".into()));
                }
                let r = *radius;
                ((center.0 - r, center.1 - r), (center.0 + r, center.1 + r))
            }
            Self::Rect { min, max } => (*min, *max),
        };
        if max.0 < min.0 || max.1 < min.1 {
            return Err(EditError::InvalidArea("max is before min".into()));
        }
        // Clip first, so huge areas over small maps stay cheap.
        let (w, h) = (scene.grid.width as i32, scene.grid.height as i32);
        let (x0, y0) = (min.0.max(0), min.1.max(0));
        let (x1, y1) = (max.0.min(w - 1), max.1.min(h - 1));
        if x1 < x0 || y1 < y0 {
            return Ok(Vec::new());
        }
        let count = (x1 - x0 + 1) as usize * (y1 - y0 + 1) as usize;
        if count > MAX_AREA_CELLS {
            return Err(EditError::AreaTooLarge(count));
        }
        Ok((y0..=y1)
            .flat_map(|y| (x0..=x1).map(move |x| (x, y)))
            .collect())
    }
}

/// A change to a scene. Serialized as `{"op": "set_blocked", ...}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SceneCommand {
    /// Marks every cell of `area` blocked (impassable) or walkable.
    SetBlocked { area: Area, blocked: bool },
    /// Adds a portal (map exit) on `cell`, with `id` or else the lowest free id.
    AddPortal {
        cell: CellPos,
        #[serde(default)]
        id: Option<i32>,
    },
    /// Moves portal `id` to cell `to`.
    MovePortal { id: i32, to: CellPos },
    /// Deletes portal `id`.
    RemovePortal { id: i32 },
}

/// A recorded change, with what it replaced.
enum Change {
    Cells(Vec<CellChange>),
    Portals {
        before: Vec<Portal>,
        after: Vec<Portal>,
    },
}

struct CellChange {
    index: usize,
    before: Cell,
    after: Cell,
}

/// An open scene: every change goes through [`SceneDoc::execute`] and can be undone.
pub struct SceneDoc {
    scene: Scene,
    history: History<Change>,
}

impl SceneDoc {
    pub fn new(scene: Scene) -> Self {
        Self {
            scene,
            history: History::default(),
        }
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Applies `command`, recording it for undo. Refused commands change nothing.
    pub fn execute(&mut self, command: &SceneCommand) -> Result<Outcome, EditError> {
        let (summary, change) = match command {
            SceneCommand::SetBlocked { area, blocked } => self.set_blocked(area, *blocked)?,
            SceneCommand::AddPortal { cell, id } => self.add_portal(*cell, *id)?,
            SceneCommand::MovePortal { id, to } => self.move_portal(*id, *to)?,
            SceneCommand::RemovePortal { id } => self.remove_portal(*id)?,
        };
        let changed = change.is_some();
        if let Some(change) = change {
            self.history.record(summary.clone(), change);
        }
        Ok(Outcome { summary, changed })
    }

    fn set_blocked(
        &mut self,
        area: &Area,
        blocked: bool,
    ) -> Result<(String, Option<Change>), EditError> {
        let flags = u16::from(blocked);
        let mut changes = Vec::new();
        for cell in area.cells(&self.scene)? {
            let Some(index) = self.scene.grid.index(cell) else {
                continue;
            };
            let before = self.scene.grid.cells[index];
            if before.flags == flags {
                continue;
            }
            let after = Cell { flags, ..before };
            self.scene.grid.cells[index] = after;
            changes.push(CellChange {
                index,
                before,
                after,
            });
        }
        let what = if blocked { "blocked" } else { "walkable" };
        let summary = match changes.len() {
            0 => format!("No cells changed (already {what})"),
            1 => format!("Marked 1 cell {what}"),
            n => format!("Marked {n} cells {what}"),
        };
        Ok((
            summary,
            (!changes.is_empty()).then_some(Change::Cells(changes)),
        ))
    }

    fn check_free(&self, cell: CellPos) -> Result<(), EditError> {
        if self.scene.grid.index(cell).is_none() {
            return Err(EditError::OutOfBounds(cell));
        }
        if self.scene.portal_at(cell).is_some() {
            return Err(EditError::PortalExists(cell));
        }
        Ok(())
    }

    fn portal_index(&self, id: i32) -> Result<usize, EditError> {
        self.scene
            .portals
            .iter()
            .position(|p| p.id == id)
            .ok_or(EditError::NoPortal(id))
    }

    fn add_portal(
        &mut self,
        cell: CellPos,
        id: Option<i32>,
    ) -> Result<(String, Option<Change>), EditError> {
        self.check_free(cell)?;
        let taken = |i: i32| self.scene.portals.iter().any(|p| p.id == i);
        let id = match id {
            Some(i) if taken(i) => return Err(EditError::PortalIdTaken(i)),
            Some(i) => i,
            None => (0..).find(|i| !taken(*i)).unwrap_or(0),
        };
        let change = self.edit_portals(|ps| ps.push(Portal { cell, id }));
        Ok((
            format!("Added portal {id} at {}, {}", cell.0, cell.1),
            Some(change),
        ))
    }

    fn move_portal(&mut self, id: i32, to: CellPos) -> Result<(String, Option<Change>), EditError> {
        let i = self.portal_index(id)?;
        if self.scene.portals[i].cell == to {
            return Ok((
                format!("Portal {id} is already at {}, {}", to.0, to.1),
                None,
            ));
        }
        self.check_free(to)?;
        let change = self.edit_portals(|ps| ps[i].cell = to);
        Ok((
            format!("Moved portal {id} to {}, {}", to.0, to.1),
            Some(change),
        ))
    }

    fn remove_portal(&mut self, id: i32) -> Result<(String, Option<Change>), EditError> {
        let i = self.portal_index(id)?;
        let change = self.edit_portals(|ps| {
            ps.remove(i);
        });
        Ok((format!("Deleted portal {id}"), Some(change)))
    }

    fn edit_portals(&mut self, edit: impl FnOnce(&mut Vec<Portal>)) -> Change {
        let before = self.scene.portals.clone();
        edit(&mut self.scene.portals);
        Change::Portals {
            before,
            after: self.scene.portals.clone(),
        }
    }

    fn apply(&mut self, change: &Change, forward: bool) {
        match change {
            Change::Cells(cells) => {
                for c in cells {
                    self.scene.grid.cells[c.index] = if forward { c.after } else { c.before };
                }
            }
            Change::Portals { before, after } => {
                self.scene.portals = if forward { after } else { before }.clone();
            }
        }
    }

    /// Runs `commands` in order as one undo step called `label`. Refused commands are
    /// skipped; each result says what happened.
    pub fn execute_batch(
        &mut self,
        label: impl Into<String>,
        commands: &[SceneCommand],
    ) -> Vec<Result<Outcome, EditError>> {
        self.begin_group(label);
        let results = commands.iter().map(|c| self.execute(c)).collect();
        self.end_group();
        results
    }

    // ---- history ----

    /// Merges the following commands into one undo step called `label` (a brush stroke, an
    /// assistant request), until [`SceneDoc::end_group`].
    pub fn begin_group(&mut self, label: impl Into<String>) {
        self.history.begin_group(label.into());
    }

    pub fn end_group(&mut self) {
        self.history.end_group();
    }

    /// Reverts the last step; returns its label.
    pub fn undo(&mut self) -> Option<String> {
        let step = self.history.take_undo()?;
        for change in step.changes.iter().rev() {
            self.apply(change, false);
        }
        let label = step.label.clone();
        self.history.undone(step);
        Some(label)
    }

    /// Reapplies the last undone step; returns its label.
    pub fn redo(&mut self) -> Option<String> {
        let step = self.history.take_redo()?;
        for change in &step.changes {
            self.apply(change, true);
        }
        let label = step.label.clone();
        self.history.redone(step);
        Some(label)
    }

    pub fn can_undo(&self) -> bool {
        self.history.undo_label().is_some()
    }

    pub fn can_redo(&self) -> bool {
        self.history.redo_label().is_some()
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

    pub fn is_dirty(&self) -> bool {
        self.history.is_dirty()
    }

    /// Call after the scene was written successfully.
    pub fn mark_saved(&mut self) {
        self.history.mark_saved();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_scene::{Grid, IsoProjection};

    fn doc() -> SceneDoc {
        SceneDoc::new(Scene {
            grid: Grid::new(8, 8),
            projection: IsoProjection::for_grid(64.0, 32.0, 8),
            ground: None,
            portals: Vec::new(),
            entities: Vec::new(),
            layers: Vec::new(),
            meta: Default::default(),
        })
    }

    fn blocked(d: &SceneDoc) -> usize {
        d.scene()
            .grid
            .cells
            .iter()
            .filter(|c| c.is_blocked())
            .count()
    }

    fn block(center: CellPos, radius: i32) -> SceneCommand {
        SceneCommand::SetBlocked {
            area: Area::Square { center, radius },
            blocked: true,
        }
    }

    #[test]
    fn group_is_one_undo_step() {
        let mut d = doc();
        d.begin_group("Paint blocked");
        d.execute(&block((2, 2), 1)).unwrap();
        d.execute(&block((3, 2), 1)).unwrap();
        d.end_group();
        assert_eq!(blocked(&d), 12);
        assert!(d.is_dirty());
        assert_eq!(d.undo().as_deref(), Some("Paint blocked"));
        assert_eq!(blocked(&d), 0);
        assert!(!d.is_dirty());
        d.redo();
        assert_eq!(blocked(&d), 12);
    }

    #[test]
    fn areas_clip_to_the_map_and_no_ops_record_nothing() {
        let mut d = doc();
        d.execute(&block((0, 0), 2)).unwrap();
        assert_eq!(blocked(&d), 9);
        let again = d.execute(&block((0, 0), 2)).unwrap();
        assert!(!again.changed);
        assert_eq!(d.history().len(), 1);
        let rect = SceneCommand::SetBlocked {
            area: Area::Rect {
                min: (-100, 6),
                max: (100, 100),
            },
            blocked: true,
        };
        assert_eq!(d.execute(&rect).unwrap().summary, "Marked 16 cells blocked");
    }

    #[test]
    fn new_edit_after_undo_drops_redo_and_saved_state() {
        let mut d = doc();
        d.execute(&block((1, 1), 0)).unwrap();
        d.mark_saved();
        d.undo();
        d.execute(&block((5, 5), 0)).unwrap();
        assert!(!d.can_redo());
        assert!(d.is_dirty());
    }

    #[test]
    fn portals_by_id_with_checks_and_undo() {
        let mut d = doc();
        let add = |cell| SceneCommand::AddPortal { cell, id: None };
        d.execute(&add((1, 1))).unwrap();
        assert_eq!(
            d.execute(&add((1, 1))),
            Err(EditError::PortalExists((1, 1)))
        );
        assert_eq!(
            d.execute(&add((99, 1))),
            Err(EditError::OutOfBounds((99, 1)))
        );
        d.execute(&add((2, 2))).unwrap();
        assert_eq!(d.scene().portals[1].id, 1);
        d.execute(&SceneCommand::MovePortal { id: 0, to: (4, 4) })
            .unwrap();
        d.execute(&SceneCommand::RemovePortal { id: 1 }).unwrap();
        assert_eq!(
            d.execute(&SceneCommand::RemovePortal { id: 1 }),
            Err(EditError::NoPortal(1))
        );
        assert_eq!(d.scene().portals.len(), 1);
        d.undo();
        d.undo();
        assert_eq!(d.scene().portal_at((1, 1)), Some(0));
        let labels: Vec<_> = d.history().into_iter().map(|e| (e.label, e.done)).collect();
        assert_eq!(
            labels,
            [
                ("Added portal 0 at 1, 1".to_owned(), true),
                ("Added portal 1 at 2, 2".to_owned(), true),
                ("Moved portal 0 to 4, 4".to_owned(), false),
                ("Deleted portal 1".to_owned(), false),
            ]
        );
    }

    #[test]
    fn batch_is_one_step_and_skips_refused_commands() {
        let mut d = doc();
        let commands = crate::parse_scene_commands(
            r#"[{"op":"add_portal","cell":[1,1]},
                {"op":"add_portal","cell":[1,1]},
                {"op":"set_blocked","area":{"shape":"cells","cells":[[3,3]]},"blocked":true}]"#,
        )
        .unwrap();
        let results = d.execute_batch("Script", &commands);
        assert!(results[0].is_ok() && results[1].is_err() && results[2].is_ok());
        assert_eq!(d.history().len(), 1);
        assert_eq!(d.undo().as_deref(), Some("Script"));
        assert!(d.scene().portals.is_empty() && blocked(&d) == 0);
        assert_eq!(
            crate::parse_scene_commands(r#"{"op":"remove_portal","id":2}"#).unwrap(),
            [SceneCommand::RemovePortal { id: 2 }]
        );
        assert!(crate::parse_scene_commands(r#"[{"op":"fly"}]"#).is_err());
    }

    #[test]
    fn commands_round_trip_through_json() {
        let json = r#"{"op":"set_blocked","area":{"shape":"rect","min":[1,2],"max":[3,4]},"blocked":false}"#;
        let cmd: SceneCommand = serde_json::from_str(json).unwrap();
        assert_eq!(
            cmd,
            SceneCommand::SetBlocked {
                area: Area::Rect {
                    min: (1, 2),
                    max: (3, 4)
                },
                blocked: false
            }
        );
        assert_eq!(serde_json::to_string(&cmd).unwrap(), json);
        let add: SceneCommand =
            serde_json::from_str(r#"{"op":"add_portal","cell":[5,6]}"#).unwrap();
        assert_eq!(
            add,
            SceneCommand::AddPortal {
                cell: (5, 6),
                id: None
            }
        );
        let schema = crate::scene_command_schema().to_string();
        assert!(
            schema.contains("set_blocked") && schema.contains("remove_portal"),
            "{schema}"
        );
    }
}
