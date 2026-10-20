//! c3-game: the gameplay simulation shared by the Studio's Play mode, the client and (M6)
//! the server. A [`Sim`] runs one map in fixed ticks: players walk, fight monsters born in
//! spawn areas, earn experience and levels, pick up drops and trade with shops. What things
//! are and how fights are settled comes from the game through [`Rules`]; its numbers (ticks,
//! ranges, timings, the bag) from the project's [`GameplaySettings`]; the engine keeps the
//! world, movement, timing and order (see docs/DECISIONS.md: a typed world, no ECS).

mod booths;
mod broadcasts;
mod chat;
mod chests;
mod commands;
mod dialogue;
mod entity;
mod event;
mod friends;
mod guilds;
mod index;
mod input;
pub(crate) mod locks;
mod marriage;
mod mentors;
mod monster_ai;
mod monsters;
mod nobility;
mod npc_fights;
mod partners;
mod players;
mod portals;
mod pvp;
mod quests;
mod regions;
mod remote;
mod rng;
mod rules;
mod save;
mod script_events;
mod sim;
mod skills;
mod statuses;
mod storage;
pub use guilds::GuildAction;
mod teams;
mod trades;
pub use trades::TradeNews;
mod world_state;
/// A map made while playing, as the world state keeps it.
pub use c3_script::MapCopyView;
pub use teams::{Mate, TEAM_SIZE, TeamAction, TeamOp};
pub use world_state::{GUILD_RELATIONS, Guild, Post, WorldState};

pub use booths::{Booth, BoothAction, MAX_WARES, Ware};
pub use broadcasts::{Broadcast, BroadcastAction};
pub use c3_core::settings::GameplaySettings;
pub use chat::{ChatChannel, ChatLine};
pub use commands::HELP as COMMAND_HELP;
pub use entity::{GroundItem, Item, Monster, NameMark, Player, PlayerLook, Task};
pub use event::Event;
pub use friends::{FriendAction, MAX_ENEMIES, MAX_FRIENDS};
pub use input::{Input, MAX_CHAT};
pub use mentors::{Gifts, MentorAction, MentorGift, Mentorship};
pub use nobility::{Donation, Noble, rank_by_value};
pub use npc_fights::{npc_destroyed_event, npc_life_flag};
pub use partners::{MAX_PARTNERS, PartnerAction};
pub use remote::{EntityState, Viewer, WorldUpdate};
pub use rng::Rng;
pub use rules::{
    Drop, Fighter, HUNTS_BLACK, HUNTS_CRIMINALS, HUNTS_GOOD, HUNTS_RED, ItemType, Loot,
    MonsterType, Rules, SkillKind, SkillTarget, SkillType, Stats, StatsFor, StatusType, find_rules,
};
pub use save::{PlayerSave, SavedItem};
pub use sim::Sim;
pub use skills::CastAt;
pub use statuses::ActiveStatus;

use c3_scene::CellPos;
use serde::{Deserialize, Serialize};

/// A player, monster or ground item; unique in a [`Sim`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EntityId(pub u32);

/// Seconds as whole milliseconds.
pub(crate) fn ms(seconds: f64) -> u64 {
    (seconds * 1000.0).round().max(0.0) as u64
}

/// Steps between two cells (diagonals count one).
pub fn distance(a: CellPos, b: CellPos) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

#[cfg(test)]
mod tests;
