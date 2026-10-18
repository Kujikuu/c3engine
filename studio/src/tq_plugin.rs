//! The only module that knows about TQ/Conquer formats. It opens a client folder as an
//! asset source and converts maps and models to engine types; the scene view draws through
//! it ([`SceneAssets`]). Everything else in the Studio works on `c3_scene` / `c3_assets` types.
//!
//! When more games are supported this becomes one implementation of a format-plugin trait.

use crate::maps::MapRef;
use c3_assets::effect::Effect;
use c3_assets::sprite::{Prefab, SpriteSet};
use c3_assets::{Image, model::Model};
use c3_formats_tq::{c3::C3File, ini::PathTable, model_io, scene_io, texture, vfs::ClientFs};
use c3_render::SceneAssets;
use c3_scene::{Population, Scene, TileLayer};
use game_conquer::Content;
use game_conquer::server_data::{self, ServerPopulation};
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub use scene_io::Imported;

/// Kinds of TQ files, by extension (lowercase, without the dot), for the asset browser.
pub fn tq_kind(ext: &str) -> Option<crate::browser::AssetKind> {
    use crate::browser::AssetKind;
    Some(match ext {
        "dmap" => AssetKind::Map,
        "c3" => AssetKind::Model,
        "ani" => AssetKind::Sprites,
        "pul" => AssetKind::Puzzle,
        "wdf" | "dnp" => AssetKind::Package,
        "scene" | "part" => AssetKind::Terrain,
        "dat" | "ini" | "msk" | "tme" | "rgn" => AssetKind::Data,
        _ => return None,
    })
}

/// Decodes a DDS or TGA image file's bytes.
pub fn decode_tq_image(bytes: &[u8]) -> Option<Image> {
    texture::decode_image(bytes).ok()
}

/// One line describing a C3 file's contents, e.g. `2 meshes · 2 motions · 1 trail`.
pub fn c3_summary(bytes: &[u8]) -> String {
    use c3_formats_tq::c3::ChunkKind;
    let Ok(file) = C3File::parse(bytes) else {
        return "Not a readable C3 file".to_owned();
    };
    // (singular, plural, count)
    let mut counts: Vec<(&str, &str, usize)> = Vec::new();
    for chunk in &file.chunks {
        let (one, many) = match chunk.kind() {
            ChunkKind::Phy(_) => ("mesh", "meshes"),
            ChunkKind::Motion => ("motion", "motions"),
            ChunkKind::Shape => ("trail", "trails"),
            ChunkKind::Particles | ChunkKind::ParticlesExtended => {
                ("particle system", "particle systems")
            }
            ChunkKind::Camera => ("camera path", "camera paths"),
            ChunkKind::ShapeMotion | ChunkKind::Unknown => continue,
        };
        match counts.iter_mut().find(|(o, _, _)| *o == one) {
            Some((_, _, n)) => *n += 1,
            None => counts.push((one, many, 1)),
        }
    }
    if counts.is_empty() {
        return "Empty C3 file".to_owned();
    }
    counts
        .iter()
        .map(|(one, many, n)| format!("{n} {}", if *n == 1 { one } else { many }))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A game client folder: loose files plus packed archives.
pub struct Client {
    pub root: PathBuf,
    /// The game's own loader: maps' art and effects as the client draws them (shared with
    /// Play mode's world).
    content: Rc<Content>,
    /// `ini/3dobj.ini`: model id → C3 path.
    objects: PathTable,
    /// `ini/3dtexture.ini`: texture id → image path.
    textures: PathTable,
}

/// A model listed in the client's object table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelEntry {
    pub id: u64,
    pub path: String,
}

/// The parts of a Conquer 5065 client folder that are game content (copied into projects);
/// executables, libraries, logs and the patcher are left out.
pub const CONTENT_ENTRIES: [&str; 8] = [
    "ani", "c3", "data", "ini", "map", "sound", "c3.wdf", "data.wdf",
];

/// Checks that `root` is a Conquer 5065 client folder (it has the map catalog).
pub fn check_client(root: &Path) -> Result<(), String> {
    let fs = ClientFs::open(root).map_err(|e| format!("{}: {e}", root.display()))?;
    match fs.read("ini/GameMap.dat") {
        Some(_) => Ok(()),
        None => Err(format!(
            "{} doesn't look like a Conquer client folder (no ini/GameMap.dat).",
            root.display()
        )),
    }
}

impl Client {
    pub fn open(root: &str) -> Result<Self, String> {
        let content = Rc::new(Content::open(root)?);
        let table = |name: &str| {
            content
                .files()
                .read(&format!("ini/{name}"))
                .map(|b| PathTable::parse(&b))
                .unwrap_or_default()
        };
        Ok(Self {
            root: PathBuf::from(root),
            objects: table("3dobj.ini"),
            textures: table("3dtexture.ini"),
            content,
        })
    }

    /// The name of NPC type `look` in `Npc.ini` (e.g. `Storekeeper`).
    pub fn npc_type_name(&self, look: u32) -> Option<&str> {
        self.content.npc_type(look).map(|t| t.name.as_str())
    }

    /// The game's content loader (actors, NPC models).
    pub fn content(&self) -> &Content {
        &self.content
    }

    /// The content loader, shared (a played world keeps it).
    pub fn shared_content(&self) -> Rc<Content> {
        Rc::clone(&self.content)
    }

    fn fs(&self) -> &ClientFs {
        self.content.files()
    }

    /// Camera tilt of 3D models and effects on maps, in degrees.
    pub fn tilt_deg(&self) -> f32 {
        self.content.oblique_angle()
    }

    /// Map files found in the client, as asset paths.
    pub fn list_maps(&self) -> Vec<String> {
        let mut maps: Vec<String> = std::fs::read_dir(self.root.join("map").join("map"))
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|n| n.to_ascii_lowercase().ends_with(".dmap"))
                    .map(|n| format!("map/map/{n}"))
                    .collect()
            })
            .unwrap_or_default();
        maps.sort_by_key(|m| m.to_ascii_lowercase());
        maps
    }

    /// Game map ids (`GameMap.dat`) whose map file is `path`; several maps can share one.
    pub fn map_ids(&self, path: &str) -> Vec<u32> {
        self.content.map_ids_for_path(path)
    }

    /// The NPCs and spawn areas of `map` in a server SQL dump (TQ's `cq_npc`/`cq_generator`).
    pub fn server_population(&self, file: &Path, map: &MapRef) -> Result<Population, String> {
        let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
        let server = ServerPopulation::from_sql(&String::from_utf8_lossy(&bytes));
        let ids = match map {
            MapRef::Client(path) => self.map_ids(path),
            MapRef::Project(_) => Vec::new(),
        };
        if ids.is_empty() {
            return Err("this map has no game map id in GameMap.dat".into());
        }
        Ok(server.for_maps(&ids))
    }

    /// SQL writing `population` into a TQ/Redux database (rows keep their own map id; new
    /// ones get `map`'s first id).
    pub fn population_sql(&self, population: &Population, map: &MapRef) -> String {
        let default_map = match map {
            MapRef::Client(path) => self.map_ids(path).first().copied().unwrap_or(0),
            MapRef::Project(_) => 0,
        };
        server_data::population_sql(population, default_map)
    }

    /// Tables of a server SQL dump that hold the game's data (preselected on import).
    pub fn suggested_tables(&self) -> &'static [&'static str] {
        server_data::DATA_TABLES
    }

    /// Names of the effects in `3DEffect.ini`, sorted.
    pub fn effect_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.content.effect_names().map(str::to_owned).collect();
        names.sort_by_key(|n| n.to_lowercase());
        names
    }

    /// Models from `3dobj.ini`, by id.
    pub fn list_models(&self) -> Vec<ModelEntry> {
        self.objects
            .paths
            .iter()
            .map(|(&id, path)| ModelEntry {
                id,
                path: path.clone(),
            })
            .collect()
    }

    pub fn import_model(&self, path: &str) -> Result<Model, String> {
        let bytes = self
            .fs()
            .read(path)
            .ok_or_else(|| format!("File not found: {path}"))?;
        let file = C3File::parse(&bytes).map_err(|e| format!("{path}: {e}"))?;
        model_io::import_model(&file).map_err(|e| format!("{path}: {e}"))
    }

    /// Image path registered under the same id in `3dtexture.ini` (NPCs, monsters and
    /// effects share ids between their mesh and texture; equipment uses the part tables).
    pub fn texture_for(&self, id: u64) -> Option<&str> {
        self.textures.get(id)
    }

    pub fn import_map(&self, path: &str) -> Result<Imported, String> {
        scene_io::import_map(self.fs(), path).map_err(|e| format!("{path}: {e}"))
    }

    /// Bytes of `path` (loose or packed).
    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.fs().read(path).map(|b| b.into_owned())
    }

    /// Files inside the client's packages whose names are known, with their sizes, and the
    /// number of packed entries in all. WDF packages store hashed names only, so names come
    /// from what the client refers to: the ANI frame lists and the ini path tables.
    pub fn packed_files(&self) -> (Vec<(String, u64)>, usize) {
        let mut names: Vec<String> = Vec::new();
        let listed = |dir: &str, ext: &str| -> Vec<PathBuf> {
            std::fs::read_dir(self.root.join(dir))
                .map(|rd| {
                    rd.flatten()
                        .map(|e| e.path())
                        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext)))
                        .collect()
                })
                .unwrap_or_default()
        };
        for ani in listed("ani", "ani") {
            if let Ok(ani) = c3_formats_tq::ani::Ani::open(&ani) {
                for section in &ani.sections {
                    names.extend((0..section.frames.len()).filter_map(|i| section.frame_str(i)));
                }
            }
        }
        for ini in listed("ini", "ini") {
            if let Ok(bytes) = std::fs::read(&ini) {
                names.extend(PathTable::parse(&bytes).paths.into_values());
            }
        }
        let mut seen = std::collections::HashSet::new();
        let mut files = Vec::new();
        for name in names {
            let rel = name.trim().replace('\\', "/");
            if !rel.contains('.') || !seen.insert(rel.to_ascii_lowercase()) {
                continue;
            }
            if let Some(size) = self.fs().packed_size(&rel)
                && !self.root.join(&rel).is_file()
            {
                files.push((rel, u64::from(size)));
            }
        }
        (files, self.fs().packed_count())
    }

    /// Decodes an image asset (DDS/TGA).
    pub fn load_image(&self, path: &str) -> Option<Image> {
        texture::decode_image(&self.fs().read(path)?).ok()
    }

    /// Writes `scene` to the DMap at `path` (atomic). With `backup`, the original is first
    /// kept as `<file>.bak` (once), for content that isn't the project's own copy.
    pub fn save_map(&self, scene: &Scene, path: &str, backup: bool) -> Result<PathBuf, String> {
        let file = self.root.join(Path::new(path));
        write_map_file(&file, &scene_io::export_dmap(scene).to_bytes(), backup)?;
        Ok(file)
    }
}

pub fn write_map_file(file: &Path, bytes: &[u8], backup: bool) -> Result<(), String> {
    let mut bak = file.as_os_str().to_owned();
    bak.push(".bak");
    let bak = PathBuf::from(bak);
    if backup && file.exists() && !bak.exists() {
        std::fs::copy(file, &bak).map_err(|e| format!("Backup failed: {e}"))?;
    }
    let mut tmp = file.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes).map_err(|e| format!("Save failed: {e}"))?;
    std::fs::rename(&tmp, file).map_err(|e| format!("Save failed: {e}"))
}

impl SceneAssets for Client {
    fn sprite_set(&self, path: &str) -> Option<SpriteSet> {
        self.content.sprite_set(path)
    }

    fn prefab(&self, path: &str) -> Option<Prefab> {
        self.content.prefab(path)
    }

    fn tile_layer(&self, path: &str) -> Option<TileLayer> {
        self.content.tile_layer(path)
    }

    fn image(&self, path: &str) -> Option<Image> {
        self.content.image(path)
    }

    fn effect(&self, name: &str) -> Result<Effect, String> {
        self.content.effect(name)
    }
}
