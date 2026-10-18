//! Portals (map exits), addressed by id.

use super::{Change, Edit, SceneDoc};
use crate::EditError;
use c3_scene::{CellPos, Portal, PortalTarget};

impl SceneDoc {
    /// `cell` is on the map and has no portal yet.
    fn check_portal_free(&self, cell: CellPos) -> Result<(), EditError> {
        self.check_cell(cell)?;
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

    pub(super) fn add_portal(&mut self, cell: CellPos, id: Option<i32>) -> Edit {
        self.check_portal_free(cell)?;
        let taken = |i: i32| self.scene.portals.iter().any(|p| p.id == i);
        let id = match id {
            Some(i) if taken(i) => return Err(EditError::PortalIdTaken(i)),
            Some(i) => i,
            None => (0..).find(|i| !taken(*i)).unwrap_or(0),
        };
        let change = self.edit_portals(|ps| ps.push(Portal { cell, id, to: None }));
        Ok((
            format!("Added portal {id} at {}, {}", cell.0, cell.1),
            Some(change),
        ))
    }

    pub(super) fn move_portal(&mut self, id: i32, to: CellPos) -> Edit {
        let i = self.portal_index(id)?;
        if self.scene.portals[i].cell == to {
            return Ok((
                format!("Portal {id} is already at {}, {}", to.0, to.1),
                None,
            ));
        }
        self.check_portal_free(to)?;
        let change = self.edit_portals(|ps| ps[i].cell = to);
        Ok((
            format!("Moved portal {id} to {}, {}", to.0, to.1),
            Some(change),
        ))
    }

    pub(super) fn set_portal_target(&mut self, id: i32, to: Option<PortalTarget>) -> Edit {
        let i = self.portal_index(id)?;
        if self.scene.portals[i].to == to {
            return Ok((format!("Portal {id} already leads there"), None));
        }
        let change = self.edit_portals(|ps| ps[i].to = to);
        let summary = match to {
            Some(t) => format!(
                "Portal {id} leads to map {} at {}, {}",
                t.map, t.cell.0, t.cell.1
            ),
            None => format!("Portal {id} leads nowhere"),
        };
        Ok((summary, Some(change)))
    }

    pub(super) fn remove_portal(&mut self, id: i32) -> Edit {
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
}
