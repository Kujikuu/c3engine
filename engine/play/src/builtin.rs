//! Art the engine draws itself, so a project made from scratch shows something before it has
//! art of its own: the default ground of new maps and plain figures for roles. Every game's
//! assets answer these paths (they start with `builtin:`).

use crate::character::LookKind;
use c3_assets::Image;
use c3_assets::model::{Billboard, Model, ModelPart, SkinVertex, SkinnedMesh};
use glam::{Mat4, Vec2, Vec3};
use std::f32::consts::TAU;

/// Prefix of the images the engine draws itself.
pub const PREFIX: &str = "builtin:";
/// Ground tile of a new map.
pub const GROUND_IMAGE: &str = "builtin:ground";
/// Size of the ground tile, in pixels.
pub const GROUND_TILE: u32 = 256;

/// The texture of a figure for a role of `kind`.
pub fn figure_image(kind: LookKind) -> &'static str {
    match kind {
        LookKind::Player => "builtin:figure-player",
        LookKind::Monster => "builtin:figure-monster",
        LookKind::Npc | LookKind::Model => "builtin:figure-npc",
    }
}

/// The image the engine draws for `path`, when it is one of its own.
pub fn image(path: &str) -> Option<Image> {
    let name = path.strip_prefix(PREFIX)?;
    let figure = |rgb: [f32; 3]| Some(shading_ramp(rgb));
    match name {
        "ground" => Some(ground_tile()),
        "figure-player" => figure([0.30, 0.55, 0.95]),
        "figure-monster" => figure([0.90, 0.32, 0.28]),
        "figure-npc" => figure([0.95, 0.78, 0.30]),
        _ => None,
    }
}

/// A figure's texture: `u` runs from the shaded to the lit side, `v` from head to feet.
fn shading_ramp(rgb: [f32; 3]) -> Image {
    const SIZE: u32 = 32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let light = 0.45 + 0.55 * x as f32 / (SIZE - 1) as f32;
            let height = 1.0 - 0.25 * y as f32 / (SIZE - 1) as f32;
            for c in rgb {
                rgba.push((c * light * height * 255.0).round().clamp(0.0, 255.0) as u8);
            }
            rgba.push(255);
        }
    }
    Image {
        width: SIZE,
        height: SIZE,
        rgba,
    }
}

/// A soft grass tile that repeats without seams.
fn ground_tile() -> Image {
    let size = GROUND_TILE;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let n = tiling_noise(x, y, size, 32) * 0.6 + tiling_noise(x, y, size, 8) * 0.4;
            let speck = hash(x, y).is_multiple_of(97) as u8 as f32 * 0.08;
            let shade = 0.85 + 0.25 * n + speck;
            for base in [0.27, 0.38, 0.22] {
                rgba.push((base * shade * 255.0).round().clamp(0.0, 255.0) as u8);
            }
            rgba.push(255);
        }
    }
    Image {
        width: size,
        height: size,
        rgba,
    }
}

fn hash(x: u32, y: u32) -> u32 {
    let mut h = x.wrapping_mul(0x27d4_eb2d) ^ y.wrapping_mul(0x1656_67b1);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^ (h >> 13)
}

/// Value noise in 0..1 on a lattice of `cell` pixels, wrapping every `size` pixels.
fn tiling_noise(x: u32, y: u32, size: u32, cell: u32) -> f32 {
    let lattice = size / cell;
    let value = |i: u32, j: u32| (hash(i % lattice, j % lattice) % 1000) as f32 / 999.0;
    let (i, j) = (x / cell, y / cell);
    let (fx, fy) = (
        (x % cell) as f32 / cell as f32,
        (y % cell) as f32 / cell as f32,
    );
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (smooth(fx), smooth(fy));
    let top = value(i, j) * (1.0 - sx) + value(i + 1, j) * sx;
    let low = value(i, j + 1) * (1.0 - sx) + value(i + 1, j + 1) * sx;
    top * (1.0 - sy) + low * sy
}

/// A plain standing figure: a tapered body and a head, feet at the origin, up along −Z (the
/// role transform's vertical), about as tall as a classic character.
pub fn figure() -> Model {
    const SIDES: usize = 10;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    // Rings from the feet up: (height, radius). Body, shoulders, neck, then the head.
    let rings: [(f32, f32); 8] = [
        (0.0, 20.0),
        (-60.0, 17.0),
        (-92.0, 21.0),
        (-100.0, 9.0),
        (-104.0, 13.0),
        (-116.0, 17.0),
        (-130.0, 13.0),
        (-138.0, 4.0),
    ];
    let top = rings[rings.len() - 1].0;
    for &(z, r) in &rings {
        for s in 0..=SIDES {
            let a = s as f32 / SIDES as f32 * TAU;
            let (sin, cos) = a.sin_cos();
            vertices.push(SkinVertex {
                position: Vec3::new(cos * r, sin * r, z),
                normal: None,
                // Lit from the front-left; v runs from the head (0) down to the feet (1).
                uv: Vec2::new(0.5 + 0.5 * (a - 2.4).cos(), z / top),
                color: 0xFFFF_FFFF,
                bones: [0, 0],
                weights: [1.0, 0.0],
            });
        }
    }
    let row = (SIDES + 1) as u32;
    for ring in 0..rings.len() as u32 - 1 {
        for s in 0..SIDES as u32 {
            let a = ring * row + s;
            let b = a + row;
            indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    let mesh = SkinnedMesh {
        name: "figure".into(),
        indices,
        opaque_index_count: 0,
        texture: String::new(),
        bounds: (Vec3::new(-21.0, -21.0, top), Vec3::new(21.0, 21.0, 0.0)),
        bind_pose: Mat4::IDENTITY,
        texture_rows: 1,
        uv_scroll: None,
        two_sided: true,
        billboard: Billboard::None,
        opacity_keys: Vec::new(),
        visibility_keys: Vec::new(),
        texture_keys: Vec::new(),
        vertices,
    };
    let opaque = mesh.indices.len();
    Model {
        parts: vec![ModelPart {
            mesh: SkinnedMesh {
                opaque_index_count: opaque,
                ..mesh
            },
            clip: None,
            visible: true,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_images_draw() {
        for name in ["ground", "figure-player", "figure-monster", "figure-npc"] {
            let image = image(&format!("{PREFIX}{name}")).expect(name);
            assert_eq!(image.rgba.len(), (image.width * image.height * 4) as usize);
        }
        assert!(image("builtin:nothing").is_none());
        assert!(image("data/ground.png").is_none());
    }

    #[test]
    fn the_figure_is_closed_and_stands_on_its_feet() {
        let model = figure();
        let mesh = &model.parts[0].mesh;
        assert!(
            mesh.indices
                .iter()
                .all(|&i| (i as usize) < mesh.vertices.len())
        );
        assert!(mesh.vertices.iter().all(|v| v.position.z <= 0.0));
    }
}
