//! c3-formats-gltf: glTF 2.0 models (`.gltf`, `.glb`) as engine [`Model`]s.
//!
//! glTF is Y-up in metres; the engine is −Z-up in world pixels, so positions become
//! `(x, z, −y) × scale`. Each triangle primitive becomes a model part. A skinned part keeps
//! its vertices in bind pose with up to two joints each (the two heaviest, renormalized); a
//! rigid part is skinned to its own node, so animated nodes move it too. Animations are sampled
//! at [`FPS`] into one clip per part. Textures are addressed as `<model path>#texture<i>`.

use c3_anim::{Clip, Key};
use c3_assets::Image;
use c3_assets::model::{Billboard, Model, ModelPart, SkinVertex, SkinnedMesh};
use glam::{Mat3, Mat4, Quat, Vec2, Vec3};
use gltf::animation::util::ReadOutputs;
use gltf::animation::{Interpolation, Property};
use std::path::Path;

/// Frames per second animations are sampled at (the engine's model frame rate).
pub const FPS: f32 = 30.0;

/// A model read from a glTF file.
pub struct Imported {
    /// The model, posed by its first animation (when it has any).
    pub model: Model,
    /// Every animation, with one clip per model part.
    pub animations: Vec<Animation>,
    /// The file's images, by index (`#texture<i>`).
    pub images: Vec<Image>,
}

pub struct Animation {
    pub name: String,
    pub clips: Vec<Option<Clip>>,
}

impl Imported {
    /// Poses the model with animation `index` (`None`: the bind pose).
    pub fn pose(&mut self, index: Option<usize>) {
        let clips = index.and_then(|i| self.animations.get(i));
        for (n, part) in self.model.parts.iter_mut().enumerate() {
            part.clip = clips.and_then(|a| a.clips.get(n).cloned().flatten());
        }
    }
}

/// The path a mesh of model `path` names its texture `index` by.
pub fn texture_path(path: &str, index: usize) -> String {
    format!("{path}#texture{index}")
}

/// Splits `model#texture<i>` into the model and the texture index.
pub fn split_texture_path(path: &str) -> Option<(&str, usize)> {
    let (model, index) = path.rsplit_once("#texture")?;
    Some((model, index.parse().ok()?))
}

/// One part's geometry and what drives it.
struct Part {
    mesh: SkinnedMesh,
    driver: Driver,
}

enum Driver {
    /// Joints of skin `skin`, seen from the mesh's node `node`.
    Skin { node: usize, skin: usize },
    /// The part's own node, whose bind transform is `bind`.
    Rigid { node: usize, bind: Mat4 },
}

/// Reads the model at `file`; `path` is its asset path (textures are named after it) and
/// `scale` the world pixels per glTF unit.
pub fn load(file: &Path, path: &str, scale: f32) -> Result<Imported, String> {
    let (doc, buffers, images) = gltf::import(file).map_err(|e| format!("{path}: {e}"))?;
    let axes = axes(scale);
    let rest = local_transforms(&doc, |n| Some(n.transform().decomposed()));
    let bind = globals(&doc, &rest);
    let mut parts = Vec::new();
    for node in doc.nodes() {
        let Some(mesh) = node.mesh() else { continue };
        for (p, primitive) in mesh.primitives().enumerate() {
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let name = match mesh.name() {
                Some(n) => format!("{n}.{p}"),
                None => format!("mesh{}.{p}", mesh.index()),
            };
            let driver = match node.skin() {
                Some(skin) => Driver::Skin {
                    node: node.index(),
                    skin: skin.index(),
                },
                None => Driver::Rigid {
                    node: node.index(),
                    bind: bind[node.index()],
                },
            };
            let to_model = match &driver {
                // Skinned vertices stay in the mesh's space, as the joints expect.
                Driver::Skin { .. } => axes,
                Driver::Rigid { bind, .. } => axes * *bind,
            };
            let mesh = read_primitive(&primitive, &buffers, name, path, to_model)?;
            parts.push(Part { mesh, driver });
        }
    }
    if parts.is_empty() {
        return Err(format!("{path} has no triangle meshes"));
    }
    let skins: Vec<Vec<(usize, Mat4)>> = doc
        .skins()
        .map(|skin| {
            let reader = skin.reader(|b| Some(&buffers[b.index()]));
            let inverse: Vec<Mat4> = reader
                .read_inverse_bind_matrices()
                .map(|m| m.map(|c| Mat4::from_cols_array_2d(&c)).collect())
                .unwrap_or_default();
            skin.joints()
                .enumerate()
                .map(|(j, n)| (n.index(), inverse.get(j).copied().unwrap_or(Mat4::IDENTITY)))
                .collect()
        })
        .collect();
    let animations = doc
        .animations()
        .enumerate()
        .map(|(i, a)| Animation {
            name: a
                .name()
                .map_or_else(|| format!("animation {i}"), str::to_owned),
            clips: sample(&doc, &a, &buffers, &parts, &skins, axes),
        })
        .collect();
    let mut imported = Imported {
        model: Model {
            parts: parts
                .into_iter()
                .map(|p| ModelPart {
                    mesh: p.mesh,
                    clip: None,
                    visible: true,
                })
                .collect(),
        },
        animations,
        images: images.iter().map(to_image).collect(),
    };
    let first = (!imported.animations.is_empty()).then_some(0);
    imported.pose(first);
    Ok(imported)
}

/// glTF units to engine model space: Y-up metres to −Z-up pixels.
fn axes(scale: f32) -> Mat4 {
    Mat4::from_cols(
        glam::Vec4::new(scale, 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, -scale, 0.0),
        glam::Vec4::new(0.0, scale, 0.0, 0.0),
        glam::Vec4::W,
    )
}

type Trs = ([f32; 3], [f32; 4], [f32; 3]);

/// Each node's local transform, as `trs` gives it.
fn local_transforms(doc: &gltf::Document, trs: impl Fn(&gltf::Node) -> Option<Trs>) -> Vec<Mat4> {
    doc.nodes()
        .map(|n| {
            let (t, r, s) = trs(&n).unwrap_or_else(|| n.transform().decomposed());
            Mat4::from_scale_rotation_translation(Vec3::from(s), Quat::from_array(r), Vec3::from(t))
        })
        .collect()
}

/// Each node's transform in the scene, from local transforms `local`.
fn globals(doc: &gltf::Document, local: &[Mat4]) -> Vec<Mat4> {
    let mut out = local.to_vec();
    let mut parent = vec![None; local.len()];
    for n in doc.nodes() {
        for c in n.children() {
            parent[c.index()] = Some(n.index());
        }
    }
    fn global(i: usize, local: &[Mat4], parent: &[Option<usize>], depth: usize) -> Mat4 {
        match parent[i] {
            Some(p) if depth < 256 => global(p, local, parent, depth + 1) * local[i],
            _ => local[i],
        }
    }
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = global(i, local, &parent, 0);
    }
    out
}

fn read_primitive(
    primitive: &gltf::Primitive,
    buffers: &[gltf::buffer::Data],
    name: String,
    path: &str,
    to_model: Mat4,
) -> Result<SkinnedMesh, String> {
    let reader = primitive.reader(|b| Some(&buffers[b.index()]));
    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .ok_or_else(|| format!("{path}: mesh {name} has no positions"))?
        .collect();
    let normals: Option<Vec<[f32; 3]>> = reader.read_normals().map(Iterator::collect);
    let uvs: Option<Vec<[f32; 2]>> = reader.read_tex_coords(0).map(|t| t.into_f32().collect());
    let joints: Option<Vec<[u16; 4]>> = reader.read_joints(0).map(|j| j.into_u16().collect());
    let weights: Option<Vec<[f32; 4]>> = reader.read_weights(0).map(|w| w.into_f32().collect());
    let indices: Vec<u32> = match reader.read_indices() {
        Some(i) => i.into_u32().collect(),
        None => (0..positions.len() as u32).collect(),
    };
    let material = primitive.material();
    let pbr = material.pbr_metallic_roughness();
    let texture = pbr
        .base_color_texture()
        .map(|info| texture_path(path, info.texture().source().index()))
        .unwrap_or_default();
    let color = argb(pbr.base_color_factor());
    let normal_matrix = Mat3::from_mat4(to_model).inverse().transpose();
    let vertices: Vec<SkinVertex> = positions
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let (bones, weights) = match (&joints, &weights) {
                (Some(j), Some(w)) => two_heaviest(j[i], w[i]),
                _ => ([0, 0], [1.0, 0.0]),
            };
            SkinVertex {
                position: to_model.transform_point3(Vec3::from(*p)),
                normal: normals
                    .as_ref()
                    .map(|n| (normal_matrix * Vec3::from(n[i])).normalize_or_zero()),
                uv: uvs.as_ref().map_or(Vec2::ZERO, |u| Vec2::from(u[i])),
                color,
                bones,
                weights,
            }
        })
        .collect();
    let (min, max) = vertices.iter().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), v| (lo.min(v.position), hi.max(v.position)),
    );
    let blended = material.alpha_mode() == gltf::material::AlphaMode::Blend;
    Ok(SkinnedMesh {
        name,
        opaque_index_count: if blended { 0 } else { indices.len() },
        indices,
        texture,
        bounds: (min, max),
        bind_pose: to_model,
        texture_rows: 1,
        uv_scroll: None,
        two_sided: material.double_sided(),
        billboard: Billboard::None,
        opacity_keys: Vec::new(),
        visibility_keys: Vec::new(),
        texture_keys: Vec::new(),
        vertices,
    })
}

/// The two heaviest of four joint influences, weights summing to 1.
fn two_heaviest(joints: [u16; 4], weights: [f32; 4]) -> ([u32; 2], [f32; 2]) {
    let mut order = [0, 1, 2, 3];
    order.sort_by(|&a, &b| weights[b].total_cmp(&weights[a]));
    let (a, b) = (order[0], order[1]);
    let sum = weights[a] + weights[b];
    if sum <= 0.0 {
        return ([u32::from(joints[0]), 0], [1.0, 0.0]);
    }
    (
        [u32::from(joints[a]), u32::from(joints[b])],
        [weights[a] / sum, weights[b] / sum],
    )
}

fn argb(rgba: [f32; 4]) -> u32 {
    let c = rgba.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u32);
    (c[3] << 24) | (c[0] << 16) | (c[1] << 8) | c[2]
}

/// A channel's keys: times and values (translations, rotations or scales as 4 floats).
struct Track {
    node: usize,
    property: Property,
    step: bool,
    times: Vec<f32>,
    values: Vec<[f32; 4]>,
}

impl Track {
    fn at(&self, t: f32) -> Option<[f32; 4]> {
        let next = self.times.partition_point(|&k| k <= t);
        let value = match (next.checked_sub(1), self.times.get(next)) {
            (None, _) => *self.values.first()?,
            (Some(i), None) => *self.values.get(i)?,
            (Some(i), Some(&b)) if !self.step => {
                let a = self.times[i];
                let f = if b > a { (t - a) / (b - a) } else { 0.0 };
                let (va, vb) = (self.values[i], self.values[i + 1]);
                if self.property == Property::Rotation {
                    Quat::from_array(va)
                        .slerp(Quat::from_array(vb), f)
                        .to_array()
                } else {
                    std::array::from_fn(|k| va[k] + (vb[k] - va[k]) * f)
                }
            }
            (Some(i), Some(_)) => self.values[i],
        };
        Some(value)
    }
}

fn tracks(animation: &gltf::Animation, buffers: &[gltf::buffer::Data]) -> Vec<Track> {
    animation
        .channels()
        .filter_map(|channel| {
            let reader = channel.reader(|b| Some(&buffers[b.index()]));
            let times: Vec<f32> = reader.read_inputs()?.collect();
            let mut values: Vec<[f32; 4]> = match reader.read_outputs()? {
                ReadOutputs::Translations(v) => v.map(|[x, y, z]| [x, y, z, 0.0]).collect(),
                ReadOutputs::Scales(v) => v.map(|[x, y, z]| [x, y, z, 0.0]).collect(),
                ReadOutputs::Rotations(v) => v.into_f32().collect(),
                ReadOutputs::MorphTargetWeights(_) => return None,
            };
            let interpolation = channel.sampler().interpolation();
            if interpolation == Interpolation::CubicSpline {
                // In-tangent, value, out-tangent per key: keep the values.
                values = values.chunks(3).filter_map(|c| c.get(1).copied()).collect();
            }
            (values.len() >= times.len() && !times.is_empty()).then(|| Track {
                node: channel.target().node().index(),
                property: channel.target().property(),
                step: interpolation == Interpolation::Step,
                times,
                values,
            })
        })
        .collect()
}

fn sample(
    doc: &gltf::Document,
    animation: &gltf::Animation,
    buffers: &[gltf::buffer::Data],
    parts: &[Part],
    skins: &[Vec<(usize, Mat4)>],
    axes: Mat4,
) -> Vec<Option<Clip>> {
    let tracks = tracks(animation, buffers);
    let duration = tracks
        .iter()
        .filter_map(|t| t.times.last().copied())
        .fold(0.0, f32::max);
    let frames = ((duration * FPS).ceil() as u32 + 1).max(1);
    let to_engine = |m: Mat4| axes * m * axes.inverse();
    let mut keys: Vec<Vec<Key>> = vec![Vec::new(); parts.len()];
    for frame in 0..frames {
        let t = frame as f32 / FPS;
        let local = local_transforms(doc, |n| {
            let (mut tr, mut r, mut s) = n.transform().decomposed();
            for track in tracks.iter().filter(|k| k.node == n.index()) {
                let v = track.at(t)?;
                match track.property {
                    Property::Translation => tr = [v[0], v[1], v[2]],
                    Property::Rotation => r = v,
                    Property::Scale => s = [v[0], v[1], v[2]],
                    Property::MorphTargetWeights => {}
                }
            }
            Some((tr, r, s))
        });
        let global = globals(doc, &local);
        for (part, out) in parts.iter().zip(&mut keys) {
            let bones = match part.driver {
                Driver::Skin { node, skin } => {
                    let mesh_inverse = global[node].inverse();
                    skins[skin]
                        .iter()
                        .map(|(joint, inverse_bind)| {
                            to_engine(mesh_inverse * global[*joint] * *inverse_bind)
                        })
                        .collect()
                }
                Driver::Rigid { node, bind } => vec![to_engine(global[node] * bind.inverse())],
            };
            out.push(Key { frame, bones });
        }
    }
    keys.into_iter()
        .map(|keys| {
            let moves = keys
                .iter()
                .any(|k| k.bones.iter().any(|b| !b.abs_diff_eq(Mat4::IDENTITY, 1e-4)));
            moves.then(|| Clip {
                bone_count: keys.first().map_or(0, |k| k.bones.len()),
                frame_count: frames,
                keys,
            })
        })
        .collect()
}

/// A glTF image as RGBA8.
fn to_image(data: &gltf::image::Data) -> Image {
    use gltf::image::Format;
    let (w, h) = (data.width, data.height);
    let px = &data.pixels;
    let rgba: Vec<u8> = match data.format {
        Format::R8G8B8A8 => px.clone(),
        Format::R8G8B8 => px.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        Format::R8G8 => px
            .chunks(2)
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        Format::R8 => px.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        // 16-bit and float images: shown white rather than guessed.
        _ => vec![255; (w * h * 4) as usize],
    };
    Image::new(w, h, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axes_turn_y_up_into_minus_z_up() {
        let a = axes(10.0);
        assert!(
            a.transform_point3(Vec3::Y)
                .abs_diff_eq(Vec3::new(0.0, 0.0, -10.0), 1e-5)
        );
        assert!(
            a.transform_point3(Vec3::Z)
                .abs_diff_eq(Vec3::new(0.0, 10.0, 0.0), 1e-5)
        );
        // A rotation, not a mirror.
        assert!(Mat3::from_mat4(a).determinant() > 0.0);
    }

    #[test]
    fn the_heaviest_joints_are_kept() {
        assert_eq!(
            two_heaviest([4, 5, 6, 7], [0.1, 0.6, 0.0, 0.3]),
            ([5, 7], [2.0 / 3.0, 1.0 / 3.0])
        );
        assert_eq!(two_heaviest([4, 5, 6, 7], [0.0; 4]), ([4, 0], [1.0, 0.0]));
    }

    #[test]
    fn texture_paths_split() {
        assert_eq!(
            split_texture_path(&texture_path("a/b.glb", 2)),
            Some(("a/b.glb", 2))
        );
        assert_eq!(split_texture_path("a/b.png#3"), None);
    }

    /// A triangle on a node moving 1 unit up over 1 second, written as `.gltf` + `.bin`.
    fn moving_triangle(dir: &Path) -> std::path::PathBuf {
        let mut bin = Vec::new();
        for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0] {
            bin.extend(v.to_le_bytes());
        }
        for v in [0.0f32, 1.0] {
            bin.extend(v.to_le_bytes());
        }
        for v in [0.0f32, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend(v.to_le_bytes());
        }
        std::fs::write(dir.join("tri.bin"), &bin).unwrap();
        let gltf = r#"{
            "asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}],
            "nodes": [{"mesh": 0}],
            "meshes": [{"name": "tri", "primitives": [{"attributes": {"POSITION": 0}}]}],
            "buffers": [{"uri": "tri.bin", "byteLength": 68}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 36},
                {"buffer": 0, "byteOffset": 36, "byteLength": 8},
                {"buffer": 0, "byteOffset": 44, "byteLength": 24}],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                 "min": [0, 0, 0], "max": [1, 0, 1]},
                {"bufferView": 1, "componentType": 5126, "count": 2, "type": "SCALAR",
                 "min": [0], "max": [1]},
                {"bufferView": 2, "componentType": 5126, "count": 2, "type": "VEC3"}],
            "animations": [{"name": "rise", "samplers": [{"input": 1, "output": 2}],
                "channels": [{"sampler": 0, "target": {"node": 0, "path": "translation"}}]}]
        }"#;
        let file = dir.join("tri.gltf");
        std::fs::write(&file, gltf).unwrap();
        file
    }

    #[test]
    fn a_moving_triangle_loads_posed_and_animated() {
        let dir = std::env::temp_dir().join(format!("c3-gltf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let imported = load(&moving_triangle(&dir), "tri.gltf", 10.0).unwrap();
        let part = &imported.model.parts[0];
        // glTF +Z (forward) is engine +Y.
        assert!(
            part.mesh.vertices[2]
                .position
                .abs_diff_eq(Vec3::new(0.0, 10.0, 0.0), 1e-4)
        );
        assert_eq!(imported.animations[0].name, "rise");
        let clip = part.clip.as_ref().expect("the node moves");
        assert_eq!(clip.frame_count, FPS as u32 + 1);
        // One second in, the triangle has risen 1 unit: 10 pixels up, along −Z.
        let risen = clip.sample_bone(0, FPS).transform_point3(Vec3::ZERO);
        assert!(
            risen.abs_diff_eq(Vec3::new(0.0, 0.0, -10.0), 1e-3),
            "{risen}"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tracks_interpolate_and_hold() {
        let track = Track {
            node: 0,
            property: Property::Translation,
            step: false,
            times: vec![0.0, 1.0],
            values: vec![[0.0; 4], [2.0, 0.0, 0.0, 0.0]],
        };
        assert_eq!(track.at(0.5).unwrap()[0], 1.0);
        assert_eq!(track.at(5.0).unwrap()[0], 2.0);
        assert_eq!(track.at(-1.0).unwrap()[0], 0.0);
    }
}
