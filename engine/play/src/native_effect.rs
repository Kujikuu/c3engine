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
//!
//! [[layers]]           # a glTF model playing its first animation
//! model = "models/ring.gltf"
//! blend = "alpha"
//! ```
//!
//! A flipbook's frames are packed into one square atlas image, `<effect>#atlas<layer>`.

use crate::native::NativeAssets;
use c3_assets::Image;
use c3_assets::effect::{Blend, Effect, EffectLayer, EffectTiming, ParticleFrame, ParticleSystem};
use c3_render::SceneAssets;
use glam::{Mat4, Vec3};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectFile {
    #[serde(default = "default_frame_ms")]
    pub frame_ms: u32,
    #[serde(default = "one")]
    pub loops: u32,
    #[serde(default)]
    pub delay_ms: u32,
    #[serde(default)]
    pub loop_gap_ms: u32,
    #[serde(default)]
    pub layers: Vec<LayerFile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerFile {
    /// A sliced image whose animation plays as a flipbook.
    pub sheet: Option<String>,
    #[serde(default = "all_frames")]
    pub animation: String,
    #[serde(default = "default_size")]
    pub size: f32,
    #[serde(default)]
    pub rise: f32,
    /// A glTF model, instead of a sheet.
    pub model: Option<String>,
    #[serde(default)]
    pub blend: BlendMode,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
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
        (Some(sheet), None) => {
            let frames = flipbook_frames(layer, sheet, assets)?;
            let rows = atlas_rows(frames.len());
            let cells = (rows * rows) as f32;
            let lift = Mat4::from_translation(Vec3::new(0.0, 0.0, -layer.rise));
            Ok(EffectLayer {
                meshes: Vec::new(),
                particles: vec![ParticleSystem {
                    frames: (0..frames.len())
                        .map(|f| ParticleFrame {
                            positions: vec![Vec3::ZERO],
                            ages: vec![(f as f32 + 0.5) / cells],
                            sizes: vec![layer.size / 2.0],
                            transform: lift,
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
