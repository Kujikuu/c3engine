//! An open map document: an engine [`Scene`] plus its tile texture cache and undo history.
//! Format-agnostic: loading and saving go through `tq_plugin`.

use c3_assets::Image;
use c3_scene::{Cell, CellPos, Portal, Scene};
use eframe::egui::{self, Pos2, Rect, TextureHandle, Vec2};
use std::collections::HashMap;

/// Result of asking the cache for a ground tile.
pub enum TileLookup {
    Ready(TextureHandle),
    /// Not decoded yet (decode budget spent this frame).
    Pending,
    /// Missing or undecodable.
    Failed,
}

/// One undoable step.
enum Change {
    Cells(Vec<CellChange>),
    Portals {
        before: Vec<Portal>,
        after: Vec<Portal>,
    },
}

#[derive(Clone, Copy)]
struct CellChange {
    index: usize,
    before: Cell,
    after: Cell,
}

#[derive(Default)]
struct History {
    undo: Vec<Change>,
    redo: Vec<Change>,
    /// Cell edits of the brush stroke in progress (merged into one undo step).
    stroke: Vec<CellChange>,
    /// `undo.len()` at the last save; `None` if that state is no longer reachable.
    saved_at: Option<usize>,
}

pub struct MapDoc {
    /// Asset path the scene was opened from (and is saved to).
    pub path: String,
    pub scene: Scene,
    /// Non-fatal load problems, shown in the inspector.
    pub warnings: Vec<String>,
    tiles: HashMap<u16, Option<TextureHandle>>,
    history: History,
}

impl MapDoc {
    pub fn new(path: &str, scene: Scene, warnings: Vec<String>) -> Self {
        Self {
            path: path.to_owned(),
            scene,
            warnings,
            tiles: HashMap::new(),
            history: History {
                saved_at: Some(0),
                ..Default::default()
            },
        }
    }

    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    // ---- geometry (egui types) ----

    pub fn cell_to_world(&self, cell: CellPos) -> Pos2 {
        let (x, y) = self.scene.cell_to_world(cell);
        Pos2::new(x, y)
    }

    pub fn world_to_cell(&self, p: Pos2) -> CellPos {
        self.scene.world_to_cell((p.x, p.y))
    }

    pub fn cell(&self, pos: CellPos) -> Option<Cell> {
        self.scene.grid.get(pos).copied()
    }

    /// Ground layer bounds in world space, or the grid's bounds if there is no ground.
    pub fn bounds(&self) -> Rect {
        match self.scene.ground.as_ref().filter(|g| g.width > 0) {
            Some(g) => {
                let (w, h) = g.world_size();
                Rect::from_min_size(
                    Pos2::new(g.origin.0 as f32, g.origin.1 as f32),
                    Vec2::new(w as f32, h as f32),
                )
            }
            None => {
                let (gw, gh) = (self.scene.grid.width as i32, self.scene.grid.height as i32);
                let corners = [(0, 0), (gw - 1, 0), (0, gh - 1), (gw - 1, gh - 1)]
                    .map(|c| self.cell_to_world(c));
                Rect::from_points(&corners)
            }
        }
    }

    // ---- ground tile cache ----

    pub fn loaded_tiles(&self) -> usize {
        self.tiles.len()
    }

    pub fn used_tiles(&self) -> usize {
        self.scene.ground.as_ref().map_or(0, |g| g.unique_tiles())
    }

    /// Returns the texture for a ground tile, decoding it with `load` if `budget` allows.
    pub fn tile(
        &mut self,
        ctx: &egui::Context,
        id: u16,
        budget: &mut usize,
        load: impl Fn(&str) -> Option<Image>,
    ) -> TileLookup {
        if let Some(t) = self.tiles.get(&id) {
            return t.clone().map_or(TileLookup::Failed, TileLookup::Ready);
        }
        if *budget == 0 {
            return TileLookup::Pending;
        }
        *budget -= 1;
        let frame = self
            .scene
            .ground
            .as_ref()
            .and_then(|g| g.tileset.frames.get(&id));
        let tex = frame.and_then(|f| load(f)).map(|img| {
            let color = egui::ColorImage::from_rgba_unmultiplied(
                [img.width as usize, img.height as usize],
                &img.rgba,
            );
            ctx.load_texture(format!("tile{id}"), color, egui::TextureOptions::LINEAR)
        });
        self.tiles.insert(id, tex.clone());
        tex.map_or(TileLookup::Failed, TileLookup::Ready)
    }

    // ---- editing ----

    /// Sets walkability of every cell in a square brush (a diamond on screen).
    /// Changes accumulate into the current stroke until `end_stroke`.
    pub fn paint(&mut self, center: CellPos, radius: i32, blocked: bool) {
        let flags = u16::from(blocked);
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let Some(index) = self.scene.grid.index((center.0 + dx, center.1 + dy)) else {
                    continue;
                };
                let before = self.scene.grid.cells[index];
                if before.flags == flags {
                    continue;
                }
                let after = Cell { flags, ..before };
                self.scene.grid.cells[index] = after;
                self.history.stroke.push(CellChange {
                    index,
                    before,
                    after,
                });
            }
        }
    }

    /// Commits the stroke in progress as one undo step.
    pub fn end_stroke(&mut self) {
        let stroke = std::mem::take(&mut self.history.stroke);
        if !stroke.is_empty() {
            self.push(Change::Cells(stroke));
        }
    }

    pub fn portal_at(&self, pos: CellPos) -> Option<usize> {
        self.scene.portal_at(pos)
    }

    /// Adds a portal with the next free id. Returns false if the cell is invalid or taken.
    pub fn add_portal(&mut self, cell: CellPos) -> bool {
        if self.scene.grid.index(cell).is_none() || self.portal_at(cell).is_some() {
            return false;
        }
        let id = (0..)
            .find(|i| self.scene.portals.iter().all(|p| p.id != *i))
            .unwrap();
        self.edit_portals(|ps| ps.push(Portal { cell, id }));
        true
    }

    pub fn remove_portal(&mut self, i: usize) {
        self.edit_portals(|ps| {
            ps.remove(i);
        });
    }

    pub fn move_portal(&mut self, i: usize, to: CellPos) -> bool {
        if self.scene.grid.index(to).is_none() || self.portal_at(to).is_some() {
            return false;
        }
        self.edit_portals(|ps| ps[i].cell = to);
        true
    }

    fn edit_portals(&mut self, f: impl FnOnce(&mut Vec<Portal>)) {
        let before = self.scene.portals.clone();
        f(&mut self.scene.portals);
        let after = self.scene.portals.clone();
        self.push(Change::Portals { before, after });
    }

    fn push(&mut self, change: Change) {
        let h = &mut self.history;
        if h.saved_at.is_some_and(|s| s > h.undo.len()) {
            h.saved_at = None; // the saved state was in the redo stack we're discarding
        }
        h.redo.clear();
        h.undo.push(change);
    }

    pub fn can_undo(&self) -> bool {
        !self.history.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.history.redo.is_empty()
    }

    pub fn undo(&mut self) {
        self.end_stroke();
        if let Some(c) = self.history.undo.pop() {
            self.apply(&c, false);
            self.history.redo.push(c);
        }
    }

    pub fn redo(&mut self) {
        if let Some(c) = self.history.redo.pop() {
            self.apply(&c, true);
            self.history.undo.push(c);
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

    pub fn is_dirty(&self) -> bool {
        self.history.saved_at != Some(self.history.undo.len()) || !self.history.stroke.is_empty()
    }

    /// Call after the scene was written successfully.
    pub fn mark_saved(&mut self) {
        self.end_stroke();
        self.history.saved_at = Some(self.history.undo.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_scene::{Grid, IsoProjection};

    fn doc() -> MapDoc {
        let scene = Scene {
            grid: Grid::new(8, 8),
            projection: IsoProjection::for_grid(64.0, 32.0, 8),
            ground: None,
            portals: Vec::new(),
            entities: Vec::new(),
            layers: Vec::new(),
            meta: Default::default(),
        };
        MapDoc::new("map/map/test.DMap", scene, Vec::new())
    }

    fn blocked(d: &MapDoc) -> usize {
        d.scene.grid.cells.iter().filter(|c| c.is_blocked()).count()
    }

    #[test]
    fn stroke_is_one_undo_step() {
        let mut d = doc();
        d.paint((2, 2), 1, true);
        d.paint((3, 2), 1, true);
        d.end_stroke();
        assert_eq!(blocked(&d), 12);
        assert!(d.is_dirty());
        d.undo();
        assert_eq!(blocked(&d), 0);
        assert!(!d.is_dirty());
        d.redo();
        assert_eq!(blocked(&d), 12);
    }

    #[test]
    fn brush_clips_at_map_edge() {
        let mut d = doc();
        d.paint((0, 0), 2, true);
        d.end_stroke();
        assert_eq!(blocked(&d), 9);
    }

    #[test]
    fn new_edit_after_undo_drops_redo_and_saved_state() {
        let mut d = doc();
        d.paint((1, 1), 0, true);
        d.mark_saved();
        d.undo();
        d.paint((5, 5), 0, true);
        d.end_stroke();
        assert!(!d.can_redo());
        assert!(d.is_dirty());
    }

    #[test]
    fn portals_add_move_remove_undo() {
        let mut d = doc();
        assert!(d.add_portal((1, 1)));
        assert!(!d.add_portal((1, 1)), "cell already has a portal");
        assert!(!d.add_portal((99, 1)), "outside the map");
        assert!(d.add_portal((2, 2)));
        assert_eq!(d.scene.portals[1].id, 1);
        assert!(d.move_portal(0, (4, 4)));
        d.remove_portal(1);
        assert_eq!(d.scene.portals.len(), 1);
        d.undo();
        d.undo();
        assert_eq!(d.portal_at((1, 1)), Some(0));
    }

    #[test]
    fn backup_written_once_and_save_is_atomic() {
        let dir = std::env::temp_dir().join(format!("c3studio-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("x.DMap");
        std::fs::write(&file, b"original").unwrap();
        crate::tq_plugin::write_with_backup(&file, b"v1").unwrap();
        crate::tq_plugin::write_with_backup(&file, b"v2").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"v2");
        assert_eq!(std::fs::read(dir.join("x.DMap.bak")).unwrap(), b"original");
        assert!(!dir.join("x.DMap.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
