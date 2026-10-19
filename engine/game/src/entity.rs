//! The things in a simulated world: players, monsters and items on the ground.

use crate::EntityId;
use crate::rules::{Fighter, Stats};
use c3_scene::CellPos;
use c3_script::Conversation;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// One item someone carries (a stack of them); `id` is unique in the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: u32,
    pub kind: u32,
    /// How many are stacked here (up to the type's `stack`).
    #[serde(default = "one")]
    pub count: u32,
    /// Durability left, for types that have one; at 0 the item is broken and gives nothing.
    #[serde(default)]
    pub durability: u32,
    /// Its quality step (0: the lowest), each adding to what it gives.
    #[serde(default)]
    pub quality: u8,
}

fn one() -> u32 {
    1
}

impl Item {
    /// One new item `id` of type `kind` (`t`), whole.
    pub fn new(id: u32, kind: u32, t: Option<&crate::ItemType>) -> Self {
        Self {
            id,
            kind,
            count: 1,
            durability: t.map_or(0, |t| t.durability),
            quality: 0,
        }
    }

    /// Whether it is worn out (a type with durability, none left).
    pub fn is_broken(&self, t: &crate::ItemType) -> bool {
        t.durability > 0 && self.durability == 0
    }
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
    /// Closing in to cast a skill.
    Cast {
        skill: u32,
        at: crate::skills::CastAt,
    },
}

/// A player's own look: the body and hair chosen when the character was made.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerLook {
    pub body: u32,
    pub hair: u32,
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
    /// Other currencies, by name (see the settings' `currencies`).
    #[serde(default)]
    pub wallet: BTreeMap<String, u64>,
    /// Skills it knows, in the order of its skill bar.
    #[serde(default)]
    pub skills: Vec<u32>,
    /// When (game ms) each skill rested will be ready again.
    #[serde(default)]
    pub cooldowns: BTreeMap<u32, u64>,
    /// Status effects it bears.
    #[serde(default)]
    pub statuses: Vec<crate::statuses::ActiveStatus>,
    /// Players it killed on PK maps.
    #[serde(default)]
    pub pk_kills: u32,
    /// The events script's timers: name, and when (game ms) it runs out.
    #[serde(skip)]
    pub(crate) timers: Vec<(String, u64)>,
    /// The regions it stands in (ids), to tell scripts when that changes.
    #[serde(skip)]
    pub(crate) regions: Vec<u32>,
    pub stats: Stats,
    pub bag: Vec<Item>,
    /// What it wears, one item per slot.
    pub equipment: Vec<Item>,
    /// The team it is in: its leader's id (its own when it leads).
    #[serde(default)]
    pub team: Option<EntityId>,
    /// Its team on every map, itself among them (the leader first); sent to it alone.
    #[serde(default)]
    pub mates: Vec<crate::Mate>,
    /// What a script's walk is called, to tell it (`on_arrive`) when the player gets there.
    #[serde(skip)]
    pub arrive_tag: Option<String>,
    /// The look chosen when it was made: body and hair, as the game numbers them (0: the
    /// game's default).
    #[serde(default)]
    pub look: PlayerLook,
    /// The profession chosen when it was made (as the game numbers them; 0: none).
    #[serde(default)]
    pub profession: u32,
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
    /// Status effects it bears.
    #[serde(default)]
    pub statuses: Vec<crate::statuses::ActiveStatus>,
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
