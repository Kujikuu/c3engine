//! An open map document: DMap + ground data, tile texture cache, edits with undo/redo, saving.
//! World space matches the client: cell (x, y) centre =
//! (CELL_W*(x-y)/2 + origin_x, CELL_H*(x+y)/2 + origin_y), origin = (CELL_W*width/2, CELL_H/2).
//! The puzzle is placed at `bg_origin()` (client `GetBgWorldPos`).

use c3_formats_tq::{
    ani::Ani,
    dmap::{Cell, DMap, Passage},
    pul::{EMPTY, GRID_SIZE, Puzzle},
    texture::Image,
    vfs::ClientFs,
};
use eframe::egui::{self, Pos2, Rect, TextureHandle, Vec2};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const CELL_W: f32 = 64.0;
pub const CELL_H: f32 = 32.0;

pub type CellPos = (i32, i32);

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
        before: Vec<Passage>,
        after: Vec<Passage>,
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
    pub path: String,
    pub dmap: DMap,
    pub puzzle: Puzzle,
    pub ani: Ani,
    pub used_tiles: usize,
    tiles: HashMap<u16, Option<TextureHandle>>,
    history: History,
}

impl MapDoc {
    pub fn load(fs: &ClientFs, path: &str) -> Result<Self, String> {
        let read = |p: &str| fs.read(p).ok_or_else(|| format!("File not found: {p}"));
        let dmap = DMap::parse(&read(path)?).map_err(|e| format!("{path}: {e}"))?;
        let pul_path = dmap.puzzle_file.as_str();
        let puzzle = Puzzle::parse(&read(&pul_path)?).map_err(|e| format!("{pul_path}: {e}"))?;
        let ani_path = puzzle.ani_file.as_str();
        let ani = Ani::parse(&read(&ani_path)?).map_err(|e| format!("{ani_path}: {e}"))?;
        Ok(Self::from_parts(path, dmap, puzzle, ani))
    }

    pub fn from_parts(path: &str, dmap: DMap, puzzle: Puzzle, ani: Ani) -> Self {
        let mut used: Vec<u16> = puzzle
            .tiles
            .iter()
            .copied()
            .filter(|&t| t != EMPTY)
            .collect();
        used.sort_unstable();
        used.dedup();
        Self {
            path: path.to_owned(),
            dmap,
            puzzle,
            ani,
            used_tiles: used.len(),
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

    // ---- geometry ----

    pub fn origin(&self) -> Vec2 {
        Vec2::new(CELL_W * self.dmap.width as f32 / 2.0, CELL_H / 2.0)
    }

    pub fn cell_to_world(&self, (x, y): CellPos) -> Pos2 {
        let o = self.origin();
        Pos2::new(
            CELL_W * (x - y) as f32 / 2.0 + o.x,
            CELL_H * (x + y) as f32 / 2.0 + o.y,
        )
    }

    pub fn world_to_cell(&self, p: Pos2) -> CellPos {
        let o = self.origin();
        let a = (p.x - o.x) / (CELL_W / 2.0);
        let b = (p.y - o.y) / (CELL_H / 2.0);
        (
            ((a + b) / 2.0).round() as i32,
            ((b - a) / 2.0).round() as i32,
        )
    }

    fn cell_index(&self, (x, y): CellPos) -> Option<usize> {
        let (w, h) = (self.dmap.width as i32, self.dmap.height as i32);
        (x >= 0 && y >= 0 && x < w && y < h).then(|| (y * w + x) as usize)
    }

    pub fn cell(&self, pos: CellPos) -> Option<Cell> {
        self.cell_index(pos).map(|i| self.dmap.cells[i])
    }

    /// Puzzle size in world pixels.
    pub fn bg_size(&self) -> Vec2 {
        Vec2::new(self.puzzle.width as f32, self.puzzle.height as f32) * GRID_SIZE as f32
    }

    /// World position of the puzzle's top-left. Port of `CGameMap::GetBgWorldPos`
    /// (integer math as in the client).
    pub fn bg_origin(&self) -> Pos2 {
        let (bw, bh) = (
            self.puzzle.width as i32 * GRID_SIZE as i32,
            self.puzzle.height as i32 * GRID_SIZE as i32,
        );
        let (w, h) = (self.dmap.width as i32, self.dmap.height as i32);
        let (cw, ch) = (CELL_W as i32, CELL_H as i32);
        let x = cw * w / 2 - bw / 2;
        let y = ch / 2 + ch * h / 2 - bh / 2 - ((h + 1) % 2) * ch / 2;
        Pos2::new(x as f32, y as f32)
    }

    pub fn bg_rect(&self) -> Rect {
        Rect::from_min_size(self.bg_origin(), self.bg_size())
    }

    // ---- ground tile cache ----

    pub fn loaded_tiles(&self) -> usize {
        self.tiles.len()
    }

    /// Returns the texture for a puzzle tile, decoding it if `budget` allows.
    pub fn tile(
        &mut self,
        ctx: &egui::Context,
        fs: &ClientFs,
        idx: u16,
        budget: &mut usize,
    ) -> TileLookup {
        if let Some(t) = self.tiles.get(&idx) {
            return t.clone().map_or(TileLookup::Failed, TileLookup::Ready);
        }
        if *budget == 0 {
            return TileLookup::Pending;
        }
        *budget -= 1;
        let tex = (|| {
            let frame = self.ani.get(&Puzzle::tile_title(idx))?.frame_str(0)?;
            let img = Image::decode(&fs.read(&frame)?).ok()?;
            let size = [img.width as usize, img.height as usize];
            let color = egui::ColorImage::from_rgba_unmultiplied(size, &img.rgba);
            Some(ctx.load_texture(format!("puzzle{idx}"), color, egui::TextureOptions::LINEAR))
        })();
        self.tiles.insert(idx, tex.clone());
        tex.map_or(TileLookup::Failed, TileLookup::Ready)
    }

    // ---- editing ----

    /// Sets the walkability of every cell in a square brush (a diamond on screen).
    /// Changes accumulate into the current stroke until `end_stroke`.
    pub fn paint(&mut self, center: CellPos, radius: i32, blocked: bool) {
        let mask = u16::from(blocked);
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let Some(index) = self.cell_index((center.0 + dx, center.1 + dy)) else {
                    continue;
                };
                let before = self.dmap.cells[index];
                if before.mask == mask {
                    continue;
                }
                let after = Cell { mask, ..before };
                self.dmap.cells[index] = after;
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
        self.dmap.passages.iter().position(|p| (p.x, p.y) == pos)
    }

    /// Adds a portal with the next free index. Returns false if the cell is invalid or taken.
    pub fn add_portal(&mut self, pos: CellPos) -> bool {
        if self.cell_index(pos).is_none() || self.portal_at(pos).is_some() {
            return false;
        }
        let index = (0..)
            .find(|i| self.dmap.passages.iter().all(|p| p.index != *i))
            .unwrap();
        self.edit_portals(|ps| {
            ps.push(Passage {
                x: pos.0,
                y: pos.1,
                index,
            })
        });
        true
    }

    pub fn remove_portal(&mut self, i: usize) {
        self.edit_portals(|ps| {
            ps.remove(i);
        });
    }

    pub fn move_portal(&mut self, i: usize, to: CellPos) -> bool {
        if self.cell_index(to).is_none() || self.portal_at(to).is_some() {
            return false;
        }
        self.edit_portals(|ps| {
            ps[i].x = to.0;
            ps[i].y = to.1;
        });
        true
    }

    fn edit_portals(&mut self, f: impl FnOnce(&mut Vec<Passage>)) {
        let before = self.dmap.passages.clone();
        f(&mut self.dmap.passages);
        let after = self.dmap.passages.clone();
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
                    self.dmap.cells[c.index] = if forward { c.after } else { c.before };
                }
            }
            Change::Portals { before, after } => {
                self.dmap.passages = if forward { after } else { before }.clone();
            }
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.history.saved_at != Some(self.history.undo.len()) || !self.history.stroke.is_empty()
    }

    // ---- saving ----

    /// File the map is saved to: the loose client file (the client prefers loose files over archives).
    pub fn save_path(&self, client_root: &Path) -> PathBuf {
        client_root.join(Path::new(&self.path))
    }

    /// Writes the DMap. Before the first overwrite, keeps the original as `<file>.bak`.
    pub fn save(&mut self, client_root: &Path) -> Result<PathBuf, String> {
        self.end_stroke();
        let path = self.save_path(client_root);
        let backup = path.with_extension("DMap.bak");
        if path.exists() && !backup.exists() {
            std::fs::copy(&path, &backup).map_err(|e| format!("Backup failed: {e}"))?;
        }
        let bytes = self.dmap.to_bytes();
        // Write to a temp file first so a failed write never corrupts the map.
        let tmp = path.with_extension("DMap.tmp");
        std::fs::write(&tmp, &bytes).map_err(|e| format!("Save failed: {e}"))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("Save failed: {e}"))?;
        self.history.saved_at = Some(self.history.undo.len());
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> MapDoc {
        MapDoc::from_parts(
            "map/map/test.DMap",
            DMap::new(8, 8, "map/puzzle/test.pul"),
            Puzzle::new(1, 1, "ani/test.ani"),
            Ani::default(),
        )
    }

    fn blocked(d: &MapDoc) -> usize {
        d.dmap.cells.iter().filter(|c| c.mask != 0).count()
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
        d.end_stroke();
        d.history.saved_at = Some(1); // pretend saved here
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
        assert_eq!(d.dmap.passages[1].index, 1);
        assert!(d.move_portal(0, (4, 4)));
        d.remove_portal(1);
        assert_eq!(d.dmap.passages.len(), 1);
        d.undo();
        d.undo();
        assert_eq!(d.portal_at((1, 1)), Some(0));
    }

    #[test]
    fn save_writes_loadable_map_and_keeps_backup() {
        let root = std::env::temp_dir().join(format!("c3studio-test-{}", std::process::id()));
        let file = root.join("map/map/test.DMap");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let original = DMap::new(8, 8, "map/puzzle/test.pul").to_bytes();
        std::fs::write(&file, &original).unwrap();

        let mut d = doc();
        d.paint((3, 3), 0, true);
        d.add_portal((6, 6));
        assert_eq!(d.save(&root).unwrap(), file);
        assert!(!d.is_dirty());

        let saved = DMap::parse(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(saved.cell(3, 3).unwrap().mask, 1);
        assert_eq!(saved.passages.len(), 1);
        assert_eq!(
            std::fs::read(file.with_extension("DMap.bak")).unwrap(),
            original
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
