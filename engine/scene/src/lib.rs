//! c3-scene: game-agnostic 2.5D map model.
//!
//! A [`Scene`] is a cell grid (walkability, surface, height) plus a tiled ground layer,
//! portals, placed entities, parallax layers and its population (NPCs, monster spawns,
//! regions). Format plugins import and export it.
//! Editors and the runtime only ever touch these types.

pub mod draw_order;
mod grid;
pub mod native;
pub mod path;
mod population;
mod projection;
mod region;
mod scene;

pub use grid::{Cell, Grid};
pub use population::{Npc, Population, PortalLink, Spawn};
pub use projection::IsoProjection;
pub use region::{Region, Weather, WeatherKind};
pub use scene::{
    EffectTransform, Entity, EntityKind, LayerItem, ParallaxLayer, Placement, Portal, PortalTarget,
    Scene, TileLayer, Tileset,
};

/// Integer cell coordinate.
pub type CellPos = (i32, i32);
/// Integer world (pixel) coordinate.
pub type WorldPos = (i32, i32);
