//! c3-game: the gameplay simulation shared by the Studio's Play mode, the client and (M6)
//! the server. A [`Sim`] runs one map in fixed ticks: players walk, fight monsters born in
//! spawn areas, earn experience and levels, pick up drops and trade with shops. What things
//! are and how fights are settled comes from the game through [`Rules`]; the engine keeps
//! the world, movement, timing and order (see docs/DECISIONS.md: a typed world, no ECS).

mod commands;
mod dialogue;
mod entity;
mod event;
mod index;
mod monsters;
mod players;
mod rng;
mod rules;
mod sim;

pub use commands::HELP as COMMAND_HELP;
pub use entity::{BAG_SIZE, GroundItem, Item, Monster, Player, Task};
pub use event::Event;
pub use rng::Rng;
pub use rules::{Drop, Fighter, ItemType, MonsterType, Rules, Stats};
pub use sim::{Sim, TICK_MS};

use c3_scene::CellPos;

/// A player, monster or ground item; unique in a [`Sim`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(pub u32);

/// How long a dead monster lies before it vanishes (ms).
pub const CORPSE_MS: u64 = 3000;
/// How long a dead player waits to come back (ms).
pub const REVIVE_MS: u64 = 10_000;
/// How long a drop stays on the ground (ms).
pub const ITEM_LIFE_MS: u64 = 60_000;
/// Cells from an NPC within which a player can trade with it.
pub const NPC_REACH: i32 = 18;
/// Cells from an NPC within which a player can start talking to it.
pub const TALK_RANGE: i32 = 3;

/// Steps between two cells (diagonals count one).
pub fn distance(a: CellPos, b: CellPos) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

#[cfg(test)]
mod tests;
