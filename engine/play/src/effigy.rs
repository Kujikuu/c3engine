//! A statue of the hero being posed (TQ's CDlgEffigy and CHero's dummy): the hero's look in
//! the statue window, in a pose picked, played or paused and turned; placed on the nearest
//! free statue slot its guild rank may take (`gameplay.statues`, CHero::CreateStatuty).

use crate::preview::{LookPreview, UiModel};
use c3_core::settings::StatueSettings;
use c3_scene::CellPos;

pub use c3_ui::EffigyAction;

/// The figure being posed (`effigy.model`) and whether its pose plays.
pub struct Effigy {
    pub model: UiModel,
    pub playing: bool,
}

impl Effigy {
    pub fn new(preview: LookPreview) -> Self {
        Self {
            model: UiModel::new("effigy.model", preview),
            playing: true,
        }
    }
}

/// Why a statue can't be put up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatueRefused {
    /// Not on a map with statue slots, or too far from the slot (StrRes 100128).
    NotHere,
    /// The hero's guild rank takes no slots (StrRes 100129).
    Rank,
    /// Every slot the rank may take has a statue.
    Full,
}

/// The slot a statue of a player of guild rank `rank`, standing on `at` of map `map`, goes
/// on: the nearest free one of the rank's (`taken` says which cells have a statue).
pub fn statue_slot(
    statues: &StatueSettings,
    map: u32,
    rank: Option<u32>,
    at: CellPos,
    taken: impl Fn(CellPos) -> bool,
) -> Result<CellPos, StatueRefused> {
    let map = (statues.maps.iter())
        .find(|m| m.map == map)
        .ok_or(StatueRefused::NotHere)?;
    let rank = (statues.ranks.iter())
        .find(|r| Some(r.rank) == rank)
        .ok_or(StatueRefused::Rank)?;
    let distance = |c: CellPos| {
        let (dx, dy) = (i64::from(c.0 - at.0), i64::from(c.1 - at.1));
        dx * dx + dy * dy
    };
    let slot = (rank.slots.iter())
        .filter_map(|&i| map.slots.get(i))
        .map(|&[x, y]| (x, y))
        .filter(|&c| !taken(c))
        .min_by_key(|&c| distance(c))
        .ok_or(StatueRefused::Full)?;
    // TQ refuses only when the slot is past the reach both ways, below and right of the
    // player (CreateStatuty's check).
    let reach = statues.reach;
    if slot.0 - at.0 > reach && slot.1 - at.1 > reach {
        return Err(StatueRefused::NotHere);
    }
    Ok(slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_core::settings::{StatueMap, StatueRank};

    #[test]
    fn statues_take_the_nearest_free_slot_of_the_rank() {
        let statues = StatueSettings {
            kind: 9,
            maps: vec![StatueMap {
                map: 1037,
                slots: vec![[187, 191], [184, 198], [194, 188]],
            }],
            ranks: vec![
                StatueRank {
                    rank: 100,
                    slots: vec![0],
                },
                StatueRank {
                    rank: 90,
                    slots: vec![1, 2],
                },
            ],
            reach: 4,
        };
        let free = |_| false;
        assert_eq!(
            statue_slot(&statues, 1037, Some(90), (193, 189), free),
            Ok((194, 188))
        );
        let first_taken = |c: CellPos| c == (194, 188);
        assert_eq!(
            statue_slot(&statues, 1037, Some(90), (193, 189), first_taken),
            Ok((184, 198))
        );
        assert_eq!(
            statue_slot(&statues, 1037, Some(50), (193, 189), free),
            Err(StatueRefused::Rank)
        );
        assert_eq!(
            statue_slot(&statues, 1002, Some(100), (193, 189), free),
            Err(StatueRefused::NotHere)
        );
        assert_eq!(
            statue_slot(&statues, 1037, Some(100), (150, 150), free),
            Err(StatueRefused::NotHere)
        );
    }
}
