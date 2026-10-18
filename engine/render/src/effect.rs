//! Effect playback: an [`Effect`] at a moment in time → meshes for the renderer.
//! Ported from the classic client's `C3DEffect::Draw` and its parts:
//!
//! - meshes (`Phy_Calculate`): skinned by their clip, faded by opacity keys, switched by
//!   visibility keys, UVs moved by sprite-sheet keys or scrolling; billboards face the camera;
//! - particles (`Ptcl_Draw`): baked positions drawn as camera-facing quads whose sprite-sheet
//!   cell follows each particle's age;
//! - trails (`Shape_Draw`): a ribbon swept by an edge, smoothed with five in-between edges
//!   per step and stepped every 30 ms.
//!
//! All geometry comes out in world space except meshes, which carry their transform.

use crate::MeshVertex;
use c3_assets::effect::{Effect, EffectLayer, EffectTiming, ParticleSystem, Playback, Trail};
use c3_assets::model::{Billboard, ModelPart};
use glam::{Mat4, Vec2, Vec3};
use std::collections::VecDeque;

/// How often trails record a new step.
const TRAIL_STEP_MS: u64 = 30;
/// In-between edges per trail step (`Shape_SetSegment` smoothing).
pub const TRAIL_SMOOTHING: usize = 5;
/// Distance from the screen plane to the classic camera (`CMyBitmap::GameCameraCreate`).
const CAMERA_DISTANCE: f32 = 1000.0;

/// Effect geometry for one draw.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectMesh {
    /// Index of the [`EffectLayer`] it belongs to (its texture and blend).
    pub layer: usize,
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
    pub opacity: f32,
    /// Model → world.
    pub transform: Mat4,
}

/// Position of the classic camera for a view centred on world point `center`.
pub fn camera_eye(center: (f32, f32)) -> Vec3 {
    Vec3::new(center.0, -CAMERA_DISTANCE, center.1)
}

/// Where a ground effect stands (`CGame3DEffectEx::Show`, map effects): scaled and rotated
/// in place (`rotation` in radians about X, Y, Z), lifted by the effect's up offset and
/// `height`, tilted like actors, then moved to `position` plus the ground offset.
pub fn ground_placement(
    effect: &Effect,
    position: (f32, f32),
    height: f32,
    tilt_deg: f32,
    rotation: Vec3,
    scale: Vec3,
) -> Mat4 {
    let o = effect.offset;
    Mat4::from_translation(Vec3::new(position.0 + o.x, 0.0, position.1 + o.y))
        * Mat4::from_rotation_x(tilt_deg.to_radians())
        * Mat4::from_translation(Vec3::new(0.0, o.z, height * 1.4))
        * Mat4::from_rotation_z(rotation.z)
        * Mat4::from_rotation_y(rotation.y)
        * Mat4::from_rotation_x(rotation.x)
        * Mat4::from_scale(scale)
}

/// A playing effect: its clock and trail histories.
#[derive(Debug, Clone)]
pub struct EffectPlayer {
    started_ms: u64,
    timing: EffectTiming,
    /// Per layer, per trail.
    trails: Vec<Vec<TrailHistory>>,
    last_trail_step: Option<u64>,
}

impl EffectPlayer {
    pub fn new(effect: &Effect, now_ms: u64) -> Self {
        Self {
            started_ms: now_ms,
            timing: effect.timing,
            trails: effect
                .layers
                .iter()
                .map(|l| vec![TrailHistory::default(); l.trails.len()])
                .collect(),
            last_trail_step: None,
        }
    }

    /// Plays forever, whatever the effect's own loop count (classic map effects).
    pub fn endless(mut self) -> Self {
        self.timing = self.timing.endless();
        self
    }

    pub fn playback(&self, effect: &Effect, now_ms: u64) -> Playback {
        self.timing
            .playback(effect.frame_count(), now_ms.saturating_sub(self.started_ms))
    }

    /// Geometry at `now_ms` (empty while hidden or finished). `placement` maps effect space
    /// to world space; `eye` is the camera position (see [`camera_eye`]).
    pub fn meshes(
        &mut self,
        effect: &Effect,
        now_ms: u64,
        placement: Mat4,
        eye: Vec3,
    ) -> Vec<EffectMesh> {
        let Playback::Frame(frame) = self.playback(effect, now_ms) else {
            return Vec::new();
        };
        let commit = self
            .last_trail_step
            .is_none_or(|t| now_ms >= t + TRAIL_STEP_MS);
        if commit {
            self.last_trail_step = Some(now_ms);
        }
        let mut out = Vec::new();
        for (index, layer) in effect.layers.iter().enumerate() {
            // Draw order within a layer: trails, particles, meshes.
            for (trail, history) in layer.trails.iter().zip(&mut self.trails[index]) {
                out.extend(trail_mesh(trail, history, frame, placement, commit, index));
            }
            for particles in &layer.particles {
                out.extend(particle_mesh(particles, frame, placement, index));
            }
            out.extend(layer_meshes(layer, frame, placement, eye, index));
        }
        out
    }
}

fn layer_meshes<'a>(
    layer: &'a EffectLayer,
    frame: u32,
    placement: Mat4,
    eye: Vec3,
    index: usize,
) -> impl Iterator<Item = EffectMesh> + 'a {
    layer
        .meshes
        .iter()
        .filter_map(move |part| effect_mesh(part, frame, placement, eye, index))
}

fn effect_mesh(
    part: &ModelPart,
    frame: u32,
    placement: Mat4,
    eye: Vec3,
    layer: usize,
) -> Option<EffectMesh> {
    let mesh = &part.mesh;
    let frames = part.clip.as_ref().map_or(1, |c| c.frame_count.max(1));
    let key_frame = frame % frames;
    let opacity = mesh.opacity_at(key_frame);
    if !part.visible || !mesh.visible_at(key_frame) || opacity <= 0.0 {
        return None;
    }
    let uv_offset = mesh.uv_offset_at(key_frame, frame);
    let positions = mesh.posed_positions(part.clip.as_ref(), key_frame as f32);
    let vertices = positions
        .iter()
        .zip(&mesh.vertices)
        .map(|(p, v)| MeshVertex {
            position: p.to_array(),
            normal: [0.0; 3],
            uv: (v.uv + uv_offset).to_array(),
        })
        .collect();
    Some(EffectMesh {
        layer,
        vertices,
        indices: mesh.indices.clone(),
        opacity,
        transform: billboard(mesh.billboard, placement, eye),
    })
}

/// Billboards keep only the placement's position and face the camera
/// (`C3DEffect::Draw` BILB/BIB2); `Vertical` keeps the placement's up axis.
fn billboard(kind: Billboard, placement: Mat4, eye: Vec3) -> Mat4 {
    let up = match kind {
        Billboard::None => return placement,
        Billboard::Full => Vec3::NEG_Z,
        Billboard::Vertical => placement.y_axis.truncate(),
    };
    let position = placement.w_axis.truncate();
    let to_camera = (eye - position).normalize_or_zero();
    let right = to_camera.cross(up);
    let forward = up.cross(right);
    Mat4::from_cols(
        right.extend(0.0),
        up.extend(0.0),
        forward.extend(0.0),
        position.extend(1.0),
    )
}

/// Camera-facing particle quads. The classic camera looks along +Y with up = −Z, so the
/// view's right is world X and its up is world −Z.
fn particle_mesh(
    system: &ParticleSystem,
    frame: u32,
    placement: Mat4,
    layer: usize,
) -> Option<EffectMesh> {
    let f = system
        .frames
        .get(frame as usize % system.frames.len().max(1))?;
    if f.positions.is_empty() {
        return None;
    }
    let world = placement * f.transform;
    // Average axis scale (`Ptcl_GetMatrixScale5065`).
    let scale = world.transform_vector3(Vec3::ONE).length() * 0.577_350_26;
    let rows = system.atlas_rows.max(1);
    let cell = 1.0 / rows as f32;
    let mut vertices = Vec::with_capacity(f.positions.len() * 4);
    let mut indices = Vec::with_capacity(f.positions.len() * 6);
    for ((&position, &age), &size) in f.positions.iter().zip(&f.ages).zip(&f.sizes) {
        let p = world.transform_point3(position);
        let s = size * scale;
        let n = (age * (rows * rows) as f32) as u32;
        let uv = Vec2::new((n % rows) as f32, (n / rows) as f32) * cell;
        let base = vertices.len() as u32;
        // (right, up) offsets in view space and their sprite-sheet corner.
        for (dx, dy, du, dv) in [
            (-s, -s, 0.0, cell),
            (s, -s, cell, cell),
            (-s, s, 0.0, 0.0),
            (s, s, cell, 0.0),
        ] {
            vertices.push(MeshVertex {
                position: [p.x + dx, p.y, p.z - dy],
                normal: [0.0; 3],
                uv: [uv.x + du, uv.y + dv],
            });
        }
        indices.extend([0, 1, 2, 2, 1, 3].map(|i| base + i));
    }
    Some(EffectMesh {
        layer,
        vertices,
        indices,
        opacity: 1.0,
        transform: Mat4::IDENTITY,
    })
}

/// Recorded trail edges in world space, newest first.
#[derive(Debug, Clone, Default)]
struct TrailHistory {
    edges: VecDeque<[Vec3; 2]>,
}

impl TrailHistory {
    /// Adds `edge` after the in-between edges leading to it from the newest one, keeping
    /// `segments` quads. The first edge only starts the ribbon.
    fn push(&mut self, edge: [Vec3; 2], segments: usize) {
        if let Some(&last) = self.edges.front() {
            let length = edge[0].distance(edge[1]);
            for n in 1..=TRAIL_SMOOTHING {
                let t = n as f32 / (TRAIL_SMOOTHING + 1) as f32;
                let a = last[0].lerp(edge[0], t);
                let b = last[1].lerp(edge[1], t);
                // In-between edges keep the current edge's length.
                let now = a.distance(b);
                let a = if now > 0.0 {
                    b.lerp(a, length / now)
                } else {
                    a
                };
                self.edges.push_front([a, b]);
            }
        }
        self.edges.push_front(edge);
        self.edges.truncate(segments + 1);
    }

    /// Ribbon quads: u runs 0.95 (newest) → 0.05 (oldest), v is 0 along the edge's first
    /// point and 1 along its second.
    fn mesh(&self, segments: usize) -> (Vec<MeshVertex>, Vec<u32>) {
        let step = 0.9 / segments.max(1) as f32;
        let mut vertices = Vec::with_capacity(self.edges.len() * 2);
        for (k, edge) in self.edges.iter().enumerate() {
            let u = 0.95 - k as f32 * step;
            for (point, v) in edge.iter().zip([0.0, 1.0]) {
                vertices.push(MeshVertex {
                    position: point.to_array(),
                    normal: [0.0; 3],
                    uv: [u, v],
                });
            }
        }
        let indices = (0..self.edges.len().saturating_sub(1) as u32)
            .flat_map(|k| {
                let (new, old) = (k * 2, k * 2 + 2);
                [new, new + 1, old + 1, old, old + 1, new]
            })
            .collect();
        (vertices, indices)
    }
}

fn trail_mesh(
    trail: &Trail,
    history: &mut TrailHistory,
    frame: u32,
    placement: Mat4,
    commit: bool,
    layer: usize,
) -> Option<EffectMesh> {
    let m = placement
        * *trail
            .frames
            .get(frame as usize % trail.frames.len().max(1))?;
    let edge = trail.edge.map(|p| m.transform_point3(p));
    let segments = trail.segments as usize;
    // Between steps, draw a preview extended to the current edge without recording it.
    let mut preview;
    let shown = if commit {
        history.push(edge, segments);
        &*history
    } else {
        preview = history.clone();
        preview.push(edge, segments);
        &preview
    };
    let (vertices, indices) = shown.mesh(segments);
    (!indices.is_empty()).then_some(EffectMesh {
        layer,
        vertices,
        indices,
        opacity: 1.0,
        transform: Mat4::IDENTITY,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trail_smooths_between_steps_and_caps_its_length() {
        let mut h = TrailHistory::default();
        h.push([Vec3::ZERO, Vec3::X], 8);
        assert_eq!(h.edges.len(), 1, "first edge only starts the ribbon");
        h.push([Vec3::Z * 6.0, Vec3::Z * 6.0 + Vec3::X], 8);
        assert_eq!(h.edges.len(), 1 + TRAIL_SMOOTHING + 1);
        assert_eq!(h.edges[3][1], Vec3::new(1.0, 0.0, 3.0), "halfway edge");
        h.push([Vec3::Z * 12.0, Vec3::Z * 12.0 + Vec3::X], 8);
        assert_eq!(h.edges.len(), 9, "segments + 1 edges");
        let (v, i) = h.mesh(8);
        assert_eq!((v.len(), i.len()), (18, 8 * 6));
        assert_eq!(v[0].uv, [0.95, 0.0]);
    }

    #[test]
    fn full_billboard_faces_the_classic_camera() {
        let placement = Mat4::from_translation(Vec3::new(100.0, 0.0, 50.0));
        let eye = camera_eye((100.0, 50.0));
        let m = billboard(Billboard::Full, placement, eye);
        // Model +Y (authored up) points up the screen (−Z); model Z points at the camera.
        assert!(m.transform_vector3(Vec3::Y).abs_diff_eq(Vec3::NEG_Z, 1e-6));
        assert!(m.transform_vector3(Vec3::Z).abs_diff_eq(Vec3::NEG_Y, 1e-6));
        assert!(m.transform_vector3(Vec3::X).abs_diff_eq(Vec3::X, 1e-6));
    }
}
