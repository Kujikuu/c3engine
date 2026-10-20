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
    /// The game's numbers for it (named by the settings' `item_attributes`).
    #[serde(default, skip_serializing_if = "no_attrs")]
    pub attrs: [i32; ITEM_ATTRIBUTES],
    /// Worn, the slot it is in when that isn't its type's own (its `alt_slot`); 0 otherwise.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub worn_at: u8,
    /// Locked (1), or unlocking until that day (YYYYMMDD); 0: free (see `locks`).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub lock: u32,
}

fn is_zero_u32(n: &u32) -> bool {
    *n == 0
}

fn is_zero_i64(n: &i64) -> bool {
    *n == 0
}

fn is_zero(n: &u8) -> bool {
    *n == 0
}

pub use c3_core::settings::ITEM_ATTRIBUTES;

fn no_attrs(attrs: &[i32; ITEM_ATTRIBUTES]) -> bool {
    attrs.iter().all(|&a| a == 0)
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
            attrs: [0; ITEM_ATTRIBUTES],
            worn_at: 0,
            lock: 0,
        }
    }

    /// The slot it is worn in (if worn), its type `t` saying the usual one.
    pub fn slot(&self, t: Option<&crate::ItemType>) -> Option<u8> {
        match self.worn_at {
            0 => t?.slot,
            slot => Some(slot),
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

/// A player's own look: the body and hair chosen when the character was made, and its face
/// (0: the game's usual).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerLook {
    pub body: u32,
    pub hair: u32,
    #[serde(default)]
    pub face: u32,
}

/// How a player's name shows: good, a criminal's (blue), or red or black for PK points (a
/// criminal with PK points shows the points' colour).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NameMark {
    #[default]
    Good,
    Criminal,
    Red,
    Black,
}

impl NameMark {
    /// Whether attacking or killing it is fair (no crime, no PK points).
    pub fn is_evil(self) -> bool {
        self != Self::Good
    }
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
    /// Items kept with storage NPCs (by NPC id), and the money kept (any of them).
    #[serde(default)]
    pub storage: BTreeMap<u32, Vec<Item>>,
    /// Offline training: the credit banked, when it started (unix seconds; 0: not training),
    /// and when this stay online began.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub training_credit: u32,
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub training_since: i64,
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub online_since: i64,
    /// What its bag's chests hold, by the chest's item id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub chests: BTreeMap<u32, Vec<Item>>,
    #[serde(default)]
    pub saved_money: u64,
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
    /// The items being worked together while `on_combine` runs.
    pub(crate) combining: Vec<u32>,
    /// The event a placement the player was asked for runs, and where it placed it while
    /// that runs.
    pub(crate) placing: Option<String>,
    pub(crate) placed: Option<(CellPos, u8)>,
    /// The action and frame it was placed in (a statue's pose).
    #[serde(skip)]
    pub(crate) placed_pose: Option<(u32, u32)>,
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
    /// PK points: what killing other players has earned (game-defined).
    #[serde(default)]
    pub pk: i32,
    /// Whom its attacks may hit among other players.
    #[serde(default)]
    pub pk_mode: c3_core::settings::PkMode,
    /// Its name as others see it (crime and PK points), kept up to date by the simulation.
    #[serde(default)]
    pub mark: NameMark,
    /// Until when (game ms) it is a criminal.
    #[serde(skip)]
    pub(crate) crime_until: u64,
    /// When (game ms) it next loses a PK point.
    #[serde(skip)]
    pub(crate) next_decay_ms: u64,
    /// When (game ms) it last left a friend words.
    #[serde(skip)]
    pub(crate) word_left_ms: Option<u64>,
    /// The game's numbers (the settings' `attributes`) that differ from their default.
    #[serde(default)]
    pub attributes: BTreeMap<String, i64>,
    /// Where it comes back to (dying, or a script sending it there); `None`: the map's.
    #[serde(default)]
    pub record: Option<c3_scene::PortalTarget>,
    pub running: bool,
    pub task: Task,
    /// The dialogue the player is in, if any.
    pub dialogue: Option<Conversation>,
    /// The NPC whose shop the player has open.
    pub shop: Option<u32>,
    /// The booth the player keeps, if it has one open.
    #[serde(default)]
    pub booth: Option<crate::Booth>,
    /// Script variables kept with the player (quest steps, counters).
    pub flags: BTreeMap<String, i64>,
    /// Script text variables kept with the player (a name typed for a later step).
    pub texts: BTreeMap<String, String>,
    pub(crate) path: VecDeque<CellPos>,
    /// Game time (ms) the next step or hit may happen.
    pub(crate) next_step_ms: u64,
    pub(crate) next_attack_ms: u64,
    /// When a dead player comes back to life (or may ask to).
    pub(crate) revive_at_ms: Option<u64>,
    /// Dead and up as a ghost.
    #[serde(default)]
    pub ghost: bool,
    /// Turned into a monster of this type until this game time (ms).
    #[serde(skip)]
    pub transform: Option<(u32, u64)>,
    /// The type of the monster that last killed it (0: none, or a player).
    #[serde(skip)]
    pub(crate) killed_by_monster: u32,
}

impl Player {
    pub fn is_alive(&self) -> bool {
        self.revive_at_ms.is_none()
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

    /// It as one side of a fight (see [`Fighter::facts`]).
    pub fn fighter(&self, rules: &dyn crate::Rules) -> Fighter {
        let mut facts = (rules.monster(self.kind))
            .map(|t| t.facts.clone())
            .unwrap_or_default();
        facts.insert("monster".to_owned(), i64::from(self.kind));
        crate::statuses::add_status_facts(&mut facts, &self.statuses, rules);
        Fighter {
            level: self.level,
            stats: self.stats,
            facts,
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
    /// The very item a player dropped (its plus, gems and the rest kept).
    #[serde(default)]
    pub(crate) item: Option<Item>,
    pub(crate) expires_ms: u64,
    /// Whose loot it is, and until when only it (its spouse, its team) may pick it up.
    #[serde(default)]
    pub(crate) owner: Option<EntityId>,
    #[serde(default)]
    pub(crate) protected_until_ms: u64,
}
