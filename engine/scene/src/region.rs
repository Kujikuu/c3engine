//! Regions: rectangles of a map that change what happens there — music and ambient sound,
//! safe zones where no one fights, zones where players can't attack each other, booth
//! (market) areas, weather, light, and script events when a player walks in or out.

use crate::CellPos;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Region {
    /// Unique on the map; scripts get it in `on_enter_region(id)` / `on_leave_region(id)`.
    pub id: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// First and last cell, inclusive.
    pub min: CellPos,
    pub max: CellPos,
    /// Music played while inside (a sound asset); none when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub music: String,
    /// Sound looped while inside (wind, a crowd); none when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sound: String,
    /// No one fights here: players, monsters and guards alike.
    #[serde(default, skip_serializing_if = "is_false")]
    pub safe: bool,
    /// Players can't attack each other here, even on a PK map.
    #[serde(default, skip_serializing_if = "is_false")]
    pub no_pk: bool,
    /// Players may set up booths (shops of their own) here.
    #[serde(default, skip_serializing_if = "is_false")]
    pub booth: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weather: Option<Weather>,
    /// The light here (overrides the map's); none: the map's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<[u8; 3]>,
    /// Game values the engine doesn't interpret (TQ region type and data), kept for export.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}

/// What falls from the sky, and how much (1–100).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Weather {
    pub kind: WeatherKind,
    pub amount: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WeatherKind {
    Rain,
    Snow,
    /// Falling leaves or petals.
    Leaves,
}

impl WeatherKind {
    pub const ALL: [Self; 3] = [Self::Rain, Self::Snow, Self::Leaves];

    pub fn label(self) -> &'static str {
        match self {
            Self::Rain => "Rain",
            Self::Snow => "Snow",
            Self::Leaves => "Leaves",
        }
    }
}

fn is_false(b: &bool) -> bool {
    !b
}

impl Region {
    /// Whether `cell` lies in the region.
    pub fn contains(&self, cell: CellPos) -> bool {
        (self.min.0..=self.max.0).contains(&cell.0) && (self.min.1..=self.max.1).contains(&cell.1)
    }

    pub fn center(&self) -> CellPos {
        ((self.min.0 + self.max.0) / 2, (self.min.1 + self.max.1) / 2)
    }

    /// A name for lists: its own, or what it does.
    pub fn label(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        let mut parts = Vec::new();
        if self.safe {
            parts.push("Safe zone".to_owned());
        }
        if self.no_pk {
            parts.push("No PK".to_owned());
        }
        if self.booth {
            parts.push("Market".to_owned());
        }
        if let Some(w) = self.weather {
            parts.push(w.kind.label().to_owned());
        }
        if !self.music.is_empty() {
            parts.push("Music".to_owned());
        }
        match parts.is_empty() {
            true => format!("Region {}", self.id),
            false => parts.join(", "),
        }
    }
}
