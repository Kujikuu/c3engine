//! Converts a client map (DMap + PUL + ANI) into the engine's native scene format.
//! Usage: import_map <client_dir> <map/map/xxx.DMap> <out.scene.ron>
use c3_formats_tq::{scene_io, vfs::ClientFs};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [client, map, out] = &args[..] else {
        eprintln!("usage: import_map <client_dir> <map/map/xxx.DMap> <out.scene.ron>");
        std::process::exit(2);
    };
    let fs = ClientFs::open(client).expect("client folder");
    let imported = scene_io::import_map(&fs, map).unwrap_or_else(|e| {
        eprintln!("{map}: {e}");
        std::process::exit(1);
    });
    for w in &imported.warnings {
        eprintln!("warning: {w}");
    }
    c3_scene::native::save_scene(&imported.scene, Path::new(out)).expect("write scene");
    println!("{map} -> {out}");
}
