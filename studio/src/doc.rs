//! An open map document: a `c3_edit::SceneDoc` (scene + command history) plus where it came
//! from. Every change goes through commands.

use crate::maps::MapRef;
use c3_edit::{EditError, HistoryEntry, Outcome, SceneCommand, SceneDoc};
use c3_scene::{Cell, CellPos, Scene};
use eframe::egui::{Pos2, Rect, Vec2};

pub struct MapDoc {
    /// Where the map lives; a client map becomes a project map when first saved.
    pub map: MapRef,
    /// Non-fatal load problems, shown in the inspector.
    pub warnings: Vec<String>,
    edit: SceneDoc,
    /// The ground as last saved (a classic map's ground is its own file, written when it
    /// changed).
    saved_ground: Option<c3_scene::TileLayer>,
}

impl MapDoc {
    pub fn new(map: MapRef, scene: Scene, warnings: Vec<String>) -> Self {
        Self {
            map,
            warnings,
            saved_ground: scene.ground.clone(),
            edit: SceneDoc::new(scene),
        }
    }

    pub fn name(&self) -> &str {
        self.map.name()
    }

    pub fn scene(&self) -> &Scene {
        self.edit.scene()
    }

    /// The scene's document, for commands from elsewhere (an agent's).
    pub fn scene_doc(&mut self) -> &mut SceneDoc {
        &mut self.edit
    }

    /// The ground layer, when it changed since the last save.
    pub fn changed_ground(&self) -> Option<&c3_scene::TileLayer> {
        self.scene()
            .ground
            .as_ref()
            .filter(|g| Some(*g) != self.saved_ground.as_ref())
    }

    // ---- geometry (egui types) ----

    pub fn cell_to_world(&self, cell: CellPos) -> Pos2 {
        let (x, y) = self.scene().cell_to_world(cell);
        Pos2::new(x, y)
    }

    pub fn world_to_cell(&self, p: Pos2) -> CellPos {
        self.scene().world_to_cell((p.x, p.y))
    }

    pub fn cell(&self, pos: CellPos) -> Option<Cell> {
        self.scene().grid.get(pos).copied()
    }

    /// Id of the portal on `pos`, if any.
    pub fn portal_id_at(&self, pos: CellPos) -> Option<i32> {
        let scene = self.scene();
        scene.portal_at(pos).map(|i| scene.portals[i].id)
    }

    /// Ground layer bounds in world space, or the grid's bounds if there is no ground.
    pub fn bounds(&self) -> Rect {
        let scene = self.scene();
        match scene.ground.as_ref().filter(|g| g.width > 0) {
            Some(g) => {
                let (w, h) = g.world_size();
                Rect::from_min_size(
                    Pos2::new(g.origin.0 as f32, g.origin.1 as f32),
                    Vec2::new(w as f32, h as f32),
                )
            }
            None => {
                let (gw, gh) = (scene.grid.width as i32, scene.grid.height as i32);
                let corners = [(0, 0), (gw - 1, 0), (0, gh - 1), (gw - 1, gh - 1)]
                    .map(|c| self.cell_to_world(c));
                Rect::from_points(&corners)
            }
        }
    }

    pub fn used_tiles(&self) -> usize {
        self.scene().ground.as_ref().map_or(0, |g| g.unique_tiles())
    }

    // ---- editing (commands) ----

    pub fn execute(&mut self, command: &SceneCommand) -> Result<Outcome, EditError> {
        self.edit.execute(command)
    }

    pub fn execute_batch(
        &mut self,
        label: &str,
        commands: &[SceneCommand],
    ) -> Vec<Result<Outcome, EditError>> {
        self.edit.execute_batch(label, commands)
    }

    pub fn begin_group(&mut self, label: impl Into<String>) {
        self.edit.begin_group(label);
    }

    pub fn end_group(&mut self) {
        self.edit.end_group();
    }

    pub fn undo(&mut self) -> Option<String> {
        self.edit.undo()
    }

    pub fn redo(&mut self) -> Option<String> {
        self.edit.redo()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.edit.undo_label()
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.edit.redo_label()
    }

    pub fn history(&self) -> Vec<HistoryEntry> {
        self.edit.history()
    }

    pub fn is_dirty(&self) -> bool {
        self.edit.is_dirty()
    }

    /// Call after the scene was written successfully.
    pub fn mark_saved(&mut self) {
        self.saved_ground = self.scene().ground.clone();
        self.edit.mark_saved();
    }
}
