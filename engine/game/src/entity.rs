//! The things in a simulated world: players, monsters and items on the ground.

use crate::EntityId;
use crate::rules::{Fighter, Stats};
use c3_scene::CellPos;
use c3_script::Conversation;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// One item someone carries; `id` is unique in the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: u32,
    pub kind: u32,
}

/// What a player is doing.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Task {
    #[default]
    Idle,
    /// Walking a planned path.
    Walk,
    /// Closing in on and hitting a monster.
    Attack(EntityId),
    /// Walking to an item on the ground to pick it up.
    PickUp(EntityId),
    /// Walking up to NPC `id` to talk to it.
    Talk(u32),
    /// Walking to the portal on a cell to go through it.
    Portal(CellPos),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Player {
    pub id: EntityId,
    pub name: String,
    pub cell: CellPos,
    pub direction: u8,
    pub level: u32,
    pub exp: u64,
    pub life: u32,
    pub mana: u32,
    pub money: u64,
    pub stats: Stats,
    pub bag: Vec<Item>,
    /// What it wears, one item per slot.
    pub equipment: Vec<Item>,
    pub running: bool,
    pub task: Task,
    /// The dialogue the player is in, if any.
    pub dialogue: Option<Conversation>,
    /// The NPC whose shop the player has open.
    pub shop: Option<u32>,
    /// Script variables kept with the player (quest steps, counters).
    pub flags: BTreeMap<String, i64>,
    pub(crate) path: VecDeque<CellPos>,
    /// Game time (ms) the next step or hit may happen.
    pub(crate) next_step_ms: u64,
    pub(crate) next_attack_ms: u64,
    /// When a dead player comes back to life.
    pub(crate) revive_at_ms: Option<u64>,
}

impl Player {
    pub fn is_alive(&self) -> bool {
        self.revive_at_ms.is_none()
    }

    pub fn fighter(&self) -> Fighter {
        Fighter {
            level: self.level,
            stats: self.stats,
        }
    }

    pub(crate) fn step_ms(&self) -> u64 {
        let ms = u64::from(self.stats.step_ms.max(1));
        if self.running { ms / 2 } else { ms }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Monster {
    pub id: EntityId,
    /// Its monster type.
    pub kind: u32,
    pub level: u32,
    pub cell: CellPos,
    pub direction: u8,
    pub life: u32,
    pub stats: Stats,
    /// Where it was born; it returns there when it gives up a chase.
    pub home: CellPos,
    /// The spawn area it belongs to.
    pub(crate) spawn: Option<u32>,
    pub(crate) target: Option<EntityId>,
    pub(crate) next_step_ms: u64,
    pub(crate) next_attack_ms: u64,
    /// When it died (it lies there a moment before it vanishes).
    pub(crate) died_at_ms: Option<u64>,
}

impl Monster {
    pub fn is_alive(&self) -> bool {
        self.died_at_ms.is_none()
    }

    pub fn fighter(&self) -> Fighter {
        Fighter {
            level: self.level,
            stats: self.stats,
        }
    }
}

/// Money or an item lying on the ground.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroundItem {
    pub id: EntityId,
    pub cell: CellPos,
    pub money: u64,
    /// The item type, when it is not money.
    pub kind: Option<u32>,
    pub(crate) expires_ms: u64,
}
