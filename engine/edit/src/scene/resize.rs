//! Resizing a map: the grid grows or is cut at its far edges (cells keep their place, so
//! the projection and everything placed in world space stay put).

use super::{Change, Edit, SceneDoc};
use crate::EditError;
use c3_scene::{CellPos, Grid, Scene, TileLayer};

/// Cells along a side of the smallest and the largest map.
pub const MAP_SIDES: std::ops::RangeInclusive<u32> = 8..=4096;

impl SceneDoc {
    pub(super) fn resize(&mut self, width: u32, height: u32) -> Edit {
        if !MAP_SIDES.contains(&width) || !MAP_SIDES.contains(&height) {
            return Err(EditError::InvalidArea(format!(
                "a map is {} to {} cells on a side",
                MAP_SIDES.start(),
                MAP_SIDES.end()
            )));
        }
        let old = &self.scene.grid;
        if (old.width, old.height) == (width, height) {
            return Ok(("Nothing changed".into(), None));
        }
        let after = resized(&self.scene, width, height);
        let summary = format!("Resize the map to {width} × {height}");
        let before = std::mem::replace(&mut self.scene, after.clone());
        let change = Change::Whole {
            before: Box::new(before),
            after: Box::new(after),
        };
        Ok((summary, Some(change)))
    }
}

fn resized(scene: &Scene, width: u32, height: u32) -> Scene {
    let mut grid = Grid::new(width, height);
    for y in 0..height.min(scene.grid.height) {
        for x in 0..width.min(scene.grid.width) {
            let cell = (x as i32, y as i32);
            if let (Some(i), Some(c)) = (grid.index(cell), scene.grid.get(cell)) {
                grid.cells[i] = *c;
            }
        }
    }
    let on = |cell: CellPos| grid.index(cell).is_some();
    let mut out = scene.clone();
    out.portals.retain(|p| on(p.cell));
    out.npcs.retain(|n| on(n.cell));
    out.spawns.retain(|s| on(s.min));
    for s in &mut out.spawns {
        s.max = (
            s.max.0.min(width as i32 - 1),
            s.max.1.min(height as i32 - 1),
        );
    }
    if let Some(ground) = &scene.ground
        && let Some(image) = ground.single_image()
    {
        out.ground = Some(TileLayer::covering(
            image,
            ground.tile_size,
            &scene.projection,
            &grid,
        ));
    }
    out.grid = grid;
    out
}

#[cfg(test)]
mod tests {
    use crate::SceneCommand;
    use crate::scene::population::tests::npc;
    use crate::scene::tests::doc;
    use c3_scene::Portal;

    #[test]
    fn resizing_keeps_cells_and_drops_what_falls_off() {
        let mut d = doc();
        d.execute(&SceneCommand::SetBlocked {
            area: crate::Area::Square {
                center: (1, 1),
                radius: 0,
            },
            blocked: true,
        })
        .unwrap();
        d.execute(&SceneCommand::AddPortal {
            cell: (7, 7),
            id: None,
        })
        .unwrap();
        d.execute(&SceneCommand::AddNpc {
            npc: npc(0, (2, 2)),
        })
        .unwrap();
        assert!(
            d.execute(&SceneCommand::Resize {
                width: 2,
                height: 9
            })
            .is_err()
        );
        d.execute(&SceneCommand::Resize {
            width: 12,
            height: 8,
        })
        .unwrap();
        let s = d.scene();
        assert_eq!((s.grid.width, s.grid.height), (12, 8));
        assert!(s.grid.get((1, 1)).unwrap().is_blocked());
        assert_eq!(s.portals.len(), 1);
        d.execute(&SceneCommand::Resize {
            width: 8,
            height: 8,
        })
        .unwrap();
        d.undo();
        assert_eq!(d.scene().grid.width, 12);
        let _: Option<&Portal> = d.scene().portals.first();
        assert_eq!(d.scene().npcs.len(), 1);
    }
}
