//! Who lives on a map: NPCs standing on cells and areas where monsters keep spawning.
//! The engine places and draws them; what they do comes from the game's data and scripts.
//! Map formats that can't hold them (TQ's DMap) keep them in a [`Population`] file next to
//! the map (see `native::save_population`).

use crate::CellPos;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A character placed on the map by the game: shopkeeper, guard, quest giver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Npc {
    /// Unique among the game's NPCs; scripts and saves refer to it.
    pub id: u32,
    pub name: String,
    pub cell: CellPos,
    /// Facing, 0–7: the classic directions (see `path::direction`).
    #[serde(default)]
    pub direction: u8,
    /// The game's model (look) id.
    pub look: u32,
    /// Game-defined kind (shop, storage, task giver…).
    #[serde(default)]
    pub kind: u32,
    /// Script run when a player talks to the NPC (`npc/guard` for `scripts/npc/guard.rhai`);
    /// empty for none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub script: String,
    /// Game values the engine doesn't interpret (task ids, owner…), kept for export.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}

/// An area where the game keeps monsters of one type alive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Spawn {
    /// Unique among the game's spawn areas.
    pub id: u32,
    /// The game's monster type id.
    pub monster: u32,
    /// First and last cell of the area, inclusive.
    pub min: CellPos,
    pub max: CellPos,
    /// Most monsters alive at once.
    pub max_alive: u32,
    /// Seconds between spawn waves.
    pub respawn_secs: u32,
    /// Most monsters born per wave.
    pub per_wave: u32,
    /// Game values the engine doesn't interpret (active hours…), kept for export.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}

impl Spawn {
    /// Whether `cell` lies in the area.
    pub fn contains(&self, cell: CellPos) -> bool {
        (self.min.0..=self.max.0).contains(&cell.0) && (self.min.1..=self.max.1).contains(&cell.1)
    }

    /// The cell at the middle of the area.
    pub fn center(&self) -> CellPos {
        ((self.min.0 + self.max.0) / 2, (self.min.1 + self.max.1) / 2)
    }
}

/// Where portal `portal` of the map leads (for maps whose files can't say).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PortalLink {
    pub portal: i32,
    pub to: crate::PortalTarget,
}

/// A map's NPCs, spawn areas and portal destinations, as one value (the population file of
/// a map).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Population {
    #[serde(default)]
    pub npcs: Vec<Npc>,
    #[serde(default)]
    pub spawns: Vec<Spawn>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<PortalLink>,
}

impl Population {
    pub fn is_empty(&self) -> bool {
        self.npcs.is_empty() && self.spawns.is_empty() && self.links.is_empty()
    }
}
