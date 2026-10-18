//! The only module that knows about TQ/Conquer formats. It opens a client folder as an
//! asset source and converts maps and models to engine types. Everything else in the
//! Studio works on `c3_scene` / `c3_assets` types.
//!
//! When more games are supported this becomes one implementation of a format-plugin trait.

use c3_assets::{Image, model::Model};
use c3_formats_tq::{c3::C3File, ini::PathTable, model_io, scene_io, texture, vfs::ClientFs};
use c3_scene::Scene;
use std::path::{Path, PathBuf};

pub use scene_io::{Imported, META_SOURCE_MAP};

/// A game client folder: loose files plus packed archives.
pub struct Client {
    pub root: PathBuf,
    fs: ClientFs,
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
        let fs = ClientFs::open(root).map_err(|e| e.to_string())?;
        let table = |name: &str| {
            fs.read(&format!("ini/{name}"))
                .map(|b| PathTable::parse(&b))
                .unwrap_or_default()
        };
        Ok(Self {
            root: PathBuf::from(root),
            objects: table("3dobj.ini"),
            textures: table("3dtexture.ini"),
            fs,
        })
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
            .fs
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
        scene_io::import_map(&self.fs, path).map_err(|e| format!("{path}: {e}"))
    }

    /// Decodes an image asset (DDS/TGA).
    pub fn load_image(&self, path: &str) -> Option<Image> {
        texture::decode_image(&self.fs.read(path)?).ok()
    }

    /// Exports `scene` as a DMap to the loose client file at `path`. Before the first
    /// overwrite the original is kept as `<file>.bak`; the write is atomic.
    pub fn save_map(&self, scene: &Scene, path: &str) -> Result<PathBuf, String> {
        let file = self.root.join(Path::new(path));
        write_with_backup(&file, &scene_io::export_dmap(scene).to_bytes())?;
        Ok(file)
    }
}

pub fn write_with_backup(file: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut backup = file.as_os_str().to_owned();
    backup.push(".bak");
    let backup = PathBuf::from(backup);
    if file.exists() && !backup.exists() {
        std::fs::copy(file, &backup).map_err(|e| format!("Backup failed: {e}"))?;
    }
    let mut tmp = file.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes).map_err(|e| format!("Save failed: {e}"))?;
    std::fs::rename(&tmp, file).map_err(|e| format!("Save failed: {e}"))
}
