//! Cell grid: per-cell walkability, surface type and height.

use crate::CellPos;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cell {
    /// Movement flags; non-zero means blocked. Kept as raw bits so importers lose nothing.
    pub flags: u16,
    /// Surface/terrain type (footstep sounds, effects).
    pub surface: u16,
    /// Height offset.
    pub height: i16,
}

impl Cell {
    pub fn is_blocked(&self) -> bool {
        self.flags != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    pub width: u32,
    pub height: u32,
    /// Row-major: `cells[y * width + x]`.
    pub cells: Vec<Cell>,
}

impl Grid {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            cells: vec![Cell::default(); (width * height) as usize],
        }
    }

    pub fn index(&self, (x, y): CellPos) -> Option<usize> {
        let (w, h) = (self.width as i32, self.height as i32);
        (x >= 0 && y >= 0 && x < w && y < h).then(|| (y * w + x) as usize)
    }

    pub fn get(&self, pos: CellPos) -> Option<&Cell> {
        self.index(pos).map(|i| &self.cells[i])
    }

    pub fn get_mut(&mut self, pos: CellPos) -> Option<&mut Cell> {
        self.index(pos).map(|i| &mut self.cells[i])
    }

    pub fn walkable_ratio(&self) -> f32 {
        let walkable = self.cells.iter().filter(|c| !c.is_blocked()).count();
        walkable as f32 / self.cells.len().max(1) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_bounds() {
        let g = Grid::new(4, 3);
        assert_eq!(g.index((0, 0)), Some(0));
        assert_eq!(g.index((3, 2)), Some(11));
        assert_eq!(g.index((4, 0)), None);
        assert_eq!(g.index((-1, 0)), None);
    }
}
