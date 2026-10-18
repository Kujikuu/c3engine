//! What a script talks to and what it answers: the player it may change, the NPC it speaks
//! for, and the dialogue page it builds.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The player a script runs for: read and changed by the script, kept by the game.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerState {
    pub name: String,
    pub level: i64,
    pub money: i64,
    /// Item type → how many the player carries.
    pub items: BTreeMap<i64, i64>,
    /// Other currencies → how much the player has.
    pub wallet: BTreeMap<String, i64>,
    /// Skills the player knows.
    pub skills: Vec<i64>,
    /// Script variables kept with the player (quest steps, counters); unset reads as 0.
    pub flags: BTreeMap<String, i64>,
}

impl PlayerState {
    /// How much of currency `name` the player has.
    pub fn currency(&self, name: &str) -> i64 {
        self.wallet.get(name).copied().unwrap_or(0)
    }

    pub fn item_count(&self, item: i64) -> i64 {
        self.items.get(&item).copied().unwrap_or(0)
    }

    pub fn give_item(&mut self, item: i64, count: i64) {
        *self.items.entry(item).or_default() += count;
    }

    /// Takes `count` of `item` if the player has that many.
    pub fn take_item(&mut self, item: i64, count: i64) -> bool {
        let have = self.item_count(item);
        if have < count {
            return false;
        }
        if have == count {
            self.items.remove(&item);
        } else {
            self.items.insert(item, have - count);
        }
        true
    }
}

/// The NPC a dialogue is with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcInfo {
    pub id: u32,
    pub name: String,
}

/// One answer the player can pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogOption {
    pub label: String,
    /// Script function run when picked; `None` closes the dialogue.
    pub next: Option<String>,
}

/// What the NPC says and the answers offered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogPage {
    pub lines: Vec<String>,
    pub options: Vec<DialogOption>,
}

impl DialogPage {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.options.is_empty()
    }
}

/// Something a script did that the game shows or carries out.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// A system message to the player.
    Message(String),
    GaveItem {
        item: i64,
        count: i64,
    },
    TookItem {
        item: i64,
        count: i64,
    },
    /// Money gained (negative: paid).
    Money(i64),
    /// Currency `name` changed by `amount`.
    Currency {
        name: String,
        amount: i64,
    },
    /// The player learned skill `id`.
    LearnedSkill(i64),
    /// Move the player to `cell` of game map `map`.
    Teleport {
        map: i64,
        cell: (i64, i64),
    },
    /// Open the NPC's shop to the player.
    OpenShop,
    /// Experience gained.
    Exp(i64),
    /// The player took quest `id`.
    QuestTaken {
        id: i64,
        name: String,
    },
    /// The player completed quest `id`.
    QuestDone {
        id: i64,
        name: String,
    },
    /// `print` output, for the console.
    Log(String),
}

/// The result of running a dialogue step.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reply {
    /// The next page; empty when the dialogue is over.
    pub page: DialogPage,
    pub effects: Vec<Effect>,
}
