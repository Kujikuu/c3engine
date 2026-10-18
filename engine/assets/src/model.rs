//! Skinned model asset: meshes, each optionally driven by an animation clip.

use c3_anim::Clip;
use glam::{Mat4, Vec2, Vec3};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinVertex {
    /// Bind-pose position (already transformed by `SkinnedMesh::bind_pose`).
    pub position: Vec3,
    /// `None` when the source had no normals (generate them when needed).
    pub normal: Option<Vec3>,
    pub uv: Vec2,
    /// ARGB.
    pub color: u32,
    pub bones: [u32; 2],
    pub weights: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Billboard {
    None,
    /// Always faces the camera.
    Full,
    /// Rotates around its vertical axis only.
    Vertical,
}

/// A per-frame parameter key (opacity, visibility, texture switch).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamKey<T> {
    pub frame: i32,
    pub value: T,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkinnedMesh {
    pub name: String,
    pub vertices: Vec<SkinVertex>,
    /// Triangle list. Indices before `opaque_index_count` are opaque, the rest alpha-blended.
    pub indices: Vec<u32>,
    pub opaque_index_count: usize,
    /// Texture asset path.
    pub texture: String,
    pub bounds: (Vec3, Vec3),
    /// Bind matrix the vertex positions were transformed by at import (kept for export).
    pub bind_pose: Mat4,
    /// Rows in a sprite-sheet texture (1 = plain texture).
    pub texture_rows: u32,
    /// UV scroll per frame.
    pub uv_scroll: Option<Vec2>,
    pub two_sided: bool,
    pub billboard: Billboard,
    pub opacity_keys: Vec<ParamKey<f32>>,
    pub visibility_keys: Vec<ParamKey<bool>>,
    pub texture_keys: Vec<ParamKey<i32>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelPart {
    pub mesh: SkinnedMesh,
    pub clip: Option<Clip>,
    /// False for socket ("dummy") parts: animated attachment points that are never drawn.
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Model {
    pub parts: Vec<ModelPart>,
}

impl Model {
    pub fn part(&self, name: &str) -> Option<&ModelPart> {
        self.parts
            .iter()
            .find(|p| p.mesh.name.eq_ignore_ascii_case(name))
    }

    /// Replaces part clips with `clips`, matched by part index (a motion file holds one
    /// clip per part). Parts beyond `clips` keep their clip.
    pub fn apply_motion(&mut self, clips: &[Clip]) {
        for (part, clip) in self.parts.iter_mut().zip(clips) {
            part.clip = Some(clip.clone());
        }
    }

    /// Hides parts whose names are in `sockets` (case-insensitive).
    pub fn mark_sockets(&mut self, sockets: &[&str]) {
        for p in &mut self.parts {
            if sockets.iter().any(|s| s.eq_ignore_ascii_case(&p.mesh.name)) {
                p.visible = false;
            }
        }
    }

    /// Longest clip length in frames (0 if static).
    pub fn frame_count(&self) -> u32 {
        self.parts
            .iter()
            .filter_map(|p| p.clip.as_ref())
            .map(|c| c.frame_count)
            .max()
            .unwrap_or(0)
    }

    /// Model-space transform of socket `name` at `frame`: bone 0 of its clip
    /// (C3 `GetVirtualMotion` + `SetVirtualMotion`). Attach a child with `parent * socket`.
    pub fn socket_transform(&self, name: &str, frame: f32) -> Option<Mat4> {
        let clip = self.part(name)?.clip.as_ref()?;
        Some(clip.sample_bone(0, frame))
    }
}

impl SkinnedMesh {
    /// Vertex positions posed by `clip` at `frame`, or the stored positions without a clip.
    pub fn posed_positions(&self, clip: Option<&Clip>, frame: f32) -> Vec<Vec3> {
        match clip {
            Some(c) => {
                let pose = c.sample(frame);
                self.vertices
                    .iter()
                    .map(|v| c3_anim::skin_position(v.position, v.bones, v.weights, &pose))
                    .collect()
            }
            None => self.vertices.iter().map(|v| v.position).collect(),
        }
    }
}
