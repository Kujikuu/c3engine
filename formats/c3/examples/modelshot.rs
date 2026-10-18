//! Renders a C3 model posed at several frames to a PNG strip (flat-shaded, z-buffered),
//! to eyeball mesh and skinning correctness.
//! Usage: modelshot <client_dir> <c3 path> <out.png> [frames=4] [yaw=30] [pitch=0] [motion.c3]
//! `motion.c3` replaces each part's embedded motion (character animations live in separate files).

use c3_formats_tq::{c3::C3File, model_io, vfs::ClientFs};
use glam::{Mat3, Vec3};

const CELL: u32 = 320;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, client, path, out, ..] = &args[..] else {
        eprintln!("usage: modelshot <client_dir> <c3 path> <out.png> [frames] [yaw]");
        std::process::exit(2);
    };
    let shots: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(4);
    let yaw: f32 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(30.0);
    let pitch: f32 = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let fs = ClientFs::open(client).expect("open client");
    let file = C3File::parse(&fs.read(path).expect("c3 not found")).expect("parse c3");
    let mut model = model_io::import_model(&file).expect("import");
    if let Some(motion_path) = args.get(7) {
        let motion_file =
            C3File::parse(&fs.read(motion_path).expect("motion not found")).expect("parse motion");
        let clip = model_io::clip_from_motion(&motion_file.motions().expect("motions")[0]);
        println!(
            "motion {motion_path}: {} bones, {} frames, {} keys",
            clip.bone_count,
            clip.frame_count,
            clip.keys.len()
        );
        for p in &mut model.parts {
            p.clip = Some(clip.clone());
        }
    }
    let frames = model
        .parts
        .iter()
        .filter_map(|p| p.clip.as_ref())
        .map(|c| c.frame_count)
        .max()
        .unwrap_or(1);
    println!("{} parts, {} frames", model.parts.len(), frames);
    for p in &model.parts {
        println!(
            "  {}: {} verts, {} tris, texture {}",
            p.mesh.name,
            p.mesh.vertices.len(),
            p.mesh.indices.len() / 3,
            p.mesh.texture
        );
    }

    // Pose every shot first so all share one framing.
    let rot = Mat3::from_rotation_x(pitch.to_radians()) * Mat3::from_rotation_y(yaw.to_radians());
    let posed: Vec<Vec<(Vec<Vec3>, &[u32])>> = (0..shots)
        .map(|s| {
            let frame = s as f32 * frames as f32 / shots as f32;
            model
                .parts
                .iter()
                .map(|p| {
                    let pos = p
                        .mesh
                        .posed_positions(p.clip.as_ref(), frame)
                        .into_iter()
                        .map(|v| rot * v)
                        .collect();
                    (pos, p.mesh.indices.as_slice())
                })
                .collect()
        })
        .collect();
    let all = posed.iter().flatten().flat_map(|(v, _)| v.iter().copied());
    let (min, max) = all.fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(a, b), v| (a.min(v), b.max(v)),
    );
    let size = (max - min).max_element().max(1e-3);
    let scale = (CELL as f32 * 0.9) / size;
    let center = (min + max) / 2.0;

    let (w, h) = (CELL * shots, CELL);
    let mut rgba = vec![24u8; (w * h * 4) as usize];
    let light = Vec3::new(0.4, 0.7, -0.6).normalize();
    for (s, parts) in posed.iter().enumerate() {
        let mut depth = vec![f32::MAX; (CELL * CELL) as usize];
        // Screen: x right, y down (model y is up), z into the screen.
        let to_screen = |v: Vec3| {
            let p = (v - center) * scale;
            Vec3::new(CELL as f32 / 2.0 + p.x, CELL as f32 / 2.0 - p.y, p.z)
        };
        for (pos, indices) in parts {
            for tri in indices.as_chunks::<3>().0 {
                let [a, b, c] = [0, 1, 2].map(|i| pos[tri[i] as usize]);
                let n = (b - a).cross(c - a).normalize_or_zero();
                let shade = (40.0 + 200.0 * n.dot(light).abs()) as u8;
                let [sa, sb, sc] = [a, b, c].map(to_screen);
                raster(sa, sb, sc, |x, y, z| {
                    let i = (y * CELL + x) as usize;
                    if z < depth[i] {
                        depth[i] = z;
                        let o = ((y * w + s as u32 * CELL + x) * 4) as usize;
                        rgba[o..o + 4].copy_from_slice(&[
                            shade,
                            shade,
                            (shade as f32 * 0.9) as u8,
                            255,
                        ]);
                    }
                });
            }
        }
    }
    for px in rgba.as_chunks_mut::<4>().0 {
        px[3] = 255;
    }
    let file = std::fs::File::create(out).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&rgba).unwrap();
    println!("wrote {out}");
}

/// Fills a screen-space triangle, calling `put(x, y, depth)` per covered pixel.
fn raster(a: Vec3, b: Vec3, c: Vec3, mut put: impl FnMut(u32, u32, f32)) {
    let edge = |p: Vec3, q: Vec3, x: f32, y: f32| (q.x - p.x) * (y - p.y) - (q.y - p.y) * (x - p.x);
    let area = edge(a, b, c.x, c.y);
    if area.abs() < 1e-6 {
        return;
    }
    let x0 = a.x.min(b.x).min(c.x).floor().max(0.0) as u32;
    let x1 = a.x.max(b.x).max(c.x).ceil().min(CELL as f32 - 1.0) as u32;
    let y0 = a.y.min(b.y).min(c.y).floor().max(0.0) as u32;
    let y1 = a.y.max(b.y).max(c.y).ceil().min(CELL as f32 - 1.0) as u32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let (w0, w1, w2) = (
                edge(b, c, px, py) / area,
                edge(c, a, px, py) / area,
                edge(a, b, px, py) / area,
            );
            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                put(x, y, w0 * a.z + w1 * b.z + w2 * c.z);
            }
        }
    }
}
