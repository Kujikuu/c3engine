//! Walkability edits over areas of cells.

use super::{Change, Edit, SceneDoc};
use crate::EditError;
use c3_scene::{Cell, CellPos, Scene};
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

pub(super) struct CellChange {
    pub index: usize,
    pub before: Cell,
    pub after: Cell,
}

impl SceneDoc {
    pub(super) fn set_blocked(&mut self, area: &Area, blocked: bool) -> Edit {
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
}
