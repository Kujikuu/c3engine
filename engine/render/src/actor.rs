//! 3D actors (characters, monsters, NPCs) placed in the 2D isometric world, the way
//! the classic C3 client does it (`C3DRole::Draw`, `CMyBitmap::GameCameraCreate`):
//!
//! - camera: orthographic, looking along +Y with up = −Z, so model X → screen x,
//!   model Z → screen y (down), Y → depth; units are world pixels;
//! - role matrix (D3D order): `T(0,0,h) · Rz(facing) · Rx(tilt) · S(scale) · T(x,0,y)`.

use crate::MeshVertex;
use c3_anim::Clip;
use c3_assets::model::SkinnedMesh;
use glam::{Mat4, Vec3, Vec4};

/// Depth range (model Y, world pixels) mapped into clip depth for actors.
const DEPTH_EXTENT: f32 = 4000.0;

/// Where and how an actor is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActorPose {
    /// World pixel position of the feet (usually a cell centre).
    pub position: (f32, f32),
    /// Facing in degrees (classic: `-45 · direction - 45`).
    pub facing_deg: f32,
    /// Camera tilt in degrees (classic `ObliqueAngle`, −45 in 5065's graphic.ini).
    pub tilt_deg: f32,
    pub scale: f32,
    /// Lift above the ground in pixels (jumps, flying).
    pub height: f32,
}

impl ActorPose {
    /// Model → world matrix. glam multiplies right-to-left, so the D3D order reverses.
    pub fn transform(&self) -> Mat4 {
        Mat4::from_translation(Vec3::new(self.position.0, 0.0, self.position.1))
            * Mat4::from_scale(Vec3::splat(self.scale))
            * Mat4::from_rotation_x(self.tilt_deg.to_radians())
            * Mat4::from_rotation_z(self.facing_deg.to_radians())
            * Mat4::from_translation(Vec3::new(0.0, 0.0, self.height))
    }
}

/// World → clip for actors under a 2D camera showing world rect `[l, t, r, b]`.
pub fn actor_view(rect: [f32; 4]) -> Mat4 {
    let [l, t, r, b] = rect;
    let (w, h) = (r - l, b - t);
    Mat4::from_cols(
        Vec4::new(2.0 / w, 0.0, 0.0, 0.0),              // model X → clip x
        Vec4::new(0.0, 0.0, 1.0 / DEPTH_EXTENT, 0.0),   // model Y → depth
        Vec4::new(0.0, -2.0 / h, 0.0, 0.0),             // model Z → clip y (down)
        Vec4::new(-(r + l) / w, (b + t) / h, 0.5, 1.0), // centre depth at 0.5
    )
}

/// Vertices of `mesh` posed by `clip` at `frame`, with smooth normals rebuilt from the
/// posed triangles.
pub fn posed_vertices(mesh: &SkinnedMesh, clip: Option<&Clip>, frame: f32) -> Vec<MeshVertex> {
    let positions = mesh.posed_positions(clip, frame);
    let mut normals = vec![Vec3::ZERO; positions.len()];
    for tri in mesh.indices.as_chunks::<3>().0 {
        let [a, b, c] = tri.map(|i| i as usize);
        if a.max(b).max(c) >= positions.len() {
            continue;
        }
        let n = (positions[b] - positions[a]).cross(positions[c] - positions[a]);
        for i in [a, b, c] {
            normals[i] += n;
        }
    }
    positions
        .iter()
        .zip(&normals)
        .zip(&mesh.vertices)
        .map(|((p, n), v)| MeshVertex {
            position: p.to_array(),
            normal: n.normalize_or_zero().to_array(),
            uv: v.uv.to_array(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_maps_x_and_z_to_screen() {
        let view = actor_view([0.0, 0.0, 800.0, 600.0]);
        let clip = view * Vec4::new(800.0, 0.0, 600.0, 1.0);
        assert!(
            (clip.x - 1.0).abs() < 1e-5 && (clip.y + 1.0).abs() < 1e-5,
            "{clip}"
        );
        assert!((clip.z - 0.5).abs() < 1e-5);
    }

    #[test]
    fn upright_model_stands_on_its_position() {
        // A point 100 units "up" (−Z) with no tilt lands 100 px above the feet.
        let pose = ActorPose {
            position: (400.0, 300.0),
            facing_deg: 0.0,
            tilt_deg: 0.0,
            scale: 1.0,
            height: 0.0,
        };
        let p = pose
            .transform()
            .transform_point3(Vec3::new(0.0, 0.0, -100.0));
        assert_eq!((p.x, p.z), (400.0, 200.0));
    }
}
