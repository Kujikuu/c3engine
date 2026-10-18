//! TQ map ⇄ engine [`Scene`] conversion (DMap + PUL + ANI).
//!
//! Import resolves the map's puzzle and tileset through an [`AssetSource`]. Export rebuilds
//! the DMap; the result is semantically identical to the original (bytes after fixed-string
//! terminators are not preserved). Ground (PUL) export lands with ground editing.

use crate::ani::Ani;
use crate::dmap::{self, DMap, InteractiveObj, Passage, SceneLayer, SceneObj};
use crate::io::{Error, FixedStr, Result};
use crate::pul::{EMPTY, GRID_SIZE, Puzzle};
use c3_assets::AssetSource;
use c3_scene::{
    Cell, EffectTransform, Entity, EntityKind, Grid, IsoProjection, LayerItem, ParallaxLayer,
    Placement, Portal, Scene, TileLayer, Tileset,
};
use std::collections::BTreeMap;

pub const CELL_W: f32 = 64.0;
pub const CELL_H: f32 = 32.0;

const META_VERSION: &str = "tq.dmap.version";
const META_DATA: &str = "tq.dmap.data";

/// An imported scene plus non-fatal problems (e.g. missing ground assets).
pub struct Imported {
    pub scene: Scene,
    pub warnings: Vec<String>,
}

/// Loads a DMap and its ground into a [`Scene`]. Only an unreadable DMap is an error;
/// missing ground files produce warnings and an empty ground layer.
pub fn import_map(source: &dyn AssetSource, dmap_path: &str) -> Result<Imported> {
    let bytes = source
        .read(dmap_path)
        .ok_or_else(|| Error::Invalid(format!("file not found: {dmap_path}")))?;
    let dmap = DMap::parse(&bytes)?;
    let mut warnings = Vec::new();
    let pul_path = dmap.puzzle_file.to_gbk_string();
    let ground = match load_ground(source, &dmap, &pul_path) {
        Ok(layer) => layer,
        Err(e) => {
            warnings.push(format!("Ground not loaded: {e}"));
            TileLayer::missing(&pul_path)
        }
    };
    Ok(Imported {
        scene: scene_from_dmap(&dmap, ground),
        warnings,
    })
}

fn load_ground(source: &dyn AssetSource, dmap: &DMap, pul_path: &str) -> Result<TileLayer> {
    let mut layer = load_tile_layer(source, pul_path)?;
    let puzzle_size = (layer.width, layer.height);
    layer.origin = ground_origin(dmap, puzzle_size);
    Ok(layer)
}

/// Loads a PUL and its ANI tileset as a tile layer with origin (0, 0). Used for the
/// ground and for parallax background layers.
pub fn load_tile_layer(source: &dyn AssetSource, pul_path: &str) -> Result<TileLayer> {
    let read = |p: &str| {
        source
            .read(p)
            .ok_or_else(|| Error::Invalid(format!("file not found: {p}")))
    };
    let puzzle = Puzzle::parse(&read(pul_path)?)?;
    let ani_path = puzzle.ani_file.to_gbk_string();
    let ani = Ani::parse(&read(&ani_path)?)?;
    Ok(tile_layer(pul_path, &puzzle, &ani_path, &ani))
}

/// Builds a scene from a parsed DMap and its ground layer.
pub fn scene_from_dmap(dmap: &DMap, ground: TileLayer) -> Scene {
    let grid = Grid {
        width: dmap.width,
        height: dmap.height,
        cells: dmap
            .cells
            .iter()
            .map(|c| Cell {
                flags: c.mask,
                surface: c.terrain,
                height: c.altitude,
            })
            .collect(),
    };
    let mut meta = BTreeMap::new();
    meta.insert(META_VERSION.to_owned(), dmap.version.to_string());
    meta.insert(META_DATA.to_owned(), dmap.data.to_string());

    Scene {
        projection: IsoProjection::for_grid(CELL_W, CELL_H, dmap.width),
        ground: Some(ground),
        portals: dmap
            .passages
            .iter()
            .map(|p| Portal {
                cell: (p.x, p.y),
                id: p.index,
            })
            .collect(),
        entities: dmap.interactive.iter().map(entity_from_tq).collect(),
        layers: dmap.scene_layers.iter().map(layer_from_tq).collect(),
        grid,
        meta,
    }
}

/// Converts a scene back to a DMap.
pub fn export_dmap(scene: &Scene) -> DMap {
    let meta = |k: &str, default: u32| {
        scene
            .meta
            .get(k)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    DMap {
        version: meta(META_VERSION, 1004),
        data: meta(META_DATA, 0),
        puzzle_file: FixedStr::from_gbk(scene.ground.as_ref().map_or("", |g| g.source.as_str())),
        width: scene.grid.width,
        height: scene.grid.height,
        cells: scene
            .grid
            .cells
            .iter()
            .map(|c| dmap::Cell {
                mask: c.flags,
                terrain: c.surface,
                altitude: c.height,
            })
            .collect(),
        passages: scene
            .portals
            .iter()
            .map(|p| Passage {
                x: p.cell.0,
                y: p.cell.1,
                index: p.id,
            })
            .collect(),
        interactive: scene.entities.iter().map(entity_to_tq).collect(),
        scene_layers: scene.layers.iter().map(layer_to_tq).collect(),
    }
}

/// World position of the puzzle's top-left. Port of `CGameMap::GetBgWorldPos`.
fn ground_origin(dmap: &DMap, (pw, ph): (u32, u32)) -> (i32, i32) {
    let (bw, bh) = ((pw * GRID_SIZE) as i32, (ph * GRID_SIZE) as i32);
    let (w, h) = (dmap.width as i32, dmap.height as i32);
    let (cw, ch) = (CELL_W as i32, CELL_H as i32);
    (
        cw * w / 2 - bw / 2,
        ch / 2 + ch * h / 2 - bh / 2 - ((h + 1) % 2) * ch / 2,
    )
}

fn tile_layer(pul_path: &str, puzzle: &Puzzle, ani_path: &str, ani: &Ani) -> TileLayer {
    let frames = ani
        .sections
        .iter()
        .filter_map(|s| {
            let id = s.title_str().strip_prefix("Puzzle")?.parse::<u16>().ok()?;
            Some((id, gbk(s.frames.first()?)))
        })
        .collect();
    TileLayer {
        source: pul_path.to_owned(),
        tile_size: GRID_SIZE,
        width: puzzle.width,
        height: puzzle.height,
        tiles: puzzle
            .tiles
            .iter()
            .map(|&t| (t != EMPTY).then_some(t))
            .collect(),
        tileset: Tileset {
            source: ani_path.to_owned(),
            frames,
        },
        origin: (0, 0),
        scroll: puzzle.roll_speed.unwrap_or((0, 0)),
    }
}

fn gbk(bytes: &[u8]) -> String {
    encoding_rs::GBK
        .decode_without_bom_handling(bytes)
        .0
        .into_owned()
}

fn entity_from_tq(o: &InteractiveObj) -> Entity {
    let (placement, kind) = match o {
        InteractiveObj::Cover {
            ani_file,
            ani_title,
            cell,
            base,
            offset,
            frame_interval,
        } => (
            Placement::Cell(*cell),
            EntityKind::Sprite {
                asset: ani_file.to_gbk_string(),
                animation: ani_title.to_gbk_string(),
                footprint: *base,
                // TQ covers draw at cell - offset; engine offsets are top-left from the cell centre.
                offset: (-offset.0, -offset.1),
                frame_ms: *frame_interval,
            },
        ),
        InteractiveObj::Terrain { file, cell } => (
            Placement::Cell(*cell),
            EntityKind::Prefab {
                asset: file.to_gbk_string(),
            },
        ),
        InteractiveObj::Sound {
            file,
            world,
            range,
            volume,
        } => (
            Placement::World(*world),
            EntityKind::Sound {
                asset: file.to_gbk_string(),
                range: *range,
                volume: *volume,
            },
        ),
        InteractiveObj::Effect { index, world } => (
            Placement::World(*world),
            EntityKind::Effect {
                name: index.to_gbk_string(),
                transform: None,
            },
        ),
        InteractiveObj::EffectNew {
            index,
            world,
            angles,
            scale,
        } => (
            Placement::World(*world),
            EntityKind::Effect {
                name: index.to_gbk_string(),
                transform: Some(EffectTransform {
                    angles: *angles,
                    scale: *scale,
                }),
            },
        ),
    };
    Entity { placement, kind }
}

fn entity_to_tq(e: &Entity) -> InteractiveObj {
    let pos = match e.placement {
        Placement::Cell(c) | Placement::World(c) => c,
    };
    match &e.kind {
        EntityKind::Sprite {
            asset,
            animation,
            footprint,
            offset,
            frame_ms,
        } => InteractiveObj::Cover {
            ani_file: FixedStr::from_gbk(asset),
            ani_title: FixedStr::from_gbk(animation),
            cell: pos,
            base: *footprint,
            offset: (-offset.0, -offset.1),
            frame_interval: *frame_ms,
        },
        EntityKind::Prefab { asset } => InteractiveObj::Terrain {
            file: FixedStr::from_gbk(asset),
            cell: pos,
        },
        EntityKind::Sound {
            asset,
            range,
            volume,
        } => InteractiveObj::Sound {
            file: FixedStr::from_gbk(asset),
            world: pos,
            range: *range,
            volume: *volume,
        },
        EntityKind::Effect {
            name,
            transform: None,
        } => InteractiveObj::Effect {
            index: FixedStr::from_gbk(name),
            world: pos,
        },
        EntityKind::Effect {
            name,
            transform: Some(t),
        } => InteractiveObj::EffectNew {
            index: FixedStr::from_gbk(name),
            world: pos,
            angles: t.angles,
            scale: t.scale,
        },
    }
}

fn layer_from_tq(l: &SceneLayer) -> ParallaxLayer {
    let items = l
        .objects
        .iter()
        .map(|o| match o {
            SceneObj::Scene {
                ani_file,
                ani_title,
                cell,
                offset,
                frame_interval,
                show_way,
            } => LayerItem::Sprite {
                asset: ani_file.to_gbk_string(),
                animation: ani_title.to_gbk_string(),
                cell: *cell,
                offset: (-offset.0, -offset.1),
                frame_ms: *frame_interval,
                blend: *show_way,
            },
            SceneObj::Puzzle { file } => LayerItem::Tiles {
                asset: file.to_gbk_string(),
            },
        })
        .collect();
    ParallaxLayer {
        order: l.index,
        scroll_rate: l.move_rate,
        items,
    }
}

fn layer_to_tq(l: &ParallaxLayer) -> SceneLayer {
    let objects = l
        .items
        .iter()
        .map(|i| match i {
            LayerItem::Sprite {
                asset,
                animation,
                cell,
                offset,
                frame_ms,
                blend,
            } => SceneObj::Scene {
                ani_file: FixedStr::from_gbk(asset),
                ani_title: FixedStr::from_gbk(animation),
                cell: *cell,
                offset: (-offset.0, -offset.1),
                frame_interval: *frame_ms,
                show_way: *blend,
            },
            LayerItem::Tiles { asset } => SceneObj::Puzzle {
                file: FixedStr::from_gbk(asset),
            },
        })
        .collect();
    SceneLayer {
        index: l.order,
        layer_type: dmap::LAYER_SCENE,
        move_rate: l.scroll_rate,
        objects,
    }
}
