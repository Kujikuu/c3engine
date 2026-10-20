//! Battle power as the 5065 client counts it (CHero::CalculateBattlePower): the level, five
//! for each rebirth, and from what is worn the quality (Refined 1 to Super 4), sockets, super
//! gems and plus, a two-handed weapon counting twice; the noble rank adds its own.

use c3_game::{Item, Player, Sim};
use c3_ui::{Power, PowerRow};

/// The equipment slots the detail page lists, in its order: head, neck, ring, armor, right
/// and left hand, shoes (TQ's ITEMPOSITION_*).
const ROWS: [u8; 7] = [1, 2, 6, 3, 4, 5, 8];
const RIGHT_HAND: u8 = 4;
const LEFT_HAND: u8 = 5;

/// An item number by the settings' item attribute names (`gem1`, `plus`).
fn attr(sim: &Sim, item: &Item, name: &str) -> i64 {
    let names = &sim.settings().item_attributes;
    (names.iter().position(|n| n == name))
        .and_then(|at| item.attrs.get(at))
        .map_or(0, |v| i64::from(*v))
}

/// Whether item type `kind` takes both hands (Hero.cpp IsBattlePowerTwoHandedItem: two-handed
/// weapons, and the bow among the 500000s).
fn two_handed(kind: u32) -> bool {
    let sort = (kind % 10_000_000) / 100_000;
    sort == 5 || (sort == 4 && (kind % 100_000) / 1000 * 1000 == 21_000)
}

/// Whether item type `kind` is an arrow (it counts for nothing).
fn arrow(kind: u32) -> bool {
    (kind % 10_000_000) / 100_000 == 10 && (kind % 100_000) / 10_000 * 10_000 == 50_000
}

/// The noble rank's own battle power (the rank's number, CDlgNobility's table).
fn noble_power(sim: &Sim, p: &Player) -> i64 {
    let n = &sim.settings().nobility;
    if n.min_level == 0 {
        return 0;
    }
    i64::from(sim.world_state().noble_rank(n, &p.name).0)
}

/// Player `p`'s battle power and what makes it.
pub fn battle_power(sim: &Sim, p: &Player) -> Power {
    let worn = |slot: u8| p.equipment.iter().find(|i| sim.slot_of(i) == Some(slot));
    let right_two = worn(RIGHT_HAND).is_some_and(|i| two_handed(i.kind));
    let mut rows = Vec::new();
    let mut sums = [0i64; 4];
    for slot in ROWS {
        let item = worn(slot).filter(|i| !arrow(i.kind)).cloned();
        // A left hand under a two-handed weapon counts for nothing, shown greyed.
        let shadowed = slot == LEFT_HAND && right_two;
        let times = if slot == RIGHT_HAND && right_two {
            2
        } else {
            1
        };
        let scores = match &item {
            Some(i) if !shadowed && i.durability > 0 => {
                let quality = i64::from(i.kind % 10);
                let gems = [attr(sim, i, "gem1"), attr(sim, i, "gem2")];
                let set = gems.iter().filter(|g| **g != 0).count() as i64;
                let supers = gems.iter().filter(|g| **g != 0 && **g % 10 == 3).count() as i64;
                let quality = if (6..=9).contains(&quality) {
                    quality - 5
                } else {
                    0
                };
                [quality, set, supers, attr(sim, i, "plus")].map(|s| s * times)
            }
            _ => [0; 4],
        };
        for (sum, score) in sums.iter_mut().zip(scores) {
            *sum += score;
        }
        rows.push(PowerRow {
            quality: item.as_ref().map_or(0, |i| (i.kind % 10) as u8),
            item,
            scores,
            shadowed,
        });
    }
    let rebirths = p.attributes.get("rebirths").copied().unwrap_or(0);
    let parts = [
        i64::from(p.level),
        rebirths * 5,
        sums[0],
        sums[1],
        sums[2],
        sums[3],
        noble_power(sim, p),
    ];
    Power { parts, rows }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_handers_and_arrows_by_type() {
        assert!(two_handed(510_000), "a glaive (500000s)");
        assert!(two_handed(421_000 + 9), "a bow among the 400000s");
        assert!(!two_handed(410_000), "a blade");
        assert!(arrow(1_050_000), "an arrow (1050000)");
        assert!(!arrow(1_000_000));
    }
}
