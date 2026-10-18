//! The engine's own scene format, stored in projects. Unencrypted and readable:
//!
//! - `<name>.scene.ron`: everything but the big grids, as RON text (projection, ground
//!   tileset, portals, entities, layers, NPCs, spawns, meta);
//! - `<name>.scene.bin`: the cell grid and ground tile ids, little-endian:
//!
//! ```text
//! magic "C3SCENE\0" · version u32 · width u32 · height u32 ·
//! width×height × { flags u16, surface u16, height i16 } ·
//! tiles u32 · tiles × tile id u16 (0xFFFF = empty)
//! ```

use crate::{
    Cell, Entity, Grid, IsoProjection, Npc, ParallaxLayer, Population, Portal, Scene, Spawn,
    TileLayer, Tileset, WorldPos,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};

/// File name ending of a scene.
pub const SCENE_SUFFIX: &str = ".scene.ron";
/// File name ending of a map's population file (for map formats that can't hold it).
pub const POPULATION_SUFFIX: &str = ".population.ron";
const FORMAT_VERSION: u32 = 1;
const DATA_MAGIC: &[u8; 8] = b"C3SCENE\0";
const NO_TILE: u16 = u16::MAX;

#[derive(Serialize, Deserialize)]
struct SceneFile {
    format: u32,
    /// Grid size; the cells are in the `.scene.bin`.
    grid: (u32, u32),
    projection: IsoProjection,
    ground: Option<Ground>,
    portals: Vec<Portal>,
    entities: Vec<Entity>,
    layers: Vec<ParallaxLayer>,
    #[serde(default)]
    npcs: Vec<Npc>,
    #[serde(default)]
    spawns: Vec<Spawn>,
    meta: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize)]
struct PopulationFile {
    format: u32,
    #[serde(default)]
    npcs: Vec<Npc>,
    #[serde(default)]
    spawns: Vec<Spawn>,
}

/// A ground layer without its tile ids (they are in the `.scene.bin`).
#[derive(Serialize, Deserialize)]
struct Ground {
    source: String,
    tile_size: u32,
    width: u32,
    height: u32,
    origin: WorldPos,
    scroll: (i32, i32),
    tileset: Tileset,
}

/// The `.scene.bin` next to a `.scene.ron`.
pub fn data_path(scene_file: &Path) -> PathBuf {
    scene_file.with_extension("bin")
}

fn invalid(file: &Path, why: impl std::fmt::Display) -> Error {
    Error::new(ErrorKind::InvalidData, format!("{}: {why}", file.display()))
}

/// Writes `scene` to `file` (`.scene.ron`) and its `.scene.bin`. Each file is written to a
/// temporary name first, so a failed save never leaves a half-written scene.
pub fn save_scene(scene: &Scene, file: &Path) -> Result<()> {
    let ground = scene.ground.as_ref();
    let text = SceneFile {
        format: FORMAT_VERSION,
        grid: (scene.grid.width, scene.grid.height),
        projection: scene.projection,
        ground: ground.map(|g| Ground {
            source: g.source.clone(),
            tile_size: g.tile_size,
            width: g.width,
            height: g.height,
            origin: g.origin,
            scroll: g.scroll,
            tileset: g.tileset.clone(),
        }),
        portals: scene.portals.clone(),
        entities: scene.entities.clone(),
        layers: scene.layers.clone(),
        npcs: scene.npcs.clone(),
        spawns: scene.spawns.clone(),
        meta: scene.meta.clone(),
    };
    let ron = ron::ser::to_string_pretty(&text, ron::ser::PrettyConfig::default())
        .map_err(|e| invalid(file, e))?;

    let tiles = ground.map_or(&[][..], |g| &g.tiles[..]);
    let mut data = Vec::with_capacity(20 + scene.grid.cells.len() * 6 + 4 + tiles.len() * 2);
    data.extend_from_slice(DATA_MAGIC);
    for v in [FORMAT_VERSION, scene.grid.width, scene.grid.height] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    for c in &scene.grid.cells {
        data.extend_from_slice(&c.flags.to_le_bytes());
        data.extend_from_slice(&c.surface.to_le_bytes());
        data.extend_from_slice(&c.height.to_le_bytes());
    }
    data.extend_from_slice(&(tiles.len() as u32).to_le_bytes());
    for t in tiles {
        data.extend_from_slice(&t.unwrap_or(NO_TILE).to_le_bytes());
    }

    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    write_atomic(&data_path(file), &data)?;
    write_atomic(file, ron.as_bytes())
}

fn write_atomic(file: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = file.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, file)
}

/// Reads the scene at `file` (`.scene.ron`) and its `.scene.bin`.
pub fn load_scene(file: &Path) -> Result<Scene> {
    let text: SceneFile =
        ron::from_str(&std::fs::read_to_string(file)?).map_err(|e| invalid(file, e))?;
    if text.format > FORMAT_VERSION {
        return Err(invalid(
            file,
            format!("format {} is newer than this engine supports", text.format),
        ));
    }
    let bin = data_path(file);
    let data = std::fs::read(&bin)?;
    let mut r = Reader {
        data: &data,
        pos: 0,
        file: &bin,
    };
    if r.take(8)? != DATA_MAGIC {
        return Err(invalid(&bin, "not a scene data file"));
    }
    let _version = r.u32()?;
    let (width, height) = (r.u32()?, r.u32()?);
    if (width, height) != text.grid {
        return Err(invalid(&bin, "grid size doesn't match the .scene.ron"));
    }
    let count = width as usize * height as usize;
    let mut cells = Vec::with_capacity(count);
    for _ in 0..count {
        cells.push(Cell {
            flags: r.u16()?,
            surface: r.u16()?,
            height: r.u16()? as i16,
        });
    }
    let tile_count = r.u32()? as usize;
    let mut tiles = Vec::with_capacity(tile_count);
    for _ in 0..tile_count {
        let t = r.u16()?;
        tiles.push((t != NO_TILE).then_some(t));
    }
    let ground = text.ground.map(|g| TileLayer {
        source: g.source,
        tile_size: g.tile_size,
        width: g.width,
        height: g.height,
        tiles,
        tileset: g.tileset,
        origin: g.origin,
        scroll: g.scroll,
    });
    if let Some(g) = &ground
        && g.tiles.len() != g.width as usize * g.height as usize
    {
        return Err(invalid(&bin, "ground tile count doesn't match its size"));
    }
    Ok(Scene {
        grid: Grid {
            width,
            height,
            cells,
        },
        projection: text.projection,
        ground,
        portals: text.portals,
        entities: text.entities,
        layers: text.layers,
        npcs: text.npcs,
        spawns: text.spawns,
        meta: text.meta,
    })
}

/// Writes `population` to `file` (`<map>.population.ron`); an empty one removes the file.
pub fn save_population(population: &Population, file: &Path) -> Result<()> {
    if population.is_empty() {
        return match std::fs::remove_file(file) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let text = PopulationFile {
        format: FORMAT_VERSION,
        npcs: population.npcs.clone(),
        spawns: population.spawns.clone(),
    };
    let ron = ron::ser::to_string_pretty(&text, ron::ser::PrettyConfig::default())
        .map_err(|e| invalid(file, e))?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    write_atomic(file, ron.as_bytes())
}

/// Reads the population file at `file`; a missing file is an empty population.
pub fn load_population(file: &Path) -> Result<Population> {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Population::default()),
        Err(e) => return Err(e),
    };
    let parsed: PopulationFile = ron::from_str(&text).map_err(|e| invalid(file, e))?;
    if parsed.format > FORMAT_VERSION {
        return Err(invalid(
            file,
            format!(
                "format {} is newer than this engine supports",
                parsed.format
            ),
        ));
    }
    Ok(Population {
        npcs: parsed.npcs,
        spawns: parsed.spawns,
    })
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    file: &'a Path,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let bytes = self
            .data
            .get(self.pos..self.pos + n)
            .ok_or_else(|| invalid(self.file, "file ends early"))?;
        self.pos += n;
        Ok(bytes)
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().unwrap_or_default(),
        ))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().unwrap_or_default(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EntityKind, LayerItem, Placement};

    #[test]
    fn scenes_round_trip_through_ron_and_bin() {
        let mut grid = Grid::new(3, 2);
        grid.cells[4] = Cell {
            flags: 1,
            surface: 7,
            height: -12,
        };
        let scene = Scene {
            grid,
            projection: IsoProjection::for_grid(64.0, 32.0, 3),
            ground: Some(TileLayer {
                source: "map/puzzle/x.pul".into(),
                tile_size: 256,
                width: 2,
                height: 1,
                tiles: vec![Some(3), None],
                tileset: Tileset {
                    source: "ani/x.ani".into(),
                    frames: [(3, "data/x/3.dds".to_owned())].into(),
                },
                origin: (-10, 20),
                scroll: (0, 0),
            }),
            portals: vec![Portal {
                cell: (1, 1),
                id: 4,
            }],
            entities: vec![Entity {
                placement: Placement::Cell((2, 0)),
                kind: EntityKind::Sprite {
                    asset: "ani/tree.ani".into(),
                    animation: "tree01".into(),
                    footprint: (1, 1),
                    offset: (-32, -96),
                    frame_ms: 100,
                },
            }],
            layers: vec![ParallaxLayer {
                order: 0,
                scroll_rate: (50, 50),
                items: vec![LayerItem::Tiles {
                    asset: "map/puzzle/sky.pul".into(),
                }],
            }],
            npcs: vec![Npc {
                id: 7,
                name: "Guard".into(),
                cell: (1, 0),
                direction: 2,
                look: 900,
                kind: 1,
                script: "npc/guard".into(),
                extra: [("task0".to_owned(), "100".to_owned())].into(),
            }],
            spawns: vec![Spawn {
                id: 3,
                monster: 12,
                min: (0, 0),
                max: (2, 1),
                max_alive: 5,
                respawn_secs: 10,
                per_wave: 2,
                extra: BTreeMap::new(),
            }],
            meta: [("version".to_owned(), "1".to_owned())].into(),
        };
        let dir = std::env::temp_dir().join(format!("c3-scene-native-{}", std::process::id()));
        let file = dir.join("test.scene.ron");
        save_scene(&scene, &file).unwrap();
        assert!(data_path(&file).ends_with("test.scene.bin"));
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            text.contains("tree01") && text.contains("portals"),
            "{text}"
        );
        assert_eq!(load_scene(&file).unwrap(), scene);

        let population_file = dir.join(format!("test{POPULATION_SUFFIX}"));
        assert_eq!(
            load_population(&population_file).unwrap(),
            Population::default()
        );
        save_population(&scene.population(), &population_file).unwrap();
        assert_eq!(
            load_population(&population_file).unwrap(),
            scene.population()
        );
        save_population(&Population::default(), &population_file).unwrap();
        assert!(!population_file.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
