//! What happened in a tick, for views (animations, messages) and later the network. State
//! itself (life, money, bag) is read from the simulation; events say what changed it.

use crate::EntityId;
use c3_scene::CellPos;

#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Put somewhere at once (a teleport or a revive).
    Placed {
        id: EntityId,
        cell: CellPos,
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
    /// Something the player should be told ("Your bag is full").
    Message {
        player: EntityId,
        text: String,
    },
    /// A problem with the game's data (a spawn of an unknown monster type).
    Warning(String),
}
