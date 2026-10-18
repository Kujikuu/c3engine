//! Conquer game module: everything specific to Conquer Online 5065 on top of the engine.
//! Engine crates must never depend on this.
//!
//! [`Content`] opens a 5065 client folder and hands out engine types (scenes, models, images).

use c3_assets::Image;
use c3_assets::model::Model;
use c3_assets::sprite::{Prefab, SpriteSet};
use c3_formats_tq::{
    ani::Ani, c3::C3File, gamemap::GameMapCatalog, ini::PathTable, model_io, scene::TerrainObject,
    scene_io, sprite_io, texture, vfs::ClientFs,
};
use c3_scene::Scene;
use std::path::{Path, PathBuf};

pub use scene_io::Imported;

/// A 5065 client's assets: loose files, packed archives and the lookup tables.
pub struct Content {
    pub root: PathBuf,
    fs: ClientFs,
    catalog: GameMapCatalog,
    /// `ini/3dobj.ini`: model id → C3 path.
    objects: PathTable,
    /// `ini/3dtexture.ini`: texture id → image path.
    textures: PathTable,
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
        let table = |name: &str| {
            fs.read(&format!("ini/{name}"))
                .map(|b| PathTable::parse(&b))
                .unwrap_or_default()
        };
        let (objects, textures) = (table("3dobj.ini"), table("3dtexture.ini"));
        Ok(Self {
            root,
            fs,
            catalog,
            objects,
            textures,
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
            .objects
            .get(id)
            .ok_or_else(|| format!("model {id} is not in 3dobj.ini"))?;
        let bytes = self
            .fs
            .read(path)
            .ok_or_else(|| format!("file not found: {path}"))?;
        let file = C3File::parse(&bytes).map_err(|e| format!("{path}: {e}"))?;
        let model = model_io::import_model(&file).map_err(|e| format!("{path}: {e}"))?;
        Ok((model, self.textures.get(id).map(str::to_owned)))
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
