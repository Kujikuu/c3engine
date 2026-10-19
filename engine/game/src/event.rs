//! What happened in a tick, for views (animations, messages) and later the network. State
//! itself (life, money, bag) is read from the simulation; events say what changed it.

use crate::EntityId;
use c3_scene::CellPos;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    /// A monster, player or ground item came into the world.
    Appeared(EntityId),
    /// It left the world (a dead monster's body faded, an item was taken or expired).
    Vanished(EntityId),
    /// One step.
    Moved {
        id: EntityId,
        to: CellPos,
        running: bool,
    },
    /// Player `player` leaves for cell `cell` of map `map` (another map: the host moves it).
    Travel {
        player: EntityId,
        map: u32,
        cell: CellPos,
    },
    /// Put somewhere at once (a teleport or a revive).
    Placed {
        id: EntityId,
        cell: CellPos,
    },
    /// Jumped to a cell.
    Jumped {
        id: EntityId,
        to: CellPos,
    },
    /// A hit; `damage` 0 is a miss.
    Attacked {
        attacker: EntityId,
        target: EntityId,
        damage: u32,
    },
    Died {
        id: EntityId,
        killer: Option<EntityId>,
    },
    /// `caster` cast `skill` at `cell`, reaching `targets` (hits follow as `Attacked`).
    Cast {
        caster: EntityId,
        skill: u32,
        cell: CellPos,
        targets: Vec<EntityId>,
    },
    /// `target` got `amount` life back.
    Healed {
        target: EntityId,
        amount: u32,
    },
    /// `target` bears status `status` (anew, or again from the start).
    StatusStarted {
        target: EntityId,
        status: u32,
    },
    /// Status `status` wore off `target`.
    StatusEnded {
        target: EntityId,
        status: u32,
    },
    Revived(EntityId),
    ExpGained {
        player: EntityId,
        amount: u64,
    },
    LeveledUp {
        player: EntityId,
        level: u32,
    },
    PickedUp {
        player: EntityId,
        money: u64,
        kind: Option<u32>,
    },
    Bought {
        player: EntityId,
        kind: u32,
        price: u64,
    },
    Sold {
        player: EntityId,
        kind: u32,
        price: u64,
    },
    Used {
        player: EntityId,
        kind: u32,
    },
    /// What the player wears changed (its look may change).
    Dressed {
        player: EntityId,
    },
    /// Something the player should be told ("Your bag is full").
    Message {
        player: EntityId,
        text: String,
    },
    /// A message from a script or a gift ("You received 1 Stancher").
    Notice {
        player: EntityId,
        text: String,
    },
    /// The player's dialogue opened, moved on or closed (read it from the player).
    Dialogue {
        player: EntityId,
    },
    /// An NPC's shop opened for the player.
    ShopOpened {
        player: EntityId,
        npc: u32,
    },
    /// The player said something (those around hear it).
    Chat {
        player: EntityId,
        name: String,
        text: String,
    },
    /// The answer to the player's chat command (`ok`: it worked).
    CommandReply {
        player: EntityId,
        ok: bool,
        text: String,
    },
    /// A script printed (for the console).
    ScriptLog(String),
    /// A script played effect `effect` on `entity`.
    Effect {
        entity: EntityId,
        effect: String,
    },
    /// A script played sound `sound` where `entity` is.
    Sound {
        entity: EntityId,
        sound: String,
    },
    /// A script opened the game UI's dialog `dialog` for `player`.
    OpenWindow {
        player: EntityId,
        dialog: u32,
    },
    /// A problem with the game's data (a spawn of an unknown monster type).
    Warning(String),
    /// A cue for player `player`'s client alone (see `client_cue`).
    ClientCue {
        player: EntityId,
        name: String,
        args: [i64; 2],
    },
    /// A client cue about player `player` for everyone who sees it.
    RoomCue {
        player: EntityId,
        name: String,
        args: [i64; 2],
    },
    /// The same with a text.
    RoomCueText {
        player: EntityId,
        name: String,
        a: i64,
        text: String,
    },
    /// Effect `effect` plays once on `cell` (everyone on the map sees it).
    CellEffect {
        cell: CellPos,
        effect: String,
    },
    /// Player `player`'s storage at NPC `npc` opened, or changed (its items or the money).
    StorageOpened {
        player: EntityId,
        npc: u32,
    },
    StorageChanged {
        player: EntityId,
        npc: u32,
    },
    /// A trap was placed on this map, or (by id) went away.
    TrapPlaced(c3_scene::Trap),
    TrapGone(u32),
    /// NPC `npc` moved to `cell` of this map, or (`None`) left it.
    NpcMoved {
        npc: u32,
        cell: Option<CellPos>,
    },
    /// An NPC came to this map from another.
    NpcArrived(c3_scene::Npc),
}
