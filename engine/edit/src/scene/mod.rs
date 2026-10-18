//! Scene (map) editing: commands, the document that applies them, and its undo history.
//! Each kind of edit lives in its own module: `cells` (walkability), `portals`, `objects`
//! (placed entities) and `population` (NPCs, monster spawns).

mod cells;
mod ground;
mod objects;
mod population;
mod portals;
mod resize;

pub use cells::{Area, MAX_AREA_CELLS};
pub use objects::entity_label;
pub use resize::MAP_SIDES;

use crate::history::{History, HistoryEntry};
use crate::{EditError, Outcome};
use c3_scene::{CellPos, Entity, Npc, Portal, PortalTarget, Scene, Spawn};
use cells::CellChange;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A change to a scene. Serialized as `{"op": "set_blocked", ...}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SceneCommand {
    /// Marks every cell of `area` blocked (impassable) or walkable.
    SetBlocked { area: Area, blocked: bool },
    /// Sets the surface type (footsteps, effects) of every cell of `area`.
    SetSurface { area: Area, surface: u16 },
    /// Sets the height of every cell of `area`.
    SetHeight { area: Area, height: i16 },
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
    /// Sets where portal `id` leads (a map id and a cell); `null` leads nowhere.
    SetPortalTarget { id: i32, to: Option<PortalTarget> },
    /// Places an object (sprite, terrain prefab, sound or effect) at `index` in the entity
    /// list, or at the end.
    AddEntity {
        entity: Entity,
        #[serde(default)]
        index: Option<usize>,
    },
    /// Moves entity `index` so it stands on cell `to`.
    MoveEntity { index: usize, to: CellPos },
    /// Replaces entity `index` with `entity`.
    SetEntity { index: usize, entity: Entity },
    /// Deletes entity `index`; later entities move up one place.
    RemoveEntity { index: usize },
    /// Adds an NPC; id 0 picks the lowest free id.
    AddNpc { npc: Npc },
    /// Moves NPC `id` to cell `to`.
    MoveNpc { id: u32, to: CellPos },
    /// Replaces the NPC with `npc.id` by `npc`.
    SetNpc { npc: Npc },
    /// Deletes NPC `id`.
    RemoveNpc { id: u32 },
    /// Adds a monster spawn area; id 0 picks the lowest free id.
    AddSpawn { spawn: Spawn },
    /// Replaces the spawn area with `spawn.id` by `spawn` (moving or resizing it).
    SetSpawn { spawn: Spawn },
    /// Deletes spawn area `id`.
    RemoveSpawn { id: u32 },
    /// Makes the map `width` × `height` cells, growing or cutting at the far edges: cells
    /// keep their place, and portals, NPCs and spawn areas off the new map go. A ground
    /// of one repeated image grows with the map.
    Resize { width: u32, height: u32 },
    /// Sets the ground tiles `[x, y]` (tile columns and rows) to show `image` (added to the
    /// tileset when new), or clears them when `image` is null.
    PaintGround {
        tiles: Vec<(u32, u32)>,
        #[serde(default)]
        image: Option<String>,
    },
    /// Sets every tile joined to `at` that shows what it shows to `image` (null: clears).
    FillGround {
        at: (u32, u32),
        #[serde(default)]
        image: Option<String>,
    },
}

/// A recorded change, with what it replaced.
enum Change {
    Cells(Vec<CellChange>),
    Portals {
        before: Vec<Portal>,
        after: Vec<Portal>,
    },
    /// Entity `index` added (`before` none), removed (`after` none) or replaced.
    Entity {
        index: usize,
        before: Option<Entity>,
        after: Option<Entity>,
    },
    Npcs {
        before: Vec<Npc>,
        after: Vec<Npc>,
    },
    Spawns {
        before: Vec<Spawn>,
        after: Vec<Spawn>,
    },
    /// The ground layer.
    Ground {
        before: Box<c3_scene::TileLayer>,
        after: Box<c3_scene::TileLayer>,
    },
    /// The whole scene (a resize).
    Whole {
        before: Box<Scene>,
        after: Box<Scene>,
    },
}

/// What applying a command did: its summary and the change to record (none = no-op).
type Edit = Result<(String, Option<Change>), EditError>;

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
            SceneCommand::SetSurface { area, surface } => self.set_surface(area, *surface)?,
            SceneCommand::SetHeight { area, height } => self.set_height(area, *height)?,
            SceneCommand::AddPortal { cell, id } => self.add_portal(*cell, *id)?,
            SceneCommand::MovePortal { id, to } => self.move_portal(*id, *to)?,
            SceneCommand::RemovePortal { id } => self.remove_portal(*id)?,
            SceneCommand::SetPortalTarget { id, to } => self.set_portal_target(*id, *to)?,
            SceneCommand::AddEntity { entity, index } => self.add_entity(entity, *index)?,
            SceneCommand::MoveEntity { index, to } => self.move_entity(*index, *to)?,
            SceneCommand::SetEntity { index, entity } => self.set_entity(*index, entity)?,
            SceneCommand::RemoveEntity { index } => self.remove_entity(*index)?,
            SceneCommand::AddNpc { npc } => self.add_npc(npc)?,
            SceneCommand::MoveNpc { id, to } => self.move_npc(*id, *to)?,
            SceneCommand::SetNpc { npc } => self.set_npc(npc)?,
            SceneCommand::RemoveNpc { id } => self.remove_npc(*id)?,
            SceneCommand::AddSpawn { spawn } => self.add_spawn(spawn)?,
            SceneCommand::SetSpawn { spawn } => self.set_spawn(spawn)?,
            SceneCommand::RemoveSpawn { id } => self.remove_spawn(*id)?,
            SceneCommand::Resize { width, height } => self.resize(*width, *height)?,
            SceneCommand::PaintGround { tiles, image } => {
                self.paint_ground(tiles, image.as_deref())?
            }
            SceneCommand::FillGround { at, image } => self.fill_ground(*at, image.as_deref())?,
        };
        let changed = change.is_some();
        if let Some(change) = change {
            self.history.record(summary.clone(), change);
        }
        Ok(Outcome { summary, changed })
    }

    /// `cell` is on the map.
    fn check_cell(&self, cell: CellPos) -> Result<(), EditError> {
        match self.scene.grid.index(cell) {
            Some(_) => Ok(()),
            None => Err(EditError::OutOfBounds(cell)),
        }
    }

    fn apply(&mut self, change: &Change, forward: bool) {
        fn pick<T>(forward: bool, before: T, after: T) -> T {
            if forward { after } else { before }
        }
        match change {
            Change::Cells(cells) => {
                for c in cells {
                    self.scene.grid.cells[c.index] = pick(forward, c.before, c.after);
                }
            }
            Change::Portals { before, after } => {
                self.scene.portals = pick(forward, before, after).clone();
            }
            Change::Entity {
                index,
                before,
                after,
            } => {
                let (from, to) = if forward {
                    (before, after)
                } else {
                    (after, before)
                };
                self.apply_entity(*index, from, to);
            }
            Change::Npcs { before, after } => {
                self.scene.npcs = pick(forward, before, after).clone()
            }
            Change::Spawns { before, after } => {
                self.scene.spawns = pick(forward, before, after).clone()
            }
            Change::Ground { before, after } => {
                self.scene.ground = Some((**pick(forward, before, after)).clone())
            }
            Change::Whole { before, after } => {
                self.scene = (**pick(forward, before, after)).clone()
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
pub(crate) mod tests {
    use super::*;
    use c3_scene::{Grid, IsoProjection};

    pub(crate) fn doc() -> SceneDoc {
        SceneDoc::new(Scene {
            grid: Grid::new(8, 8),
            projection: IsoProjection::for_grid(64.0, 32.0, 8),
            ground: None,
            portals: Vec::new(),
            entities: Vec::new(),
            layers: Vec::new(),
            npcs: Vec::new(),
            spawns: Vec::new(),
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
