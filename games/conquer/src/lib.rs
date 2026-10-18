//! Conquer game module: everything specific to Conquer Online 5065 on top of the engine.
//! Engine crates must never depend on this.
//!
//! [`Content`] opens a 5065 client folder and hands out engine types (scenes, images).

use c3_assets::Image;
use c3_assets::sprite::{Prefab, SpriteSet};
use c3_formats_tq::{
    ani::Ani, gamemap::GameMapCatalog, scene::TerrainObject, scene_io, sprite_io, texture,
    vfs::ClientFs,
};
use c3_scene::Scene;
use std::path::{Path, PathBuf};

pub use scene_io::Imported;

/// A 5065 client's assets: loose files, packed archives and the map catalog.
pub struct Content {
    pub root: PathBuf,
    fs: ClientFs,
    catalog: GameMapCatalog,
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
        Ok(Self { root, fs, catalog })
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
