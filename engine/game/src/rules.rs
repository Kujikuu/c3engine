//! What the game decides: monster and item types, player stats, combat formulas, experience
//! and drops. The simulation asks; a game (`games/conquer`) answers from its tables.

use crate::rng::Rng;
use serde::{Deserialize, Serialize};

/// Fighting numbers of a player or monster.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    pub max_life: u32,
    pub max_mana: u32,
    pub attack_min: u32,
    pub attack_max: u32,
    pub defence: u32,
    /// Chance to avoid hits (game-defined scale).
    pub dodge: u32,
    /// Accuracy (game-defined scale).
    pub dexterity: u32,
    /// Cells from the target within which it can hit.
    pub attack_range: u32,
    /// Milliseconds between hits.
    pub attack_ms: u32,
    /// Milliseconds per one-cell step.
    pub step_ms: u32,
}

impl Stats {
    /// These stats with an item's bonus added (life, mana, attack, defence, dodge, accuracy).
    pub fn with_bonus(mut self, bonus: &Stats) -> Stats {
        self.max_life += bonus.max_life;
        self.max_mana += bonus.max_mana;
        self.attack_min += bonus.attack_min;
        self.attack_max += bonus.attack_max;
        self.defence += bonus.defence;
        self.dodge += bonus.dodge;
        self.dexterity += bonus.dexterity;
        self
    }
}

/// One side of a hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fighter {
    pub level: u32,
    pub stats: Stats,
}

/// A kind of monster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonsterType {
    pub id: u32,
    pub name: String,
    /// The game's model (look) id.
    pub look: u32,
    pub level: u32,
    pub stats: Stats,
    /// Cells within which it notices players.
    pub view_range: u32,
    /// Attacks players on sight (otherwise only when attacked).
    pub aggressive: bool,
}

/// A kind of item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemType {
    pub id: u32,
    pub name: String,
    /// Price in a shop.
    pub price: u64,
    /// Life restored when used (potions); 0 if it can't be used that way.
    pub restores_life: u32,
    pub restores_mana: u32,
    /// The equipment slot it is worn in (game-defined numbers); `None` if not worn.
    pub slot: Option<u8>,
    /// What wearing it adds (see [`Stats::with_bonus`]).
    pub bonus: Stats,
}

/// Something a monster leaves on the ground when it dies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drop {
    Money(u64),
    Item(u32),
}

/// A game's rules. They are `Any`, so the game's own code (a protocol front, say) can reach
/// its rules type through a simulation's `rules()`.
pub trait Rules: std::any::Any {
    fn monster(&self, kind: u32) -> Option<&MonsterType>;

    fn item(&self, kind: u32) -> Option<&ItemType>;

    /// A player's stats at `level`, before equipment (whose bonuses the simulation adds).
    fn player_stats(&self, level: u32) -> Stats;

    /// Experience needed to rise from `level` to the next; `None` at the top level.
    fn level_exp(&self, level: u32) -> Option<u64>;

    /// Damage of one hit, 0 for a miss.
    fn hit(&self, attacker: &Fighter, target: &Fighter, rng: &mut Rng) -> u32;

    /// Experience a player at `level` earns for dealing `damage` to a monster.
    fn hit_exp(&self, level: u32, target: &Fighter, damage: u32) -> u64;

    /// Extra experience for the killing blow.
    fn kill_exp(&self, level: u32, target: &Fighter) -> u64;

    /// What a monster of `kind` drops.
    fn drops(&self, kind: u32, rng: &mut Rng) -> Vec<Drop>;

    /// Item types NPC `npc` sells (empty if it has no shop).
    fn shop(&self, npc: u32) -> &[u32];

    /// Money a shop pays for an item (TQ: a third of its price).
    fn sell_price(&self, kind: u32) -> u64 {
        self.item(kind).map_or(0, |i| i.price / 3)
    }
}
