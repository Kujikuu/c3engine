//! TQ C3 → engine [`Model`] / [`Clip`] conversion.
//!
//! Mesh n of a file is animated by motion n of the same file (client `C3DEffect::Load`).
//! Character motions usually live in separate files; convert those with [`clip_from_motion`].
//! Export back to C3 lands with the Blender pipeline.

use crate::c3::{C3File, motion::Motion, phy};
use crate::io::Result;
use c3_anim::{Clip, Key};
use c3_assets::model::{Billboard, Model, ModelPart, ParamKey, SkinVertex, SkinnedMesh};
use glam::{Mat4, Vec2, Vec3};

/// Converts every mesh in a C3 file, pairing it with the motion at the same index.
pub fn import_model(file: &C3File) -> Result<Model> {
    let meshes = file.meshes()?;
    let mut motions = file.motions()?.into_iter();
    let parts = meshes
        .iter()
        .map(|m| ModelPart {
            mesh: mesh_from_phy(m),
            clip: motions.next().as_ref().map(clip_from_motion),
        })
        .collect();
    Ok(Model { parts })
}

pub fn clip_from_motion(m: &Motion) -> Clip {
    let mut keys: Vec<Key> = m
        .key_matrices()
        .into_iter()
        .map(|(frame, bones)| Key {
            frame,
            bones: bones.iter().map(Mat4::from_cols_array).collect(),
        })
        .collect();
    keys.sort_by_key(|k| k.frame);
    Clip {
        bone_count: m.bones as usize,
        frame_count: m.frames,
        keys,
    }
}

/// Positions are pre-multiplied by the bind matrix, as the client does at load
/// (`PhySeedBindPose5065`); motions animate those bound positions.
pub fn mesh_from_phy(p: &phy::Mesh) -> SkinnedMesh {
    let text = |b: &[u8]| {
        encoding_rs::GBK
            .decode_without_bom_handling(b)
            .0
            .into_owned()
    };
    let bind = Mat4::from_cols_array(&p.bind_matrix);
    SkinnedMesh {
        name: text(&p.name),
        vertices: p
            .vertices
            .iter()
            .map(|v| SkinVertex {
                position: bind.project_point3(Vec3::from(v.pos)),
                normal: v.normal.map(Vec3::from),
                uv: Vec2::from(v.uv),
                color: v.color,
                bones: v.bones,
                weights: v.weights,
            })
            .collect(),
        indices: p.indices.iter().map(|&i| u32::from(i)).collect(),
        opaque_index_count: p.opaque_triangles as usize * 3,
        texture: text(&p.texture),
        bounds: (Vec3::from(p.bbox.0), Vec3::from(p.bbox.1)),
        bind_pose: Mat4::from_cols_array(&p.bind_matrix),
        texture_rows: p.tex_row,
        uv_scroll: p.uv_step.map(Vec2::from),
        two_sided: p.two_sided,
        billboard: match p.billboard {
            phy::Billboard::None => Billboard::None,
            phy::Billboard::Full => Billboard::Full,
            phy::Billboard::Vertical => Billboard::Vertical,
        },
        opacity_keys: p
            .alpha_keys
            .iter()
            .map(|k| ParamKey {
                frame: k.frame,
                value: k.float,
            })
            .collect(),
        visibility_keys: p
            .draw_keys
            .iter()
            .map(|k| ParamKey {
                frame: k.frame,
                value: k.flag != 0,
            })
            .collect(),
        texture_keys: p
            .texture_keys
            .iter()
            .map(|k| ParamKey {
                frame: k.frame,
                value: k.int,
            })
            .collect(),
    }
}
