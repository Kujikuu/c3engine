//! Renders a map's puzzle ground to PNG: DMap → PUL → ANI → DDS (via WDF).
//! Usage: mapshot <client_dir> <map/map/xxx.DMap> <out.png> [scale_div=4]

use c3_formats_tq::{ani::Ani, dmap::DMap, pul::{Puzzle, EMPTY, GRID_SIZE}, texture::Image, vfs::ClientFs};
use std::collections::HashMap;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, client, map, out, ..] = &args[..] else {
        eprintln!("usage: mapshot <client_dir> <map/map/xxx.DMap> <out.png> [scale_div]");
        std::process::exit(2);
    };
    let div: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(4);
    let fs = ClientFs::open(client).expect("open client");

    let dmap = DMap::parse(&fs.read(map).expect("dmap not found")).expect("parse dmap");
    let pul_path = dmap.puzzle_file.as_str();
    let pul = Puzzle::parse(&fs.read(&pul_path).expect("pul not found")).expect("parse pul");
    let ani_path = pul.ani_file.as_str();
    let ani = Ani::parse(&fs.read(&ani_path).expect("ani not found")).expect("parse ani");
    println!(
        "map {}x{} cells, puzzle {} ({}x{} grids), ani {} ({} sections)",
        dmap.width, dmap.height, pul_path, pul.width, pul.height, ani_path, ani.sections.len()
    );

    let tile = GRID_SIZE / div;
    let (w, h) = (pul.width * tile, pul.height * tile);
    let mut canvas = vec![0u8; (w * h * 4) as usize];
    let mut cache: HashMap<u16, Option<Image>> = HashMap::new();
    let mut missing = 0;
    for gy in 0..pul.height {
        for gx in 0..pul.width {
            let idx = pul.tile(gx, gy);
            if idx == EMPTY {
                continue;
            }
            let img = cache.entry(idx).or_insert_with(|| {
                let Some(frame) = ani.get(&Puzzle::tile_title(idx)).and_then(|s| s.frame_str(0)) else {
                    eprintln!("tile {idx}: no ani section/frame");
                    return None;
                };
                let Some(bytes) = fs.read(&frame) else {
                    eprintln!("tile {idx}: file not found {frame}");
                    return None;
                };
                Image::decode(&bytes).map_err(|e| eprintln!("tile {idx}: {frame}: {e}")).ok()
            });
            let Some(img) = img else { missing += 1; continue };
            // Box-downsample the tile into the canvas.
            for ty in 0..tile {
                for tx in 0..tile {
                    let mut acc = [0u32; 4];
                    let mut n = 0;
                    for sy in 0..div {
                        for sx in 0..div {
                            let (x, y) = (tx * div + sx, ty * div + sy);
                            if x < img.width && y < img.height {
                                let o = ((y * img.width + x) * 4) as usize;
                                for c in 0..4 {
                                    acc[c] += img.rgba[o + c] as u32;
                                }
                                n += 1;
                            }
                        }
                    }
                    if n == 0 {
                        continue;
                    }
                    let o = (((gy * tile + ty) * w + gx * tile + tx) * 4) as usize;
                    for c in 0..3 {
                        canvas[o + c] = (acc[c] / n) as u8;
                    }
                    canvas[o + 3] = 255;
                }
            }
        }
    }
    println!("decoded {} unique tiles, {missing} missing", cache.len());

    let file = std::fs::File::create(out).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&canvas).unwrap();
    println!("wrote {out} ({w}x{h})");
}
