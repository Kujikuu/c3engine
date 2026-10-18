//! What the engine knows about each file in a project's `assets/`: a stable id and how the file
//! is imported, kept beside it in a sidecar `<file>.meta` (as Unity does). The file itself is
//! used as it is; only the sidecar holds the engine's choices.
//!
//! An image can be a sheet: sliced into a grid of frames, named animations over those frames,
//! and the frames double as a tileset's tiles. A frame of a sheet is addressed as
//! `path#index` wherever an image path is expected.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// The suffix of the sidecar file beside each asset.
pub const META_SUFFIX: &str = ".meta";

/// An asset's id: it stays the same when the file is renamed or moved.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetId(pub String);

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The sidecar of one asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetMeta {
    pub id: AssetId,
    #[serde(default)]
    pub import: Importer,
}

/// How a file becomes something the engine uses, with that kind's settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "importer", rename_all = "snake_case")]
pub enum Importer {
    Image(ImageImport),
    Sound(SoundImport),
    Model(ModelImport),
    /// The engine's own effect file (`.c3fx`).
    Effect,
    /// The engine's own animation set (`.c3char`): a model as one of the game's looks.
    Character,
    Font,
    Data,
    #[default]
    Other,
}

impl Importer {
    /// The importer for a file named `name`, with default settings.
    pub fn for_file(name: &str) -> Self {
        match extension(name).as_str() {
            "png" | "jpg" | "jpeg" | "bmp" | "tga" => Self::Image(ImageImport::default()),
            "wav" | "ogg" | "mp3" => Self::Sound(SoundImport::for_file(name)),
            "gltf" | "glb" => Self::Model(ModelImport::default()),
            "c3fx" => Self::Effect,
            "c3char" => Self::Character,
            "ttf" | "otf" => Self::Font,
            "toml" | "ron" | "json" | "csv" | "txt" | "lua" | "ini" => Self::Data,
            _ => Self::Other,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Image(_) => "Image",
            Self::Sound(_) => "Sound",
            Self::Model(_) => "Model",
            Self::Effect => "Effect",
            Self::Character => "Animation set",
            Self::Font => "Font",
            Self::Data => "Data",
            Self::Other => "File",
        }
    }

    /// What is wrong with the settings, in words.
    pub fn problems(&self) -> Vec<String> {
        match self {
            Self::Image(i) => i.problems(),
            Self::Sound(s) => s.problems(),
            Self::Model(m) => m.problems(),
            Self::Effect | Self::Character | Self::Font | Self::Data | Self::Other => Vec::new(),
        }
    }
}

/// The lower-case extension of `name`.
pub fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map_or(String::new(), |(_, e)| e.to_lowercase())
}

/// How an image is sampled when drawn larger or smaller than it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Filter {
    /// Smooth, for painted art.
    #[default]
    Linear,
    /// Sharp pixels, for pixel art.
    Nearest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageImport {
    pub filter: Filter,
    /// The point of a frame that stands on its cell, as a fraction of the frame (0, 0 is the
    /// top left; the default is the middle of the bottom edge).
    pub pivot: [f32; 2],
    /// Slices the image into a grid of frames (a sprite sheet or a tileset).
    pub grid: Option<Grid>,
    /// Named runs of frames.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub animations: BTreeMap<String, Animation>,
}

impl Default for ImageImport {
    fn default() -> Self {
        Self {
            filter: Filter::Linear,
            pivot: [0.5, 1.0],
            grid: None,
            animations: BTreeMap::new(),
        }
    }
}

impl ImageImport {
    /// How many frames an image `width` × `height` holds (1 when not sliced).
    pub fn frame_count(&self, width: u32, height: u32) -> u32 {
        match &self.grid {
            None => 1,
            Some(g) => {
                let (cols, rows) = g.columns_rows(width, height);
                g.count.map_or(cols * rows, |n| n.min(cols * rows))
            }
        }
    }

    /// The pixel rectangle `[x, y, w, h]` of frame `index` of an image `width` × `height`.
    pub fn frame_rect(&self, index: u32, width: u32, height: u32) -> Option<[u32; 4]> {
        let Some(g) = &self.grid else {
            return (index == 0).then_some([0, 0, width, height]);
        };
        if index >= self.frame_count(width, height) {
            return None;
        }
        let (cols, _) = g.columns_rows(width, height);
        let (col, row) = (index % cols, index / cols);
        let [w, h] = g.cell;
        let [sx, sy] = g.spacing;
        let [ox, oy] = g.offset;
        Some([ox + col * (w + sx), oy + row * (h + sy), w, h])
    }

    fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.pivot.iter().any(|p| !(0.0..=1.0).contains(p)) {
            out.push("the pivot is outside the frame (each part goes from 0 to 1)".to_owned());
        }
        if let Some(g) = &self.grid
            && g.cell.contains(&0)
        {
            out.push("a grid cell needs a width and a height".to_owned());
        }
        for (name, a) in &self.animations {
            if a.frames.is_empty() {
                out.push(format!("animation \"{name}\" has no frames"));
            }
            if a.frame_ms == 0 {
                out.push(format!("animation \"{name}\" needs a frame time"));
            }
        }
        out
    }
}

/// A grid of equal frames, read left to right, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grid {
    /// Size of one frame, in pixels.
    pub cell: [u32; 2],
    /// Pixels between frames.
    pub spacing: [u32; 2],
    /// Pixels before the first frame (left, top).
    pub offset: [u32; 2],
    /// Frames in use, when the last row isn't full.
    pub count: Option<u32>,
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            cell: [32, 32],
            spacing: [0, 0],
            offset: [0, 0],
            count: None,
        }
    }
}

impl Grid {
    /// Columns and rows that fit an image `width` × `height`.
    pub fn columns_rows(&self, width: u32, height: u32) -> (u32, u32) {
        let fit = |size: u32, offset: u32, cell: u32, gap: u32| {
            if cell == 0 || size < offset + cell {
                0
            } else {
                (size - offset + gap) / (cell + gap)
            }
        };
        (
            fit(width, self.offset[0], self.cell[0], self.spacing[0]),
            fit(height, self.offset[1], self.cell[1], self.spacing[1]),
        )
    }
}

/// A run of a sheet's frames played in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Animation {
    pub frames: Vec<u32>,
    #[serde(default = "default_frame_ms")]
    pub frame_ms: u32,
    #[serde(default = "yes")]
    pub looped: bool,
}

fn default_frame_ms() -> u32 {
    100
}

fn yes() -> bool {
    true
}

/// The address of frame `index` of image `path`.
pub fn frame_path(path: &str, index: u32) -> String {
    format!("{path}#{index}")
}

/// Splits `path#index` into the image and the frame (`None` for a whole image).
pub fn split_frame(path: &str) -> (&str, Option<u32>) {
    match path.rsplit_once('#') {
        Some((image, n)) => match n.parse() {
            Ok(index) => (image, Some(index)),
            Err(_) => (path, None),
        },
        None => (path, None),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundImport {
    /// Loudness, 0 (silent) to 1 (as recorded).
    pub volume: f32,
    pub looped: bool,
    /// Played as it is read (music), rather than decoded first (short sounds).
    pub stream: bool,
}

impl Default for SoundImport {
    fn default() -> Self {
        Self {
            volume: 1.0,
            looped: false,
            stream: false,
        }
    }
}

impl SoundImport {
    /// Defaults for `name`: compressed music streams and loops.
    fn for_file(name: &str) -> Self {
        let music = matches!(extension(name).as_str(), "ogg" | "mp3");
        Self {
            looped: music,
            stream: music,
            ..Self::default()
        }
    }

    fn problems(&self) -> Vec<String> {
        if (0.0..=1.0).contains(&self.volume) {
            Vec::new()
        } else {
            vec!["the volume goes from 0 to 1".to_owned()]
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelImport {
    /// World pixels per model unit.
    pub scale: f32,
}

impl Default for ModelImport {
    fn default() -> Self {
        // A 1.8 unit (metre) figure stands about as tall as a classic character.
        Self { scale: 75.0 }
    }
}

impl ModelImport {
    fn problems(&self) -> Vec<String> {
        if self.scale > 0.0 {
            Vec::new()
        } else {
            vec!["the scale must be above 0".to_owned()]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn importers_follow_the_extension() {
        assert!(matches!(
            Importer::for_file("a/Hero.PNG"),
            Importer::Image(_)
        ));
        assert!(
            matches!(Importer::for_file("theme.ogg"), Importer::Sound(s) if s.looped && s.stream)
        );
        assert!(matches!(Importer::for_file("hit.wav"), Importer::Sound(s) if !s.looped));
        assert!(matches!(Importer::for_file("tree.glb"), Importer::Model(_)));
        assert_eq!(Importer::for_file("font.ttf"), Importer::Font);
        assert_eq!(Importer::for_file("readme"), Importer::Other);
    }

    #[test]
    fn a_grid_slices_frames() {
        let mut i = ImageImport {
            grid: Some(Grid {
                cell: [16, 16],
                spacing: [2, 2],
                offset: [1, 1],
                count: None,
            }),
            ..ImageImport::default()
        };
        // 1 + 16 + 2 + 16 + 2 + 16 = 53: three columns; two rows in 35.
        assert_eq!(i.frame_count(53, 35), 6);
        assert_eq!(i.frame_rect(4, 53, 35), Some([19, 19, 16, 16]));
        assert_eq!(i.frame_rect(6, 53, 35), None);
        i.grid.as_mut().unwrap().count = Some(5);
        assert_eq!(i.frame_count(53, 35), 5);
        assert_eq!(
            ImageImport::default().frame_rect(0, 8, 8),
            Some([0, 0, 8, 8])
        );
    }

    #[test]
    fn frame_paths_split() {
        assert_eq!(split_frame(&frame_path("a/b.png", 3)), ("a/b.png", Some(3)));
        assert_eq!(split_frame("a/b.png"), ("a/b.png", None));
        assert_eq!(split_frame("a/#x.png"), ("a/#x.png", None));
    }

    #[test]
    fn problems_are_named() {
        let mut i = ImageImport {
            pivot: [2.0, 0.0],
            ..ImageImport::default()
        };
        i.animations.insert(
            "walk".into(),
            Animation {
                frames: vec![],
                frame_ms: 0,
                looped: true,
            },
        );
        assert_eq!(Importer::Image(i).problems().len(), 3);
    }
}
