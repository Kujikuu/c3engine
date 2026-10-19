//! Two-way portals: a portal's destination map gets (or reuses) a portal by the arrival
//! point that leads back. Each side's travellers arrive on a walkable cell beside the other
//! portal, never on it, so they don't bounce straight back.

use crate::app::Studio;
use crate::maps::MapRef;
use crate::selection::Selection;
use c3_edit::{SceneCommand, SceneDoc};
use c3_scene::{CellPos, PortalTarget, Scene};

/// Cells around the arrival searched for a portal to reuse.
const REUSE_RADIUS: i32 = 6;
/// Cells around a portal searched for where travellers arrive.
const ARRIVAL_RADIUS: i32 = 4;

impl Studio {
    /// Links portal `id` of the open map with a portal on its destination map, both ways.
    pub(crate) fn link_portal_both_ways(&mut self, id: i32) {
        match self.try_link(id) {
            Ok(message) => self.notify(message),
            Err(e) => self.error = Some(format!("Can't link the portal back: {e}")),
        }
    }

    fn try_link(&mut self, id: i32) -> Result<String, String> {
        let doc = self.doc.as_ref().ok_or("no map is open")?;
        let portal = (doc.scene().portals.iter())
            .find(|p| p.id == id)
            .ok_or("the portal is gone")?;
        let to = portal.to.ok_or("the portal leads nowhere yet")?;
        let here_id = (self.map_list.by_key(&doc.map.key()))
            .map(|m| m.id())
            .ok_or("this map isn't in the map list (Project Settings > Maps)")?;
        let there = self
            .map_list
            .by_id(to.map)
            .ok_or("its destination map isn't listed")?;
        let there_ref = MapRef::parse(&there.key);
        if there_ref == doc.map {
            return Err("it leads to this same map".into());
        }
        let (scene, _) = self.read_map(&there_ref)?;
        let mut other = SceneDoc::new(scene);
        // An arrival off that map (a fresh destination) goes to its middle.
        let grid = &other.scene().grid;
        let mut to = to;
        if grid.index(to.cell).is_none() {
            let middle = (grid.width as i32 / 2, grid.height as i32 / 2);
            to.cell = c3_scene::path::nearest_walkable(grid, middle, 32).unwrap_or(middle);
        }
        // The portal there: one near the arrival, else a new one on it.
        let near = (other.scene().portals.iter())
            .filter(|p| chebyshev(p.cell, to.cell) <= REUSE_RADIUS)
            .min_by_key(|p| chebyshev(p.cell, to.cell))
            .map(|p| p.id);
        let back_id = match near {
            Some(id) => id,
            None => {
                let next = other
                    .scene()
                    .portals
                    .iter()
                    .map(|p| p.id)
                    .max()
                    .unwrap_or(0)
                    + 1;
                (other.execute(&SceneCommand::AddPortal {
                    cell: to.cell,
                    id: Some(next),
                }))
                .map_err(|e| e.to_string())?;
                next
            }
        };
        let back_cell = (other.scene().portals.iter())
            .find(|p| p.id == back_id)
            .map(|p| p.cell)
            .expect("just found or made");
        let return_to =
            arrival(doc.scene(), portal.cell).ok_or("no walkable cell by this portal")?;
        let arrive_there =
            arrival(other.scene(), back_cell).ok_or("no walkable cell by the portal there")?;
        let back = PortalTarget {
            map: here_id,
            cell: return_to,
        };
        (other.execute(&SceneCommand::SetPortalTarget {
            id: back_id,
            to: Some(back),
        }))
        .map_err(|e| e.to_string())?;
        self.write_map(&there_ref, other.scene(), None)?;
        self.world.refresh();
        let doc = self.doc.as_mut().expect("checked above");
        let there_now = PortalTarget {
            map: to.map,
            cell: arrive_there,
        };
        (doc.execute(&SceneCommand::SetPortalTarget {
            id,
            to: Some(there_now),
        }))
        .map_err(|e| e.to_string())?;
        self.editor.select(Some(Selection::Portal(id)));
        Ok(format!(
            "Linked both ways: portal {back_id} on {} leads back here (saved)",
            there.name
        ))
    }
}

/// A walkable cell beside `portal` that isn't a portal, nearest first.
fn arrival(scene: &Scene, portal: CellPos) -> Option<CellPos> {
    (1..=ARRIVAL_RADIUS).find_map(|r| {
        (-r..=r)
            .flat_map(|dy| (-r..=r).map(move |dx| (dx, dy)))
            .filter(|&(dx, dy)| dx.abs() == r || dy.abs() == r)
            .map(|(dx, dy)| (portal.0 + dx, portal.1 + dy))
            .find(|&c| {
                scene.grid.get(c).is_some_and(|cell| !cell.is_blocked())
                    && scene.portal_at(c).is_none()
            })
    })
}

fn chebyshev(a: CellPos, b: CellPos) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_scene::{Grid, IsoProjection, Portal};

    #[test]
    fn travellers_arrive_beside_portals_not_on_them() {
        let grid = Grid::new(10, 10);
        let scene = Scene {
            projection: IsoProjection::for_grid(64.0, 32.0, grid.width),
            grid,
            ground: None,
            portals: vec![
                Portal {
                    cell: (5, 5),
                    id: 1,
                    to: None,
                },
                Portal {
                    cell: (4, 4),
                    id: 2,
                    to: None,
                },
            ],
            entities: Vec::new(),
            layers: Vec::new(),
            npcs: Vec::new(),
            spawns: Vec::new(),
            regions: Vec::new(),
            traps: Vec::new(),
            meta: Default::default(),
        };
        let at = arrival(&scene, (5, 5)).unwrap();
        assert_eq!(chebyshev(at, (5, 5)), 1);
        assert_ne!(at, (4, 4), "not on another portal");
    }
}
