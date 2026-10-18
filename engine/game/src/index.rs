//! Who is near a cell: entities filed by 16×16-cell block, so "players in view" and "items
//! here" look at a few blocks instead of the whole world.

use crate::EntityId;
use c3_scene::CellPos;
use std::collections::HashMap;

const BLOCK: i32 = 16;

fn block(cell: CellPos) -> (i32, i32) {
    (cell.0.div_euclid(BLOCK), cell.1.div_euclid(BLOCK))
}

#[derive(Debug, Default)]
pub struct CellIndex {
    blocks: HashMap<(i32, i32), Vec<(EntityId, CellPos)>>,
}

impl CellIndex {
    pub fn insert(&mut self, id: EntityId, cell: CellPos) {
        self.blocks.entry(block(cell)).or_default().push((id, cell));
    }

    pub fn remove(&mut self, id: EntityId, cell: CellPos) {
        if let Some(list) = self.blocks.get_mut(&block(cell)) {
            list.retain(|(e, _)| *e != id);
        }
    }

    pub fn moved(&mut self, id: EntityId, from: CellPos, to: CellPos) {
        if block(from) == block(to) {
            if let Some(entry) = self
                .blocks
                .get_mut(&block(from))
                .and_then(|list| list.iter_mut().find(|(e, _)| *e == id))
            {
                entry.1 = to;
            }
        } else {
            self.remove(id, from);
            self.insert(id, to);
        }
    }

    /// Entities within `radius` steps of `cell` (diagonals count one), with their cells.
    pub fn near(
        &self,
        cell: CellPos,
        radius: i32,
    ) -> impl Iterator<Item = (EntityId, CellPos)> + '_ {
        let (lo, hi) = (
            block((cell.0 - radius, cell.1 - radius)),
            block((cell.0 + radius, cell.1 + radius)),
        );
        (lo.0..=hi.0)
            .flat_map(move |bx| (lo.1..=hi.1).map(move |by| (bx, by)))
            .filter_map(|b| self.blocks.get(&b))
            .flatten()
            .copied()
            .filter(move |(_, c)| crate::distance(*c, cell) <= radius)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_what_is_near_after_moves() {
        let mut index = CellIndex::default();
        let (a, b) = (EntityId(1), EntityId(2));
        index.insert(a, (5, 5));
        index.insert(b, (40, 40));
        let near = |i: &CellIndex, c, r| i.near(c, r).map(|(e, _)| e).collect::<Vec<_>>();
        assert_eq!(near(&index, (6, 6), 2), [a]);
        index.moved(b, (40, 40), (7, 7));
        index.moved(a, (5, 5), (5, 6));
        let mut both = near(&index, (6, 6), 2);
        both.sort();
        assert_eq!(both, [a, b]);
        index.remove(a, (5, 6));
        assert_eq!(near(&index, (6, 6), 2), [b]);
    }
}
