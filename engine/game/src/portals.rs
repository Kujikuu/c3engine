//! Portals: the map's exits that lead somewhere. A player walks into one (clicking it, or a
//! client asking to enter it) and goes through once next to it; a destination on this map
//! places the player at once, one on another map is an [`Event::Travel`] for the host (the
//! server, or Play) to carry out.

use crate::entity::Task;
use crate::event::Event;
use crate::sim::Sim;
use crate::{EntityId, distance};
use c3_scene::{CellPos, Portal, PortalTarget};

/// How near (cells) a player must be to a portal to go through it.
pub const PORTAL_REACH: i32 = 1;

impl Sim {
    /// The map's portals; those that lead nowhere are left out.
    pub fn set_portals(&mut self, portals: &[Portal]) {
        self.portals = portals
            .iter()
            .filter_map(|p| p.to.map(|to| (p.cell, to)))
            .collect();
    }

    /// Names this map by `ids` (portals and teleports to them stay on the map).
    pub fn set_map_ids(&mut self, ids: Vec<u32>) {
        self.map_ids = ids;
    }

    /// Where the portal on `cell` leads.
    pub fn portal_at(&self, cell: CellPos) -> Option<PortalTarget> {
        self.portals
            .iter()
            .find(|(c, _)| *c == cell)
            .map(|(_, to)| *to)
    }

    pub(crate) fn is_this_map(&self, map: u32) -> bool {
        map == 0 || self.map_ids.contains(&map)
    }

    /// Walks player `id` to the portal on `cell` and through it.
    pub fn enter_portal(&mut self, id: EntityId, cell: CellPos) {
        let Some(to) = self.portal_at(cell) else {
            return;
        };
        self.stop_talking(id);
        let Some(p) = self.living(id) else { return };
        if distance(p.cell, cell) <= PORTAL_REACH {
            p.task = Task::Idle;
            self.travel(id, to);
        } else {
            p.task = Task::Portal(cell);
            self.plan_path(id, cell);
        }
    }

    pub(crate) fn walk_to_portal(&mut self, id: EntityId, cell: CellPos) {
        let here = self.players[&id].cell;
        if distance(here, cell) <= PORTAL_REACH {
            self.players.get_mut(&id).expect("ticking").task = Task::Idle;
            if let Some(to) = self.portal_at(cell) {
                self.travel(id, to);
            }
        } else if !self.step_player(id) {
            // The path ran out short of the portal.
            self.players.get_mut(&id).expect("ticking").task = Task::Idle;
        }
    }

    /// Sends player `id` to `to`: placed at once on this map, else the host moves it.
    pub(crate) fn travel(&mut self, id: EntityId, to: PortalTarget) {
        if self.is_this_map(to.map) {
            self.place(id, to.cell);
            return;
        }
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        p.task = Task::Idle;
        p.path.clear();
        self.events.push(Event::Travel {
            player: id,
            map: to.map,
            cell: to.cell,
        });
    }
}
