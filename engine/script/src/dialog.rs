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
    /// Script text variables kept with the player; unset reads as empty.
    pub texts: BTreeMap<String, String>,
    /// Variables of the whole world (every player's scripts share them); unset reads as 0.
    pub world: BTreeMap<String, i64>,
    /// Statuses the player is under.
    pub statuses: Vec<i64>,
    /// The items being worked together (in `on_combine`).
    pub combining: Vec<i64>,
    /// Where the player placed what it was asked to (`x`, `y`, `direction`), while the
    /// placement's event runs; else its own cell and facing.
    pub placed: (i64, i64, i64),
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
    /// The item being used (its id), while its `use_<type>` event runs; 0 otherwise.
    pub used_item: i64,
    /// The looks of the NPCs on the player's map.
    pub npc_looks: BTreeMap<i64, i64>,
    /// The player's living teammates on its map, and how many of them hold each item type.
    pub team_here: i64,
    /// Their names and levels (the player's own left out).
    pub team_names: Vec<String>,
    pub team_levels: Vec<i64>,
    /// The other living players on the player's map: how far each is and its statuses.
    pub nearby: Vec<(i64, Vec<i64>)>,
    pub team_items: BTreeMap<i64, i64>,
    /// The money the player keeps in storage.
    pub saved_money: i64,
    /// The guilds by id, and the guild owning each game map that has one.
    pub guilds: BTreeMap<i64, GuildView>,
    /// The wanted list, by entry id.
    pub wanted: BTreeMap<i64, WantedView>,
    /// Bounties on killers, by id (the oldest first).
    pub bounties: BTreeMap<i64, BountyView>,
    /// The police wanted list, the highest level first.
    pub police: Vec<PoliceView>,
    /// The players online on every map: the name of the map each is on.
    pub online: BTreeMap<String, String>,
    /// Maps made while playing (players' homes), by their new id.
    pub map_copies: BTreeMap<i64, MapCopyView>,
    /// The NPCs on the player's map: their names, kinds and distances from the player.
    pub map_npcs: Vec<(String, i64, i64)>,
    pub map_owners: BTreeMap<i64, i64>,
    /// The living monsters on the player's map, by their type's name.
    pub monsters: BTreeMap<String, i64>,
    /// What the player typed in the answer's text box (empty: none).
    pub accept: String,
    /// Players on each game map (all, alive).
    pub map_players: BTreeMap<i64, (i64, i64)>,
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
    /// A text box of this many characters at most: the answer is what the player types
    /// (`accept()` in `next`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<u32>,
}

/// An entry of the wanted list as scripts see it: who is wanted, who pays, the bounty.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WantedView {
    pub target: String,
    pub payer: String,
    pub bounty: i64,
}

/// Something waiting for a player who may be elsewhere or offline: a text variable set, an
/// item given.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Delivery {
    Text { name: String, value: String },
    Item { kind: i64 },
}

/// A name on the police wanted list as scripts see it: its guild, level and PK points.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PoliceView {
    pub name: String,
    pub guild: String,
    pub level: i64,
    pub pk: i64,
}

/// A map made while playing as scripts see it: the map it copies (`base`), its owner, where
/// one arrives (`x`, `y`) and its level.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MapCopyView {
    pub base: i64,
    pub owner: String,
    pub x: i64,
    pub y: i64,
    pub level: i64,
}

/// A bounty on a killer as scripts see it: the one killed (`target`), the killer
/// (`hunter`), what the reward is paid in (`kind`) and how much (0: not set yet).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BountyView {
    pub target: String,
    pub hunter: String,
    pub kind: i64,
    pub bonus: i64,
}

/// A guild as scripts see it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildView {
    pub name: String,
    pub leader: String,
    pub fund: i64,
    /// The guild it belongs to (0: none).
    pub master: i64,
    pub allies: Vec<i64>,
    pub enemies: Vec<i64>,
    /// Members by name and their rank.
    pub members: BTreeMap<String, i64>,
}

/// A change to what every map shares (guilds, map owners, world variables).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum WorldChange {
    Flag {
        name: String,
        value: i64,
    },
    /// Guild `id` made, `leader` its first member at `rank`.
    CreateGuild {
        id: i64,
        name: String,
        leader: String,
        rank: i64,
        fund: i64,
        master: i64,
    },
    DestroyGuild(i64),
    GuildFund {
        guild: i64,
        amount: i64,
    },
    /// Player `name` in guild `guild` at `rank` (0: out of it).
    Member {
        guild: i64,
        name: String,
        rank: i64,
    },
    /// Guild `other` made (`on`) or no longer an ally (both ways) or an enemy of `guild`.
    Relation {
        guild: i64,
        other: i64,
        ally: bool,
        on: bool,
    },
    /// Wanted entry `id`: `target` wanted by `payer` for `bounty` (0: the entry removed).
    Wanted {
        id: i64,
        target: String,
        payer: String,
        bounty: i64,
    },
    /// Map `id`, made while playing, set as `copy`.
    MapCopy {
        id: i64,
        copy: MapCopyView,
    },
    /// Guild `guild` led by its member `name` from now on.
    GuildLeader {
        guild: i64,
        name: String,
    },
    /// Player `name` (of `level`) put on the police wanted list, or taken off (`listed`).
    Police {
        name: String,
        level: i64,
        listed: bool,
        /// Its guild's name and PK points when put on.
        #[serde(default)]
        guild: String,
        #[serde(default)]
        pk: i64,
    },
    /// `delivery` waiting for player `to` until it is on a map.
    Deliver {
        to: String,
        delivery: Delivery,
    },
    /// What waited for player `to` given.
    Delivered {
        to: String,
    },
    /// Player `name` online on map `map` (0: logged out), of `level` (0: unchanged).
    Online {
        name: String,
        map: i64,
        #[serde(default)]
        level: i64,
    },
    /// Bounty `id` set as `view` (`None`: removed).
    Bounty {
        id: i64,
        view: Option<BountyView>,
    },
    /// Game map `map` owned by guild `guild` (0: nobody).
    MapOwner {
        map: i64,
        guild: i64,
    },
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
    /// A change to what every map shares.
    World(WorldChange),
    /// A cue for the player's client alone, by a name its front knows (a game's own client
    /// messages), with two numbers.
    ClientCue {
        name: String,
        args: [i64; 2],
    },
    /// The same, for everyone who sees the player.
    RoomCue {
        name: String,
        args: [i64; 2],
    },
    /// The player's teammates on its map each given (`count` > 0) or relieved of one item
    /// of type `item`, or told `text`.
    TeamItem {
        item: i64,
        count: i64,
    },
    TeamSay(String),
    /// NPC `npc` (on the player's map) takes look `look`.
    NpcLook {
        npc: i64,
        look: i64,
    },
    /// The player dies (no one's kill).
    Die,
    /// A room cue with a text (`room_cue_text`).
    RoomCueText {
        name: String,
        a: i64,
        text: String,
    },
    /// Effect `effect` played on each of the player's teammates on its map.
    TeamEffect(String),
    /// Timer `name` started at once for the first `count` players on game map `map` (all:
    /// a count below 1), each running its `on_timer`.
    MapTimer {
        map: i64,
        name: String,
        count: i64,
    },
    /// The NPCs of kind `kind` on this map gone.
    EraseNpcs(i64),
    /// A trap of kind `kind` (shown as `look`) on `cell` of game map `map` (this one).
    CreateTrap {
        kind: i64,
        look: i64,
        map: i64,
        cell: (i64, i64),
    },
    /// The trap the player set off, gone.
    EraseTrap,
    /// An item of type `item` lies on `cell` of game map `map` (this one).
    DropAt {
        item: i64,
        map: i64,
        cell: (i64, i64),
    },
    /// The living monsters of type `kind` on game map `map` (this one) gone.
    RemoveMonsters {
        map: i64,
        kind: i64,
    },
    /// Every player on game map `from` (this one) goes to `cell` of map `to`.
    MapTravel {
        from: i64,
        to: i64,
        cell: (i64, i64),
    },
    /// An NPC made on map `map` at `cell`: its name, look, kind, talk script and extra values
    /// (`key=value` pairs between spaces).
    CreateNpc {
        map: i64,
        cell: (i64, i64),
        name: String,
        look: i64,
        kind: i64,
        script: String,
        extra: String,
    },
    /// Status `status` on the other living players within `range` cells who lack it.
    AddStatusNear {
        status: i64,
        range: i64,
    },
    /// Status `status` on the player for `secs` (its type's own time: 0); taken off.
    AddStatusFor {
        status: i64,
        secs: i64,
    },
    RemoveStatus(i64),
    /// The NPC the player talks to opens its storage for it.
    OpenStorage,
    /// Money added to (or, below 0, taken from) what the player keeps in storage.
    SavedMoney(i64),
    /// World event `name` (no player) runs after `secs`.
    WorldTimer {
        name: String,
        secs: i64,
    },
    /// The player turned into a monster of type `monster` for `secs` (its look and its
    /// fighting numbers), after skill `skill`'s effect; refused while already one.
    Transform {
        monster: i64,
        secs: i64,
        skill: i64,
    },
    /// An item of type `item` (or `money`) lies where the monster being killed fell (where
    /// the player stands, out of a kill).
    Loot {
        item: i64,
        money: i64,
    },
    /// The player's team (the leader with every member on its map) goes to `cell` of game
    /// map `map`.
    TeamTravel {
        map: i64,
        cell: (i64, i64),
    },
    /// Effect `effect` played once on `cell` of game map `map`, for everyone there.
    EffectAt {
        map: i64,
        cell: (i64, i64),
        effect: String,
    },
    /// Play sound `path` where the player is.
    PlaySound(String),
    /// Tell every player on the map.
    Announce(String),
    /// Open the game UI's dialog `id` for the player.
    OpenWindow(i64),
    /// The team the player leads following it to `map` at `cell`: its living members within
    /// `range` cells of the player.
    TeleportTeam {
        map: i64,
        cell: (i64, i64),
        range: i64,
    },
    /// A client cue `name` with a number and a text, for the player's client alone.
    ClientCueText {
        name: String,
        a: i64,
        text: String,
    },
    /// The player asked where to place an NPC of `look` and `kind`; the answer runs the
    /// custom event `event`.
    AskPlacement {
        look: i64,
        kind: i64,
        event: String,
    },
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
