//! The project's own assets, the Unity way: plain images (PNG, JPG, BMP, TGA), glTF models,
//! sounds and other files in `assets/`, used as they are and read through their sidecars
//! ([`c3_core::asset::AssetMeta`]). An image sliced into a grid is a sprite sheet: its frames are
//! images of their own (`path#index`), its animations a sprite set, its frames a tileset. A
//! model's textures are images named `model#texture<i>`.

use c3_assets::Image;
use c3_assets::effect::Effect;
use c3_assets::sprite::{Prefab, SpriteSet};
use c3_core::asset::{Filter, ImageImport, Importer, frame_path, split_frame};
use c3_formats_gltf::Imported;
use c3_project::{AssetDb, Project};
use c3_render::SceneAssets;
use c3_scene::{TileLayer, Tileset};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

/// The animation of every image's sprite set that plays all its frames in order.
pub const ALL_FRAMES: &str = "all";

/// The project's assets and their import settings.
pub struct NativeAssets {
    db: RefCell<AssetDb>,
    /// Decoded images by file (`None`: not an image, or unreadable).
    decoded: RefCell<HashMap<String, Option<Rc<Image>>>>,
    /// Read models by file, or why they couldn't be read.
    models: RefCell<HashMap<String, Result<Rc<Imported>, String>>>,
}

impl NativeAssets {
    /// The assets of `project`, and what was wrong with their sidecars.
    pub fn new(project: &Project) -> (Self, Vec<String>) {
        let (db, problems) = AssetDb::of(project);
        (Self::with_db(db), problems)
    }

    pub fn with_db(db: AssetDb) -> Self {
        Self {
            db: RefCell::new(db),
            decoded: RefCell::new(HashMap::new()),
            models: RefCell::new(HashMap::new()),
        }
    }

    /// Reads asset `path` again (its file or sidecar changed on disk).
    pub fn reload(&self, path: &str) -> Result<(), String> {
        let file = path
            .strip_suffix(c3_core::asset::META_SUFFIX)
            .unwrap_or(path);
        self.decoded.borrow_mut().remove(file);
        self.models.borrow_mut().remove(file);
        self.db.borrow_mut().refresh(file)
    }

    /// Reads every asset again.
    pub fn reload_all(&self) -> Vec<String> {
        self.decoded.borrow_mut().clear();
        self.models.borrow_mut().clear();
        self.db.borrow_mut().rescan()
    }

    /// How image `path` is imported, when it is one of the project's images.
    pub fn image_import(&self, path: &str) -> Option<ImageImport> {
        match self.db.borrow().importer(path)? {
            Importer::Image(i) => Some(i.clone()),
            _ => None,
        }
    }

    /// Model `path` (glTF), read with its import settings.
    pub fn model(&self, path: &str) -> Result<Rc<Imported>, String> {
        if let Some(cached) = self.models.borrow().get(path) {
            return cached.clone();
        }
        let read = {
            let db = self.db.borrow();
            match db.importer(path) {
                Some(Importer::Model(m)) => {
                    c3_formats_gltf::load(&db.file(path), path, m.scale).map(Rc::new)
                }
                Some(_) => Err(format!("{path} isn't imported as a model")),
                None => Err(format!("the project has no asset \"{path}\"")),
            }
        };
        self.models
            .borrow_mut()
            .insert(path.to_owned(), read.clone());
        read
    }

    /// The bytes of file `path` of the assets.
    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        let db = self.db.borrow();
        db.meta(path)?;
        std::fs::read(db.file(path)).ok()
    }

    /// Size in pixels of image `path` (a whole image, or one of its frames).
    pub fn size(&self, path: &str) -> Option<[u32; 2]> {
        let (file, frame) = split_frame(path);
        let image = self.decode(file)?;
        match frame {
            None => Some([image.width, image.height]),
            Some(i) => {
                let rect = self
                    .image_import(file)?
                    .frame_rect(i, image.width, image.height)?;
                Some([rect[2], rect[3]])
            }
        }
    }

    /// The frames of image `path` as a tileset, tile index = frame index.
    pub fn tileset(&self, path: &str) -> Option<Tileset> {
        let import = self.image_import(path)?;
        let image = self.decode(path)?;
        let count = import.frame_count(image.width, image.height);
        let count = u16::try_from(count).unwrap_or(u16::MAX);
        Some(Tileset {
            source: path.to_owned(),
            frames: (0..count)
                .map(|i| (i, frame_path(path, i.into())))
                .collect(),
        })
    }

    fn decode(&self, file: &str) -> Option<Rc<Image>> {
        if let Some(cached) = self.decoded.borrow().get(file) {
            return cached.clone();
        }
        let image = self
            .image_import(file)
            .and_then(|_| self.read(file))
            .and_then(|bytes| decode_image(&bytes))
            .map(Rc::new);
        self.decoded
            .borrow_mut()
            .insert(file.to_owned(), image.clone());
        image
    }
}

impl SceneAssets for NativeAssets {
    fn sprite_set(&self, path: &str) -> Option<SpriteSet> {
        let import = self.image_import(path)?;
        let image = self.decode(path)?;
        let count = import.frame_count(image.width, image.height);
        let mut animations: BTreeMap<String, Vec<String>> = import
            .animations
            .iter()
            .map(|(name, a)| {
                let frames = a.frames.iter().filter(|&&f| f < count);
                (name.clone(), frames.map(|&f| frame_path(path, f)).collect())
            })
            .collect();
        let all = if import.grid.is_some() {
            (0..count).map(|f| frame_path(path, f)).collect()
        } else {
            vec![path.to_owned()]
        };
        animations.entry(ALL_FRAMES.to_owned()).or_insert(all);
        Some(SpriteSet { animations })
    }

    fn prefab(&self, _path: &str) -> Option<Prefab> {
        None
    }

    fn tile_layer(&self, _path: &str) -> Option<TileLayer> {
        None
    }

    fn image(&self, path: &str) -> Option<Image> {
        if let Some((model, index)) = c3_formats_gltf::split_texture_path(path) {
            return self.model(model).ok()?.images.get(index).cloned();
        }
        let (file, frame) = split_frame(path);
        let image = self.decode(file)?;
        let import = self.image_import(file)?;
        let mut out = match frame {
            None => (*image).clone(),
            Some(index) => {
                let [x, y, w, h] = import.frame_rect(index, image.width, image.height)?;
                crop(&image, x, y, w, h)
            }
        };
        out.sharp = import.filter == Filter::Nearest;
        Some(out)
    }

    fn effect(&self, name: &str) -> Result<Effect, String> {
        Err(format!("the project has no effect \"{name}\""))
    }
}

/// The part of `image` at `x, y`, `w` × `h` (clipped to the image).
fn crop(image: &Image, x: u32, y: u32, w: u32, h: u32) -> Image {
    let w = w.min(image.width.saturating_sub(x));
    let h = h.min(image.height.saturating_sub(y));
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for row in y..y + h {
        let start = ((row * image.width + x) * 4) as usize;
        rgba.extend_from_slice(&image.rgba[start..start + (w * 4) as usize]);
    }
    Image::new(w, h, rgba)
}

/// Decodes a PNG, JPG, BMP or TGA image.
pub fn decode_image(bytes: &[u8]) -> Option<Image> {
    let rgba = image::load_from_memory(bytes).ok()?.to_rgba8();
    Some(Image::new(rgba.width(), rgba.height(), rgba.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_core::asset::{Animation, Grid};

    #[test]
    fn sheets_slice_into_frames_and_animations() {
        let root = std::env::temp_dir().join(format!("c3-native-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // 4 × 2 frames of 8 × 8; frame n is filled with grey level n.
        let mut sheet = image::RgbaImage::new(32, 16);
        for (x, y, p) in sheet.enumerate_pixels_mut() {
            let n = (y / 8 * 4 + x / 8) as u8;
            *p = image::Rgba([n, n, n, 255]);
        }
        sheet.save(root.join("hero.png")).unwrap();
        let (mut db, _) = AssetDb::open(&root);
        let mut import = ImageImport {
            grid: Some(Grid {
                cell: [8, 8],
                count: Some(7),
                ..Grid::default()
            }),
            ..ImageImport::default()
        };
        import.animations.insert(
            "walk".into(),
            Animation {
                frames: vec![4, 5, 6, 7],
                frame_ms: 100,
                looped: true,
            },
        );
        db.set_importer("hero.png", Importer::Image(import))
            .unwrap();
        let assets = NativeAssets::with_db(db);
        let frame = assets.image("hero.png#5").unwrap();
        assert_eq!((frame.width, frame.height, frame.rgba[0]), (8, 8, 5));
        assert!(assets.image("hero.png#7").is_none());
        assert_eq!(assets.size("hero.png"), Some([32, 16]));
        let set = assets.sprite_set("hero.png").unwrap();
        // Frame 7 is past the count.
        assert_eq!(set.frames("walk").unwrap().len(), 3);
        assert_eq!(set.frames(ALL_FRAMES).unwrap().len(), 7);
        assert_eq!(assets.tileset("hero.png").unwrap().frames.len(), 7);
        assert!(assets.image("missing.png").is_none());
        let _ = std::fs::remove_dir_all(root);
    }
}
