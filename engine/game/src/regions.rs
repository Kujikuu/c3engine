//! What a map's regions mean for play: no one fights in a safe zone, players don't fight
//! each other in a no-PK zone, and the events script hears `on_enter_region(id)` and
//! `on_leave_region(id)` as players walk in and out.

use crate::EntityId;
use crate::sim::Sim;
use c3_scene::{CellPos, Region};

impl Sim {
    /// The map's regions from now on.
    pub fn set_regions(&mut self, regions: Vec<Region>) {
        self.regions = regions;
    }

    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    /// The regions holding `cell`.
    pub fn regions_at(&self, cell: CellPos) -> impl Iterator<Item = &Region> {
        self.regions.iter().filter(move |r| r.contains(cell))
    }

    /// No one fights on `cell`.
    pub fn is_safe(&self, cell: CellPos) -> bool {
        self.regions_at(cell).any(|r| r.safe)
    }

    /// Players can't attack each other on `cell`.
    pub fn is_no_pk(&self, cell: CellPos) -> bool {
        self.regions_at(cell).any(|r| r.no_pk || r.safe)
    }

    /// Players may set up booths on `cell`.
    pub fn is_market(&self, cell: CellPos) -> bool {
        self.regions_at(cell).any(|r| r.booth)
    }

    /// Whether a fight between `a` and `b` may happen where they stand (`players`: both are
    /// players).
    pub(crate) fn may_fight(&self, a: CellPos, b: CellPos, players: bool) -> bool {
        let free = |c| !self.is_safe(c) && !(players && self.is_no_pk(c));
        free(a) && free(b)
    }

    /// Tells the events script which regions players walked into or out of.
    pub(crate) fn tick_regions(&mut self) {
        if self.regions.is_empty() {
            return;
        }
        let moves: Vec<(EntityId, Vec<u32>, Vec<u32>)> = (self.players.values_mut())
            .filter_map(|p| {
                let now: Vec<u32> = (self.regions.iter())
                    .filter(|r| r.contains(p.cell))
                    .map(|r| r.id)
                    .collect();
                if now == p.regions {
                    return None;
                }
                let before = std::mem::replace(&mut p.regions, now.clone());
                let left = before
                    .iter()
                    .filter(|id| !now.contains(id))
                    .copied()
                    .collect();
                let entered = now
                    .iter()
                    .filter(|id| !before.contains(id))
                    .copied()
                    .collect();
                Some((p.id, left, entered))
            })
            .collect();
        for (player, left, entered) in moves {
            for id in left {
                self.script_event(player, "on_leave_region", vec![i64::from(id).into()]);
            }
            for id in entered {
                self.script_event(player, "on_enter_region", vec![i64::from(id).into()]);
            }
        }
    }
}
