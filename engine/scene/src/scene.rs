//! Scene contents: ground tiles, portals, entities, parallax layers, population.

use crate::{CellPos, Grid, IsoProjection, Npc, Population, Spawn, WorldPos};
use schemars::JsonSchema;
use std::collections::BTreeMap;

/// Maps tile ids to image asset paths.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tileset {
    /// Asset this tileset was loaded from (for export / display).
    pub source: String,
    pub frames: BTreeMap<u16, String>,
}

/// Grid of square image tiles placed in world space.
#[derive(Debug, Clone, PartialEq)]
pub struct TileLayer {
    /// Asset this layer was loaded from.
    pub source: String,
    pub tile_size: u32,
    pub width: u32,
    pub height: u32,
    /// Row-major tile ids; `None` = empty.
    pub tiles: Vec<Option<u16>>,
    pub tileset: Tileset,
    /// World position of the top-left tile.
    pub origin: WorldPos,
    /// Auto-scroll speed (for sky/water layers).
    pub scroll: (i32, i32),
}

impl TileLayer {
    /// Layer whose source couldn't be loaded: keeps the reference, has no tiles.
    pub fn missing(source: &str) -> Self {
        Self {
            source: source.to_owned(),
            tile_size: 1,
            width: 0,
            height: 0,
            tiles: Vec::new(),
            tileset: Tileset::default(),
            origin: (0, 0),
            scroll: (0, 0),
        }
    }

    pub fn tile(&self, x: u32, y: u32) -> Option<u16> {
        if x < self.width && y < self.height {
            self.tiles[(y * self.width + x) as usize]
        } else {
            None
        }
    }

    /// World size in pixels.
    pub fn world_size(&self) -> (u32, u32) {
        (self.width * self.tile_size, self.height * self.tile_size)
    }

    pub fn unique_tiles(&self) -> usize {
        let mut ids: Vec<u16> = self.tiles.iter().flatten().copied().collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    }
}

/// Exit point on the map; `id` is matched to a destination by the game's data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Portal {
    pub cell: CellPos,
    pub id: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, JsonSchema)]
pub enum Placement {
    Cell(CellPos),
    World(WorldPos),
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct EffectTransform {
    /// Rotation angles in degrees (pitch, pitch_alt, yaw); kept verbatim from sources.
    pub angles: [f32; 3],
    pub scale: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, JsonSchema)]
pub enum EntityKind {
    /// Animated sprite that occludes the cells it stands on (trees, buildings).
    Sprite {
        /// Animation set asset and the animation within it.
        asset: String,
        animation: String,
        /// Footprint in cells.
        footprint: (u32, u32),
        /// Sprite top-left in pixels relative to the cell centre.
        offset: (i32, i32),
        frame_ms: u32,
    },
    /// Reference to a reusable multi-part object asset.
    Prefab { asset: String },
    Sound {
        asset: String,
        range: i32,
        volume: i32,
    },
    /// Particle / 3D effect by name; `transform` is optional.
    Effect {
        name: String,
        transform: Option<EffectTransform>,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct Entity {
    pub placement: Placement,
    pub kind: EntityKind,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LayerItem {
    Sprite {
        asset: String,
        animation: String,
        cell: CellPos,
        /// Sprite top-left in pixels relative to the cell centre.
        offset: (i32, i32),
        frame_ms: u32,
        blend: u32,
    },
    Tiles {
        asset: String,
    },
}

/// Background/foreground layer drawn with its own scroll rate.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ParallaxLayer {
    /// Draw order slot among all layers.
    pub order: i32,
    pub scroll_rate: (i32, i32),
    pub items: Vec<LayerItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub grid: Grid,
    pub projection: IsoProjection,
    pub ground: Option<TileLayer>,
    pub portals: Vec<Portal>,
    pub entities: Vec<Entity>,
    pub layers: Vec<ParallaxLayer>,
    pub npcs: Vec<Npc>,
    pub spawns: Vec<Spawn>,
    /// Format-specific values an exporter needs back (e.g. file version). Not edited by tools.
    pub meta: BTreeMap<String, String>,
}

impl Scene {
    pub fn cell_to_world(&self, cell: CellPos) -> (f32, f32) {
        self.projection.cell_to_world(cell)
    }

    pub fn world_to_cell(&self, world: (f32, f32)) -> CellPos {
        self.projection.world_to_cell(world)
    }

    pub fn portal_at(&self, cell: CellPos) -> Option<usize> {
        self.portals.iter().position(|p| p.cell == cell)
    }

    /// The map's NPCs and spawn areas.
    pub fn population(&self) -> Population {
        Population {
            npcs: self.npcs.clone(),
            spawns: self.spawns.clone(),
        }
    }

    pub fn set_population(&mut self, population: Population) {
        self.npcs = population.npcs;
        self.spawns = population.spawns;
    }

    /// World position of an entity.
    pub fn entity_world(&self, e: &Entity) -> (f32, f32) {
        match e.placement {
            Placement::Cell(c) => self.cell_to_world(c),
            Placement::World((x, y)) => (x as f32, y as f32),
        }
    }
}
