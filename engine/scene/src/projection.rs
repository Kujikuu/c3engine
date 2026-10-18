//! Isometric (diamond) projection between cells and world pixels.

use crate::CellPos;

/// Cell (x, y) centre = (cell_w*(x-y)/2 + origin_x, cell_h*(x+y)/2 + origin_y).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IsoProjection {
    pub cell_w: f32,
    pub cell_h: f32,
    /// World position of cell (0, 0).
    pub origin: (f32, f32),
}

impl IsoProjection {
    /// Standard layout for a grid `width` cells wide: cell (0,0) at the top corner.
    pub fn for_grid(cell_w: f32, cell_h: f32, width: u32) -> Self {
        Self {
            cell_w,
            cell_h,
            origin: (cell_w * width as f32 / 2.0, cell_h / 2.0),
        }
    }

    pub fn cell_to_world(&self, (x, y): CellPos) -> (f32, f32) {
        (
            self.cell_w * (x - y) as f32 / 2.0 + self.origin.0,
            self.cell_h * (x + y) as f32 / 2.0 + self.origin.1,
        )
    }

    /// Cell whose diamond contains the world point.
    pub fn world_to_cell(&self, (wx, wy): (f32, f32)) -> CellPos {
        let a = (wx - self.origin.0) / (self.cell_w / 2.0);
        let b = (wy - self.origin.1) / (self.cell_h / 2.0);
        (
            ((a + b) / 2.0).round() as i32,
            ((b - a) / 2.0).round() as i32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_cell_centres() {
        let p = IsoProjection::for_grid(64.0, 32.0, 100);
        for cell in [(0, 0), (5, 9), (99, 0), (42, 77)] {
            assert_eq!(p.world_to_cell(p.cell_to_world(cell)), cell);
        }
    }
}
