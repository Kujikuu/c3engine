//! TQ effects → engine [`Effect`].
//!
//! An effect is a `3DEffect.ini` section: `Amount` parts, each a C3 effect file
//! (`EffectIdN` in `3DEffectObj.ini`) drawn with one texture (`TextureIdN` in
//! `3dtexture.ini`) and blend (`ASBN`/`ADBN`, D3D blend codes), plus the clock
//! (`Delay`, `LoopTime`, `FrameInterval`, `LoopInterval`) and an offset.
//! Ported from `C3DRoleData` (3DEffect.ini loader), `CGame3DEffectEx` and `C3DEffect`.

use crate::c3::{C3File, ChunkKind, effect};
use crate::ini::Ini;
use crate::io::Result;
use crate::model_io;
use c3_assets::effect::{
    Blend, BlendFactor, EffectLayer, EffectTiming, ParticleFrame, ParticleSystem, Trail,
};
use glam::{Mat4, Vec3};

/// One part of a `3DEffect.ini` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectPartInfo {
    /// Id in `3DEffectObj.ini`.
    pub effect: u64,
    /// Id in `3dtexture.ini`.
    pub texture: u64,
    pub src_blend: u32,
    pub dst_blend: u32,
}

/// A `3DEffect.ini` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectInfo {
    pub parts: Vec<EffectPartInfo>,
    pub timing: EffectTiming,
    /// `OffsetX`, `OffsetY` (along the ground), `OffsetZ` (up).
    pub offset: [i32; 3],
}

/// `3DEffect.ini`: effect name → definition.
#[derive(Debug, Clone, Default)]
pub struct EffectTable {
    ini: Ini,
}

impl EffectTable {
    pub fn parse(data: &[u8]) -> Self {
        Self {
            ini: Ini::parse(data),
        }
    }

    /// Effect `name`; a `+owner` suffix is ignored, as in `CGame3DEffectEx::Add`.
    pub fn get(&self, name: &str) -> Option<EffectInfo> {
        let name = name.split('+').next().unwrap_or(name);
        let s = self.ini.section(name)?;
        let num = |key: &str| s.get_parsed::<i64>(key);
        let parts = (0..num("Amount")?.max(0))
            .filter_map(|i| {
                Some(EffectPartInfo {
                    effect: s.get_parsed(&format!("EffectId{i}"))?,
                    texture: s.get_parsed(&format!("TextureId{i}")).unwrap_or(0),
                    src_blend: s.get_parsed(&format!("ASB{i}")).unwrap_or(5),
                    dst_blend: s.get_parsed(&format!("ADB{i}")).unwrap_or(6),
                })
            })
            .collect();
        let ms = |key: &str| num(key).unwrap_or(0).clamp(0, u32::MAX as i64) as u32;
        Some(EffectInfo {
            parts,
            timing: EffectTiming {
                delay_ms: ms("Delay"),
                loops: ms("LoopTime"),
                frame_ms: ms("FrameInterval"),
                loop_gap_ms: ms("LoopInterval"),
            },
            offset: ["OffsetX", "OffsetY", "OffsetZ"].map(|k| num(k).unwrap_or(0) as i32),
        })
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.ini.sections.iter().map(|s| s.name.as_str())
    }
}

/// Engine blend for D3D blend codes. The classic back buffer has no alpha channel, so
/// destination alpha reads as 1.
pub fn blend_from_d3d(src: u32, dst: u32) -> Blend {
    fn factor(code: u32) -> BlendFactor {
        match code {
            1 | 8 => BlendFactor::Zero,
            2 | 7 => BlendFactor::One,
            3 => BlendFactor::SrcColor,
            4 => BlendFactor::OneMinusSrcColor,
            6 => BlendFactor::OneMinusSrcAlpha,
            9 => BlendFactor::DstColor,
            10 => BlendFactor::OneMinusDstColor,
            _ => BlendFactor::SrcAlpha,
        }
    }
    Blend {
        src: factor(src),
        dst: factor(dst),
    }
}

/// Converts a C3 effect file's meshes, particles and trails into a layer with no texture
/// or blend set. Mesh n pairs with motion n, and trail n with trail motion n.
pub fn import_effect_layer(file: &C3File) -> Result<EffectLayer> {
    let mut particles = Vec::new();
    let mut shapes = Vec::new();
    let mut shape_motions = Vec::new();
    for chunk in &file.chunks {
        match chunk.kind() {
            ChunkKind::Particles => {
                particles.push(particles_from(&effect::Particles::parse(&chunk.body)?))
            }
            ChunkKind::Shape => shapes.push(effect::Shape::parse(&chunk.body)?),
            ChunkKind::ShapeMotion => shape_motions.push(effect::ShapeMotion::parse(&chunk.body)?),
            _ => {}
        }
    }
    let trails = shapes
        .iter()
        .zip(&shape_motions)
        .filter_map(|(s, m)| trail_from(s, m))
        .collect();
    Ok(EffectLayer {
        meshes: model_io::import_model(file)?.parts,
        particles,
        trails,
        texture: None,
        blend: Blend::ALPHA,
    })
}

fn matrix(m: &effect::Matrix) -> Mat4 {
    Mat4::from_cols_array(m)
}

fn particles_from(p: &effect::Particles) -> ParticleSystem {
    ParticleSystem {
        frames: p
            .frames
            .iter()
            .map(|f| ParticleFrame {
                positions: f.positions.iter().copied().map(Vec3::from).collect(),
                ages: f.ages.clone(),
                sizes: f.sizes.clone(),
                transform: f.matrix.as_ref().map_or(Mat4::IDENTITY, matrix),
            })
            .collect(),
        atlas_rows: p.rows.max(1),
    }
}

/// Only the first line's first two points form the edge (`Shape_Draw`); segments are
/// scaled ×5 and capped at 800 (`Shape_SetSegment`).
fn trail_from(s: &effect::Shape, m: &effect::ShapeMotion) -> Option<Trail> {
    let line = s.lines.first()?;
    Some(Trail {
        edge: [Vec3::from(*line.first()?), Vec3::from(*line.get(1)?)],
        frames: m.frames.iter().map(matrix).collect(),
        segments: (s.segments * 5).min(800),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entries_and_blends() {
        let t = EffectTable::parse(
            b"[M_Fire]\r\nAmount=2\r\nEffectId0=1066\r\nTextureId0=1420\r\nASB0=5\r\nADB0=2\r\n\
              EffectId1=7\r\nTextureId1=8\r\nDelay=10\r\nLoopTime=99999999\r\n\
              FrameInterval=33\r\nLoopInterval=0\r\nOffsetX=1\r\nOffsetY=-2\r\nOffsetZ=3\r\n",
        );
        let e = t.get("M_Fire+hero").unwrap();
        assert_eq!(e.parts.len(), 2);
        assert_eq!((e.parts[0].effect, e.parts[0].texture), (1066, 1420));
        assert_eq!((e.parts[1].src_blend, e.parts[1].dst_blend), (5, 6));
        assert_eq!(e.timing.delay_ms, 10);
        assert_eq!(e.timing.loops, 99_999_999);
        assert_eq!(e.offset, [1, -2, 3]);
        assert_eq!(blend_from_d3d(5, 2), Blend::ADDITIVE);
        assert_eq!(blend_from_d3d(5, 7), Blend::ADDITIVE, "no dest alpha");
        assert_eq!(blend_from_d3d(5, 6), Blend::ALPHA);
    }
}
