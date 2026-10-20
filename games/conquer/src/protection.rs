//! TQ's item protection (tq_re spec systems/business.md §3, the server's `FUN_00469489`): of
//! each kind of equipment, the elite or super piece that fits the player's class and scores
//! highest among all it wears and carries is protected; a Trojan keeps two one-handers; a piece
//! of a related kind scoring more than 4 higher takes the protection away.

use c3_game::{Item, Player, Rules, Sim};

/// The rules Conquer plays by: the engine's data rules, with its item protection.
pub struct ConquerRules {
    inner: Box<dyn Rules>,
}

impl ConquerRules {
    pub fn new(inner: Box<dyn Rules>) -> Self {
        Self { inner }
    }
}

/// The combine work kinds (the protocol's `combine`): composing refuses protected minors and
/// gems; fusing refuses any protected item.
const COMPOSE: u32 = 3;
const FUSE: u32 = 6;

/// Equipment slot 7, where a piece scores nothing.
const NO_SCORE_SLOT: u8 = 7;

/// An item type's sort (equipment: 1-8) and subtype (`410` for a blade).
fn sort(kind: u32) -> u32 {
    (kind / 100_000) % 10
}

fn subtype(kind: u32) -> u32 {
    (kind / 1000) % 1000
}

/// What the protection scores a piece (`FUN_00428608`): its quality above Normal (Refined 1
/// to Super 4), one for each socket and one more for each super gem, and its plus.
fn score(sim: &Sim, item: &Item) -> i64 {
    let attr = |name: &str| {
        let names = &sim.settings().item_attributes;
        (names.iter().position(|n| n == name))
            .and_then(|at| item.attrs.get(at))
            .map_or(0, |v| i64::from(*v))
    };
    if !(1..=8).contains(&sort(item.kind)) || sim.slot_of(item) == Some(NO_SCORE_SLOT) {
        return 0;
    }
    let quality = (i64::from(item.kind % 10) - 5).max(0);
    let gems = [attr("gem1"), attr("gem2")];
    let sockets = gems.iter().filter(|g| **g != 0).count() as i64;
    let supers = gems.iter().filter(|g| **g != 0 && **g % 10 == 3).count() as i64;
    quality + sockets + supers + attr("plus")
}

/// The kind a piece counts as (one-handers 4, two-handers 5, 150/151 7, else its subtype).
fn group(subtype: u32) -> u32 {
    match subtype {
        150 | 151 => 7,
        410 | 420 | 430 | 440 | 450 | 460 | 480 | 481 | 490 => 4,
        510 | 530 | 540 | 560 | 561 | 580 => 5,
        s => s,
    }
}

/// Whether a piece of `subtype` fits class `c` (and may be protected at all).
fn fits(subtype: u32, c: u32) -> bool {
    match subtype {
        117 | 160 => true,
        118 | 130 => c == 1,
        111 | 131 | 141 | 900 => c == 2,
        113 | 133 | 142 | 500 => c == 4,
        114 | 121 | 134 | 152 | 421 => matches!(c, 10 | 13 | 14),
        120 | 150 | 151 => !matches!(c, 10 | 14),
        410 | 420 | 430 | 440 | 450 | 460 | 480 | 481 | 490 => matches!(c, 1 | 2),
        510 | 530 | 540 | 560 | 561 | 580 => c != 4,
        _ => false,
    }
}

/// Whether kinds `a` and `b` are related for class `c` (one may outscore the other).
fn related(c: u32, a: u32, b: u32) -> bool {
    let pair = |x: u32, y: u32| (a == x && b == y) || (a == y && b == x);
    let among = |set: &[u32]| a != b && set.contains(&a) && set.contains(&b);
    match c {
        1 => pair(117, 118) || pair(4, 5),
        2 => among(&[117, 111, 141]) || pair(4, 5),
        4 => among(&[117, 113, 142]),
        10 | 14 => pair(117, 114) || pair(421, 5),
        13 => pair(117, 114) || pair(421, 5) || pair(120, 121) || pair(152, 7),
        _ => false,
    }
}

/// Whether `owner`'s `item` is protected (`FUN_00469489`; the merchant test is the engine's).
pub fn protected(sim: &Sim, owner: &Player, item: &Item) -> bool {
    let mine = score(sim, item);
    let c = (owner.profession % 1000) / 10;
    let sub = subtype(item.kind);
    if mine == 0 || item.kind % 10 < 8 || !fits(sub, c) {
        return false;
    }
    let own = group(sub);
    let mut spare = u8::from(c == 1 && own == 4);
    let worn = owner.equipment.iter().map(|i| (i, true));
    let carried = owner.bag.iter().map(|i| (i, false));
    worn.chain(carried).all(|(other, equipped)| {
        if other.id == item.id || !(1..=8).contains(&sort(other.kind)) {
            return true;
        }
        let theirs = score(sim, other);
        let kind = group(subtype(other.kind));
        if kind == own {
            if mine < theirs || (mine == theirs && equipped) {
                // A Trojan keeps two one-handers.
                if spare > 0 {
                    spare -= 1;
                    return true;
                }
                return false;
            }
            return true;
        }
        !(related(c, kind, own) && theirs - mine > 4)
    })
}

impl Rules for ConquerRules {
    fn monster(&self, kind: u32) -> Option<&c3_game::MonsterType> {
        self.inner.monster(kind)
    }

    fn item(&self, kind: u32) -> Option<&c3_game::ItemType> {
        self.inner.item(kind)
    }

    fn player_stats(&self, who: &c3_game::StatsFor) -> c3_game::Stats {
        self.inner.player_stats(who)
    }

    fn level_exp(&self, level: u32) -> Option<u64> {
        self.inner.level_exp(level)
    }

    fn level_time(&self, level: u32) -> Option<u64> {
        self.inner.level_time(level)
    }

    fn hit(&self, a: &c3_game::Fighter, t: &c3_game::Fighter, rng: &mut c3_game::Rng) -> u32 {
        self.inner.hit(a, t, rng)
    }

    fn hit_exp(&self, a: &c3_game::Fighter, t: &c3_game::Fighter, damage: u32) -> u64 {
        self.inner.hit_exp(a, t, damage)
    }

    fn kill_exp(&self, a: &c3_game::Fighter, t: &c3_game::Fighter) -> u64 {
        self.inner.kill_exp(a, t)
    }

    fn team_exp(
        &self,
        member: &c3_game::Fighter,
        target: &c3_game::Fighter,
        kill: u64,
        newbie: bool,
        spouse: bool,
    ) -> u64 {
        self.inner.team_exp(member, target, kill, newbie, spouse)
    }

    fn drops(&self, kind: u32, rng: &mut c3_game::Rng) -> Vec<c3_game::Drop> {
        self.inner.drops(kind, rng)
    }

    fn protected(&self, sim: &Sim, owner: &Player, item: &Item) -> bool {
        protected(sim, owner, item)
    }

    fn protects_work(&self, kind: u32, at: usize) -> bool {
        match kind {
            COMPOSE => at >= 1,
            FUSE => true,
            _ => false,
        }
    }

    fn loot(&self, kind: u32) -> c3_game::Loot {
        self.inner.loot(kind)
    }

    fn shop(&self, npc: u32) -> &[u32] {
        self.inner.shop(npc)
    }

    fn shop_currency(&self, npc: u32, kind: u32) -> Option<&str> {
        self.inner.shop_currency(npc, kind)
    }

    fn skill(&self, id: u32) -> Option<&c3_game::SkillType> {
        self.inner.skill(id)
    }

    fn skill_at(&self, id: u32, rank: u32) -> Option<&c3_game::SkillType> {
        self.inner.skill_at(id, rank)
    }

    fn status(&self, id: u32) -> Option<&c3_game::StatusType> {
        self.inner.status(id)
    }

    fn skill_hit(
        &self,
        a: &c3_game::Fighter,
        t: &c3_game::Fighter,
        skill: &c3_game::SkillType,
        rng: &mut c3_game::Rng,
    ) -> u32 {
        self.inner.skill_hit(a, t, skill, rng)
    }

    fn skill_heal(&self, healer: &c3_game::Fighter, skill: &c3_game::SkillType) -> u32 {
        self.inner.skill_heal(healer, skill)
    }

    fn shown(&self, player: &c3_game::Fighter) -> Vec<(String, i64)> {
        self.inner.shown(player)
    }

    fn may_marry(&self, a: &c3_game::Fighter, b: &c3_game::Fighter) -> bool {
        self.inner.may_marry(a, b)
    }

    fn can_wear(&self, player: &c3_game::Fighter, item: &c3_game::ItemType) -> bool {
        self.inner.can_wear(player, item)
    }

    fn sell_price(&self, kind: u32) -> u64 {
        self.inner.sell_price(kind)
    }

    fn wrapped(&self) -> Option<&dyn Rules> {
        Some(&*self.inner)
    }

    fn take_problems(&self) -> Vec<String> {
        self.inner.take_problems()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_fit_classes_and_relate() {
        // A Trojan's one-handed blade fits; a Taoist's doesn't; boots fit anyone.
        assert!(fits(410, 1) && !fits(410, 10) && fits(160, 10));
        assert_eq!((group(410), group(561), group(151)), (4, 5, 7));
        assert!(related(1, 4, 5) && related(2, 111, 141) && !related(4, 117, 118));
    }
}
