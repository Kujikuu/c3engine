//! The engine's own effect format, `.c3fx`: a readable TOML file of layers played together.
//!
//! ```toml
//! frame_ms = 60        # time per frame (the first layer sets how many frames there are)
//! loops = 1            # times it plays; 0 plays forever
//! delay_ms = 0
//! loop_gap_ms = 0
//!
//! [[layers]]           # a flipbook: a sheet's animation on a camera-facing square
//! sheet = "fx/spark.png"
//! animation = "all"
//! size = 64            # world pixels across
//! rise = 30            # world pixels above the ground
//! blend = "additive"   # or "alpha"
//! count = 8            # a burst of 8 of them…
//! spread = 120         # …flying this far out over the animation…
//! climb = 40           # …and up this far
//!
//! [[layers]]           # a glTF model playing its first animation
//! model = "models/ring.gltf"
//! blend = "alpha"
//!
//! [[layers]]           # a ribbon left by an edge circling the centre
//! sheet = "fx/streak.png"
//! trail = { radius = 60, width = 30, turns = 1.0, frames = 24, segments = 10 }
//! ```
//!
//! A flipbook's frames are packed into one square atlas image, `<effect>#atlas<layer>`.

use crate::native::NativeAssets;
use c3_assets::Image;
use c3_assets::effect::{Blend, Effect, EffectLayer, EffectTiming, ParticleFrame, ParticleSystem};
use c3_render::SceneAssets;
use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectFile {
    #[serde(default = "default_frame_ms")]
    pub frame_ms: u32,
    #[serde(default = "one")]
    pub loops: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub delay_ms: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub loop_gap_ms: u32,
    #[serde(default)]
    pub layers: Vec<LayerFile>,
}

impl Default for EffectFile {
    fn default() -> Self {
        Self {
            frame_ms: default_frame_ms(),
            loops: 1,
            delay_ms: 0,
            loop_gap_ms: 0,
            layers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerFile {
    /// A sliced image whose animation plays as a flipbook (or a trail's texture).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<String>,
    #[serde(default = "all_frames")]
    pub animation: String,
    #[serde(default = "default_size")]
    pub size: f32,
    #[serde(default, skip_serializing_if = "is_zero_f")]
    pub rise: f32,
    /// A glTF model, instead of a sheet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub blend: BlendMode,
    /// Copies of the flipbook flying out together (a burst).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub count: u32,
    /// World pixels the burst flies out over the animation.
    #[serde(default, skip_serializing_if = "is_zero_f")]
    pub spread: f32,
    /// World pixels the burst climbs over the animation.
    #[serde(default, skip_serializing_if = "is_zero_f")]
    pub climb: f32,
    /// A ribbon instead of a flipbook, textured with the sheet's first frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trail: Option<TrailFile>,
}

impl Default for LayerFile {
    fn default() -> Self {
        Self {
            sheet: None,
            animation: all_frames(),
            size: default_size(),
            rise: 0.0,
            model: None,
            blend: BlendMode::default(),
            count: 1,
            spread: 0.0,
            climb: 0.0,
            trail: None,
        }
    }
}

/// A ribbon swept by an upright edge circling the effect's centre.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrailFile {
    /// World pixels from the centre.
    pub radius: f32,
    /// The edge's height (the ribbon's width).
    pub width: f32,
    /// Times round over the animation.
    pub turns: f32,
    /// Frames the circling takes.
    pub frames: u32,
    /// How long the ribbon is, in steps (each 30 ms of movement).
    pub segments: u32,
}

impl Default for TrailFile {
    fn default() -> Self {
        Self {
            radius: 60.0,
            width: 30.0,
            turns: 1.0,
            frames: 24,
            segments: 10,
        }
    }
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

fn is_one(v: &u32) -> bool {
    *v == 1
}

fn is_zero_f(v: &f32) -> bool {
    *v == 0.0
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlendMode {
    #[default]
    Additive,
    Alpha,
}

fn default_frame_ms() -> u32 {
    60
}

fn one() -> u32 {
    1
}

fn all_frames() -> String {
    crate::native::ALL_FRAMES.to_owned()
}

fn default_size() -> f32 {
    64.0
}

/// Reads an effect file.
pub fn parse(text: &str) -> Result<EffectFile, String> {
    toml::from_str(text).map_err(|e| e.message().to_owned())
}

/// The text of an effect file.
pub fn write(file: &EffectFile) -> String {
    toml::to_string_pretty(file).unwrap_or_default()
}

/// The atlas image path of flipbook layer `layer` of effect `path`.
pub fn atlas_path(path: &str, layer: usize) -> String {
    format!("{path}#atlas{layer}")
}

/// Splits `effect#atlas<i>` into the effect and the layer.
pub fn split_atlas_path(path: &str) -> Option<(&str, usize)> {
    let (effect, layer) = path.rsplit_once("#atlas")?;
    Some((effect, layer.parse().ok()?))
}

/// Effect `path` (`file` read from it) as the renderer plays it.
pub fn build(file: &EffectFile, path: &str, assets: &NativeAssets) -> Result<Effect, String> {
    if file.layers.is_empty() {
        return Err(format!("{path} has no layers"));
    }
    let layers = file
        .layers
        .iter()
        .enumerate()
        .map(|(i, layer)| build_layer(layer, i, path, assets))
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Effect {
        layers,
        timing: EffectTiming {
            delay_ms: file.delay_ms,
            loops: if file.loops == 0 {
                u32::MAX
            } else {
                file.loops
            },
            frame_ms: file.frame_ms.max(1),
            loop_gap_ms: file.loop_gap_ms,
        },
        offset: Vec3::ZERO,
    })
}

fn build_layer(
    layer: &LayerFile,
    index: usize,
    path: &str,
    assets: &NativeAssets,
) -> Result<EffectLayer, String> {
    let blend = match layer.blend {
        BlendMode::Additive => Blend::ADDITIVE,
        BlendMode::Alpha => Blend::ALPHA,
    };
    match (&layer.sheet, &layer.model) {
        (Some(sheet), None) if layer.trail.is_some() => {
            let frames = flipbook_frames(layer, sheet, assets)?;
            let trail = layer.trail.unwrap_or_default();
            Ok(EffectLayer {
                meshes: Vec::new(),
                particles: Vec::new(),
                trails: vec![orbit_trail(&trail, layer.rise)],
                texture: frames.first().cloned(),
                blend,
            })
        }
        (Some(sheet), None) => {
            let frames = flipbook_frames(layer, sheet, assets)?;
            let rows = atlas_rows(frames.len());
            let cells = (rows * rows) as f32;
            let lift = Mat4::from_translation(Vec3::new(0.0, 0.0, -layer.rise));
            let count = layer.count.max(1);
            let last = (frames.len().max(2) - 1) as f32;
            Ok(EffectLayer {
                meshes: Vec::new(),
                particles: vec![ParticleSystem {
                    frames: (0..frames.len())
                        .map(|f| {
                            let t = f as f32 / last;
                            ParticleFrame {
                                positions: burst(count, layer.spread * t, layer.climb * t),
                                ages: vec![(f as f32 + 0.5) / cells; count as usize],
                                sizes: vec![layer.size / 2.0; count as usize],
                                transform: lift,
                            }
                        })
                        .collect(),
                    atlas_rows: rows,
                }],
                trails: Vec::new(),
                texture: Some(atlas_path(path, index)),
                blend,
            })
        }
        (None, Some(model)) => {
            let imported = assets.model(model)?;
            let texture = imported
                .model
                .parts
                .iter()
                .map(|p| p.mesh.texture.clone())
                .find(|t| !t.is_empty());
            Ok(EffectLayer {
                meshes: imported.model.parts.clone(),
                particles: Vec::new(),
                trails: Vec::new(),
                texture,
                blend,
            })
        }
        _ => Err(format!(
            "layer {} of {path} needs either a sheet or a model",
            index + 1
        )),
    }
}

/// `count` points evenly round a circle of `radius` on the ground, `climb` up (z is down).
fn burst(count: u32, radius: f32, climb: f32) -> Vec<Vec3> {
    (0..count)
        .map(|i| {
            if count == 1 {
                return Vec3::new(0.0, 0.0, -climb);
            }
            let a = i as f32 / count as f32 * std::f32::consts::TAU;
            Vec3::new(a.cos() * radius, a.sin() * radius, -climb)
        })
        .collect()
}

/// An upright edge circling the centre at `trail.radius`, `rise` above the ground.
fn orbit_trail(trail: &TrailFile, rise: f32) -> c3_assets::effect::Trail {
    let half = trail.width / 2.0;
    let frames = trail.frames.max(2);
    c3_assets::effect::Trail {
        edge: [Vec3::new(0.0, 0.0, -half), Vec3::new(0.0, 0.0, half)],
        frames: (0..frames)
            .map(|f| {
                let a = f as f32 / frames as f32 * trail.turns * std::f32::consts::TAU;
                Mat4::from_rotation_z(a)
                    * Mat4::from_translation(Vec3::new(trail.radius, 0.0, -rise - half))
            })
            .collect(),
        // The renderer smooths each step into several quads.
        segments: trail.segments.max(2) * (c3_render::effect::TRAIL_SMOOTHING as u32 + 1),
    }
}

fn flipbook_frames(
    layer: &LayerFile,
    sheet: &str,
    assets: &NativeAssets,
) -> Result<Vec<String>, String> {
    let set = assets
        .sprite_set(sheet)
        .ok_or_else(|| format!("{sheet} isn't one of the project's images"))?;
    let frames = set
        .frames(&layer.animation)
        .ok_or_else(|| format!("{sheet} has no animation \"{}\"", layer.animation))?;
    if frames.is_empty() {
        return Err(format!(
            "animation \"{}\" of {sheet} has no frames",
            layer.animation
        ));
    }
    Ok(frames.to_vec())
}

/// Rows (and columns) of the square atlas holding `frames` frames.
fn atlas_rows(frames: usize) -> u32 {
    (frames as f64).sqrt().ceil().max(1.0) as u32
}

/// The atlas of flipbook layer `layer` of effect `path`: its frames in square cells, centred.
pub fn atlas(path: &str, layer: usize, assets: &NativeAssets) -> Option<Image> {
    let file = parse(&String::from_utf8(assets.read(path)?).ok()?).ok()?;
    let spec = file.layers.get(layer)?;
    let frames: Vec<Image> = flipbook_frames(spec, spec.sheet.as_deref()?, assets)
        .ok()?
        .iter()
        .filter_map(|f| assets.image(f))
        .collect();
    let cell = frames.iter().map(|f| f.width.max(f.height)).max()?;
    let rows = atlas_rows(frames.len());
    let side = cell * rows;
    let mut rgba = vec![0u8; (side * side * 4) as usize];
    for (i, frame) in frames.iter().enumerate() {
        let i = i as u32;
        let x0 = (i % rows) * cell + (cell - frame.width) / 2;
        let y0 = (i / rows) * cell + (cell - frame.height) / 2;
        for row in 0..frame.height {
            let src = (row * frame.width * 4) as usize;
            let dst = (((y0 + row) * side + x0) * 4) as usize;
            let len = (frame.width * 4) as usize;
            rgba[dst..dst + len].copy_from_slice(&frame.rgba[src..src + len]);
        }
    }
    let mut image = Image::new(side, side, rgba);
    image.sharp = frames.iter().any(|f| f.sharp);
    Some(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_files_read_with_defaults() {
        let file = parse(
            r#"
            loops = 0
            [[layers]]
            sheet = "fx/spark.png"
            [[layers]]
            model = "models/ring.gltf"
            blend = "alpha"
            "#,
        )
        .unwrap();
        assert_eq!((file.frame_ms, file.loops), (60, 0));
        assert_eq!(file.layers[0].animation, "all");
        assert_eq!(file.layers[0].blend, BlendMode::Additive);
        assert_eq!(file.layers[1].blend, BlendMode::Alpha);
        assert!(parse("[[layers]]\nshine = 3").is_err());
        // What the editor writes reads back the same, without the defaults.
        let mut burst = file.clone();
        burst.layers[0].count = 8;
        burst.layers[0].spread = 120.0;
        burst.layers[1].trail = Some(TrailFile::default());
        let text = write(&burst);
        assert!(!text.contains("delay_ms") && text.contains("count = 8"));
        assert_eq!(parse(&text).unwrap(), burst);
    }

    #[test]
    fn bursts_ring_out_and_trails_circle() {
        let points = burst(4, 10.0, 5.0);
        assert_eq!(points.len(), 4);
        assert!((points[1].y - 10.0).abs() < 1e-4 && points[1].z == -5.0);
        let trail = orbit_trail(&TrailFile::default(), 0.0);
        assert_eq!(trail.frames.len(), 24);
        assert_eq!(trail.segments, 60);
    }

    #[test]
    fn atlases_fit_their_frames() {
        assert_eq!(atlas_rows(1), 1);
        assert_eq!(atlas_rows(4), 2);
        assert_eq!(atlas_rows(5), 3);
        assert_eq!(
            split_atlas_path(&atlas_path("fx/a.c3fx", 2)),
            Some(("fx/a.c3fx", 2))
        );
    }
}
