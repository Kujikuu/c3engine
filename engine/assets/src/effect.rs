//! Visual effect asset: layers of animated meshes, baked particles and ribbon trails,
//! played on a frame clock.

use crate::model::ModelPart;
use glam::{Mat4, Vec3};

/// Factor in `result = src · src_factor + dst · dst_factor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlendFactor {
    Zero,
    One,
    SrcColor,
    OneMinusSrcColor,
    SrcAlpha,
    OneMinusSrcAlpha,
    DstColor,
    OneMinusDstColor,
}

/// How drawn pixels combine with what is already on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Blend {
    pub src: BlendFactor,
    pub dst: BlendFactor,
}

impl Blend {
    /// Standard transparency.
    pub const ALPHA: Self = Self {
        src: BlendFactor::SrcAlpha,
        dst: BlendFactor::OneMinusSrcAlpha,
    };
    /// Glow: brightens what is behind.
    pub const ADDITIVE: Self = Self {
        src: BlendFactor::SrcAlpha,
        dst: BlendFactor::One,
    };
}

impl Default for Blend {
    fn default() -> Self {
        Self::ALPHA
    }
}

/// Particles at one frame, in emitter space.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParticleFrame {
    pub positions: Vec<Vec3>,
    /// 0..1 through each particle's life; selects its sprite-sheet cell.
    pub ages: Vec<f32>,
    /// Half the quad's side length.
    pub sizes: Vec<f32>,
    /// Emitter → effect transform.
    pub transform: Mat4,
}

/// Particle system baked to per-frame positions; drawn as camera-facing quads.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleSystem {
    pub frames: Vec<ParticleFrame>,
    /// Rows (and columns) of the sprite-sheet texture.
    pub atlas_rows: u32,
}

/// Ribbon swept behind a moving edge (weapon swings, comet tails).
#[derive(Debug, Clone, PartialEq)]
pub struct Trail {
    /// The edge's two end points, in trail space.
    pub edge: [Vec3; 2],
    /// Trail → effect transform per frame.
    pub frames: Vec<Mat4>,
    /// Ribbon length in segments.
    pub segments: u32,
}

/// One textured, blended part of an effect.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectLayer {
    pub meshes: Vec<ModelPart>,
    pub particles: Vec<ParticleSystem>,
    pub trails: Vec<Trail>,
    /// Texture asset used by every element of the layer.
    pub texture: Option<String>,
    pub blend: Blend,
}

impl EffectLayer {
    /// Longest animation among the layer's elements, in frames.
    pub fn frame_count(&self) -> u32 {
        let meshes = self
            .meshes
            .iter()
            .filter_map(|m| m.clip.as_ref().map(|c| c.frame_count));
        let particles = self.particles.iter().map(|p| p.frames.len() as u32);
        let trails = self.trails.iter().map(|t| t.frames.len() as u32);
        meshes.chain(particles).chain(trails).max().unwrap_or(0)
    }
}

/// When an effect's frames play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectTiming {
    /// Hidden for this long after starting.
    pub delay_ms: u32,
    /// Times the animation plays; `u32::MAX` loops forever.
    pub loops: u32,
    pub frame_ms: u32,
    /// Hidden pause between loops.
    pub loop_gap_ms: u32,
}

impl Default for EffectTiming {
    fn default() -> Self {
        Self {
            delay_ms: 0,
            loops: 1,
            frame_ms: 33,
            loop_gap_ms: 0,
        }
    }
}

/// What an effect shows at some point in time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Playback {
    /// Waiting out the start delay or a gap between loops.
    Hidden,
    Frame(u32),
    Finished,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Effect {
    pub layers: Vec<EffectLayer>,
    pub timing: EffectTiming,
    /// Placement offset: x/y along the ground, z up.
    pub offset: Vec3,
}

impl Effect {
    /// Frames per loop. The first layer sets the clock, as in the classic client.
    pub fn frame_count(&self) -> u32 {
        self.layers.first().map_or(0, EffectLayer::frame_count)
    }

    /// Playback state `elapsed_ms` after the effect started.
    pub fn playback(&self, elapsed_ms: u64) -> Playback {
        self.timing.playback(self.frame_count(), elapsed_ms)
    }
}

impl EffectTiming {
    /// Plays forever.
    pub fn endless(self) -> Self {
        Self {
            loops: u32::MAX,
            ..self
        }
    }

    /// Playback state of a `frames`-long animation `elapsed_ms` after it started.
    /// Port of `CGame3DEffectEx::Process`.
    pub fn playback(&self, frames: u32, elapsed_ms: u64) -> Playback {
        let frames = u64::from(frames);
        if frames == 0 || self.loops == 0 {
            return Playback::Finished;
        }
        let Some(elapsed) = elapsed_ms.checked_sub(u64::from(self.delay_ms)) else {
            return Playback::Hidden;
        };
        if self.frame_ms == 0 {
            return Playback::Hidden;
        }
        let loop_span = u64::from(self.frame_ms) * frames + u64::from(self.loop_gap_ms);
        let total = u64::from(self.loops) * loop_span - u64::from(self.loop_gap_ms);
        if self.loops != u32::MAX && elapsed >= total {
            return Playback::Finished;
        }
        let frame = (elapsed % loop_span) / u64::from(self.frame_ms);
        if frame >= frames {
            Playback::Hidden
        } else {
            Playback::Frame(frame as u32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn effect(frames: usize, timing: EffectTiming) -> Effect {
        Effect {
            layers: vec![EffectLayer {
                meshes: vec![],
                particles: vec![ParticleSystem {
                    frames: vec![ParticleFrame::default(); frames],
                    atlas_rows: 1,
                }],
                trails: vec![],
                texture: None,
                blend: Blend::ALPHA,
            }],
            timing,
            offset: Vec3::ZERO,
        }
    }

    #[test]
    fn plays_delay_frames_gaps_then_finishes() {
        let e = effect(
            4,
            EffectTiming {
                delay_ms: 100,
                loops: 2,
                frame_ms: 10,
                loop_gap_ms: 20,
            },
        );
        assert_eq!(e.playback(50), Playback::Hidden, "delay");
        assert_eq!(e.playback(100), Playback::Frame(0));
        assert_eq!(e.playback(135), Playback::Frame(3));
        assert_eq!(e.playback(145), Playback::Hidden, "gap between loops");
        assert_eq!(e.playback(160), Playback::Frame(0), "second loop");
        assert_eq!(e.playback(200), Playback::Finished);
    }

    #[test]
    fn endless_effects_never_finish() {
        let e = effect(
            3,
            EffectTiming {
                loops: u32::MAX,
                ..Default::default()
            },
        );
        assert_eq!(e.playback(10_000_000_033), Playback::Frame(1));
    }
}
