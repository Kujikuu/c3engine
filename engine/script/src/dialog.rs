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
    /// Variables of the whole world (every player's scripts share them); unset reads as 0.
    pub world: BTreeMap<String, i64>,
    /// Statuses the player is under.
    pub statuses: Vec<i64>,
    /// The game id of the player's map and the cell it stands on.
    pub map: i64,
    pub cell: (i64, i64),
    pub life: i64,
    pub max_life: i64,
    /// How many are in the player's team (0: none) and its leader's name.
    pub team_size: i64,
    pub team_leader: String,
    /// The profession chosen when the character was made (0: none).
    pub profession: i64,
    /// PK points: what killing other players has earned (game-defined).
    pub pk: i64,
    /// Equipment slot → the item type worn there.
    pub worn: BTreeMap<i64, i64>,
    /// Equipment slot → the worn item's numbers, by the settings' item attribute names.
    pub worn_attrs: BTreeMap<i64, BTreeMap<String, i64>>,
    /// The game's numbers (every attribute the settings name, defaults included).
    pub attributes: BTreeMap<String, i64>,
    /// Each attribute's range (`None`: unlimited), by name.
    pub attribute_limits: BTreeMap<String, (Option<i64>, Option<i64>)>,
    /// Where the player comes back to: game map id and cell.
    pub record: Option<(i64, (i64, i64))>,
    /// Free places in the bag.
    pub bag_space: i64,
    /// Every item the player has (bag and worn), by its id.
    pub held: BTreeMap<i64, HeldItem>,
    pub mana: i64,
    pub max_mana: i64,
    /// The type of the monster that last killed the player (0: none, or a player).
    pub killed_by_monster: i64,
    /// Experience toward the next level.
    pub exp: i64,
    /// Fights between players on its map bring no crime or PK points.
    pub map_pk_free: bool,
    /// The look chosen when the character was made: body and hair (game-defined numbers).
    pub body: i64,
    pub hair: i64,
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

/// One of the player's items as scripts see it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeldItem {
    pub kind: i64,
    /// How many there are (a stack).
    pub count: i64,
    pub durability: i64,
    pub worn: bool,
    /// The game's numbers for it, by the settings' item attribute names.
    pub attrs: BTreeMap<String, i64>,
}

/// What the NPC says and the answers offered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogPage {
    pub lines: Vec<String>,
    pub options: Vec<DialogOption>,
    /// A picture shown with the page (the NPC's face; game-defined numbers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<i64>,
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
    /// Run `on_timer(name)` of the events script after `seconds`.
    StartTimer {
        name: String,
        seconds: f64,
    },
    /// Life back (`heal`), or taken (`hurt`, never below 1).
    Life(i64),
    /// PK points gained (negative: lost).
    Pk(i64),
    /// Mana back (negative: spent, never below 0).
    Mana(i64),
    /// The player drops this much of its money where it stands.
    DropMoney(i64),
    /// Signal `name` with three numbers, run by every events script and blueprint that
    /// answers it (`fn name(a, b, c)`) once this step is done.
    Signal {
        name: String,
        args: [i64; 3],
    },
    /// The item of type `item` given last gets `value` as attribute `name` (or, named
    /// "durability", as its durability).
    GivenAttr {
        item: i64,
        name: String,
        value: i64,
    },
    /// NPC `npc` goes to `cell` of game map `map` (another map: it leaves this one).
    MoveNpc {
        npc: i64,
        map: i64,
        cell: (i64, i64),
    },
    /// Each item in the player's bag falls where it stands with this chance (percent).
    DropBagItems(i64),
    /// Experience lost (never below 0).
    LoseExp(i64),
    /// Put the player under status `id`.
    AddStatus(i64),
    /// Play effect `name` on the player.
    PlayEffect(String),
    /// Play sound `path` where the player is.
    PlaySound(String),
    /// Tell every player on the map.
    Announce(String),
    /// Open the game UI's dialog `id` for the player.
    OpenWindow(i64),
    /// A monster of type `kind` appears at `cell`.
    Spawn {
        kind: i64,
        cell: (i64, i64),
    },
    /// The player walks to `cell`; `on_arrive(tag)` runs when it gets there.
    WalkTo {
        cell: (i64, i64),
        tag: String,
    },
    /// Tell the player's team.
    TeamMessage(String),
}

/// The result of running a dialogue step.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reply {
    /// The next page; empty when the dialogue is over.
    pub page: DialogPage,
    pub effects: Vec<Effect>,
}
