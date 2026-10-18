//! Conquer game module: everything specific to Conquer Online 5065 on top of the engine.
//! Engine crates must never depend on this.
//!
//! [`Content`] opens a 5065 client folder and hands out engine types (scenes, models, images).

pub mod character;

use c3_anim::Clip;
use c3_assets::Image;
use c3_assets::effect::Effect;
use c3_assets::model::Model;
use c3_assets::sprite::{Prefab, SpriteSet};
use c3_formats_tq::{
    ani::Ani,
    c3::C3File,
    dat::ActionTable,
    effect_io::{self, EffectTable},
    gamemap::GameMapCatalog,
    ini::{Ini, PartTable, PathTable},
    model_io,
    scene::TerrainObject,
    scene_io, sprite_io, texture,
    vfs::ClientFs,
};
use c3_scene::Scene;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use scene_io::Imported;

/// A 5065 client's assets: loose files, packed archives and the lookup tables.
pub struct Content {
    pub root: PathBuf,
    fs: ClientFs,
    catalog: GameMapCatalog,
    tables: Tables,
}

/// Lookup tables from `ini/`.
#[derive(Default)]
struct Tables {
    /// `3dobj.ini`: model id → C3 path.
    objects: PathTable,
    /// `3dtexture.ini`: texture id → image path.
    textures: PathTable,
    /// `3dmotion.ini`: body motion id → C3 path.
    motions: PathTable,
    /// `WeaponMotion.ini`: weapon motion id → C3 path.
    weapon_motions: PathTable,
    /// `3DEffect.ini`: effect name → parts and clock.
    effects: EffectTable,
    /// `3DEffectObj.ini`: effect part id → C3 path.
    effect_files: PathTable,
    armor: PartTable,
    armet: PartTable,
    weapon: PartTable,
    /// Socket part names (`RolePart.ini [Dumy]`), never drawn.
    sockets: Vec<String>,
    /// `Action.dat`: action key (look·10⁶ + weapon family·10³ + action) → ms per frame.
    frame_intervals: HashMap<u32, u32>,
}

impl Tables {
    fn load(fs: &ClientFs) -> Self {
        let bytes = |name: &str| {
            fs.read(&format!("ini/{name}"))
                .map(|b| b.into_owned())
                .unwrap_or_default()
        };
        let sockets = Ini::parse(&bytes("RolePart.ini"))
            .section("Dumy")
            .map(|s| {
                s.entries
                    .iter()
                    .filter(|(k, _)| k.starts_with("Dumy"))
                    .map(|(_, v)| v.clone())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            objects: PathTable::parse(&bytes("3dobj.ini")),
            textures: PathTable::parse(&bytes("3dtexture.ini")),
            motions: PathTable::parse(&bytes("3dmotion.ini")),
            weapon_motions: PathTable::parse(&bytes("WeaponMotion.ini")),
            effects: EffectTable::parse(&bytes("3DEffect.ini")),
            effect_files: PathTable::parse(&bytes("3DEffectObj.ini")),
            armor: PartTable::parse(&bytes("armor.ini")),
            armet: PartTable::parse(&bytes("armet.ini")),
            weapon: PartTable::parse(&bytes("weapon.ini")),
            sockets,
            frame_intervals: ActionTable::parse(&bytes("Action.dat"))
                .map(|t| t.entries.iter().map(|e| (e.action, e.interval)).collect())
                .unwrap_or_default(),
        }
    }
}

impl Content {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, String> {
        let root = root.as_ref().to_path_buf();
        let fs = ClientFs::open(&root).map_err(|e| format!("{}: {e}", root.display()))?;
        let catalog = fs
            .read("ini/GameMap.dat")
            .map(|b| GameMapCatalog::parse(&b))
            .transpose()
            .map_err(|e| format!("ini/GameMap.dat: {e}"))?
            .unwrap_or_default();
        let tables = Tables::load(&fs);
        Ok(Self {
            root,
            fs,
            catalog,
            tables,
        })
    }

    /// DMap path of map document `id` (as the server refers to maps).
    pub fn map_path(&self, id: u32) -> Option<String> {
        self.catalog.get(id).map(|e| e.path_str())
    }

    /// Map ids in the catalog.
    pub fn map_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.catalog.entries.iter().map(|e| e.id)
    }

    /// Loads a map (by DMap path) as an engine scene.
    pub fn load_map(&self, path: &str) -> Result<Imported, String> {
        scene_io::import_map(&self.fs, path).map_err(|e| format!("{path}: {e}"))
    }

    /// Loads map document `id`.
    pub fn load_map_id(&self, id: u32) -> Result<(String, Scene, Vec<String>), String> {
        let path = self
            .map_path(id)
            .ok_or_else(|| format!("map {id} is not in GameMap.dat"))?;
        let imported = self.load_map(&path)?;
        Ok((path, imported.scene, imported.warnings))
    }

    /// Loads model `id` from `3dobj.ini`, with the texture registered under the same id in
    /// `3dtexture.ini` (true for NPCs and monsters; equipment uses the part tables).
    pub fn load_model(&self, id: u64) -> Result<(Model, Option<String>), String> {
        let path = self
            .tables
            .objects
            .get(id)
            .ok_or_else(|| format!("model {id} is not in 3dobj.ini"))?;
        let mut model = self.load_model_path(path)?;
        model.mark_sockets(
            &self
                .tables
                .sockets
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        );
        Ok((model, self.tables.textures.get(id).map(str::to_owned)))
    }

    /// Loads a C3 model file (its embedded motion becomes the default animation).
    pub fn load_model_path(&self, path: &str) -> Result<Model, String> {
        let bytes = self
            .fs
            .read(path)
            .ok_or_else(|| format!("file not found: {path}"))?;
        let file = C3File::parse(&bytes).map_err(|e| format!("{path}: {e}"))?;
        model_io::import_model(&file).map_err(|e| format!("{path}: {e}"))
    }

    /// Loads a motion file's clips (one per model part, by index).
    pub fn load_motion(&self, path: &str) -> Option<Vec<Clip>> {
        model_io::import_motion(&C3File::parse(&self.fs.read(path)?).ok()?).ok()
    }

    /// Camera tilt for 3D actors in degrees: the 7th field of `ini/graphic.ini`
    /// (`LoadGraphicConfig5065`), −30 if absent or malformed.
    pub fn oblique_angle(&self) -> f32 {
        let text = self
            .fs
            .read("ini/graphic.ini")
            .map(|b| String::from_utf8_lossy(&b).into_owned());
        text.and_then(|t| {
            let fields: Vec<&str> = t.split_whitespace().collect();
            (fields.len() >= 7)
                .then(|| fields[6].parse::<f32>().ok())
                .flatten()
        })
        .unwrap_or(-30.0)
    }

    /// Loads effect `name` from `3DEffect.ini`. Like the client, a part whose file or
    /// texture is missing ends the effect there.
    pub fn load_effect(&self, name: &str) -> Result<Effect, String> {
        let info = self
            .tables
            .effects
            .get(name)
            .ok_or_else(|| format!("effect {name} is not in 3DEffect.ini"))?;
        let mut layers = Vec::with_capacity(info.parts.len());
        for part in &info.parts {
            let Some(path) = self.tables.effect_files.get(part.effect) else {
                break;
            };
            let Some(texture) = self.tables.textures.get(part.texture) else {
                break;
            };
            let Some(bytes) = self.fs.read(path) else {
                break;
            };
            let file = C3File::parse(&bytes).map_err(|e| format!("{path}: {e}"))?;
            let mut layer =
                effect_io::import_effect_layer(&file).map_err(|e| format!("{path}: {e}"))?;
            layer.texture = Some(texture.to_owned());
            layer.blend = effect_io::blend_from_d3d(part.src_blend, part.dst_blend);
            layers.push(layer);
        }
        let [x, y, z] = info.offset.map(|v| v as f32);
        Ok(Effect {
            layers,
            timing: info.timing,
            offset: glam::Vec3::new(x, y, z),
        })
    }

    /// Effect names in `3DEffect.ini`.
    pub fn effect_names(&self) -> impl Iterator<Item = &str> {
        self.tables.effects.names()
    }

    /// Loads an ANI file as a sprite set.
    pub fn load_sprite_set(&self, path: &str) -> Option<SpriteSet> {
        Some(sprite_io::sprite_set_from_ani(
            &Ani::parse(&self.fs.read(path)?).ok()?,
        ))
    }

    /// Loads a `.scene` terrain object as a prefab.
    pub fn load_prefab(&self, path: &str) -> Option<Prefab> {
        Some(sprite_io::prefab_from_scene(
            &TerrainObject::parse(&self.fs.read(path)?).ok()?,
        ))
    }

    /// Decodes an image asset (DDS/TGA).
    pub fn load_image(&self, path: &str) -> Option<Image> {
        texture::decode_image(&self.fs.read(path)?).ok()
    }
}

impl c3_render::SceneAssets for Content {
    fn sprite_set(&self, path: &str) -> Option<SpriteSet> {
        self.load_sprite_set(path)
    }

    fn prefab(&self, path: &str) -> Option<Prefab> {
        self.load_prefab(path)
    }

    fn tile_layer(&self, path: &str) -> Option<c3_scene::TileLayer> {
        scene_io::load_tile_layer(&self.fs, path).ok()
    }

    fn image(&self, path: &str) -> Option<Image> {
        self.load_image(path)
    }
}
