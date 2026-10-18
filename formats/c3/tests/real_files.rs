//! Round-trip tests against the real 5065 client. Skipped when the client is absent.
//! Override the location with `C3_CLIENT_DIR`.

use c3_formats_tq::{dmap::DMap, hash, wdf};
use std::path::PathBuf;

fn client_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("C3_CLIENT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"D:\Conquer2\Tools\eosource\client5065"));
    dir.is_dir().then_some(dir).or_else(|| {
        eprintln!("client dir not found, skipping");
        None
    })
}

#[test]
fn wdf_archives_parse_and_round_trip() {
    let Some(dir) = client_dir() else { return };
    for name in ["c3.wdf", "data.wdf"] {
        let path = dir.join(name);
        let raw = std::fs::read(&path).unwrap();
        let archive = wdf::Wdf::parse(raw.clone()).unwrap();
        assert!(!archive.entries.is_empty(), "{name} empty");
        assert!(
            archive.entries.windows(2).all(|w| w[0].uid <= w[1].uid),
            "{name} index not sorted"
        );
        assert!(
            wdf::write_preserving(&archive) == raw,
            "{name} not byte-identical"
        );
        eprintln!("{name}: {} entries", archive.entries.len());
    }
}

#[test]
fn wdf_builder_round_trips_contents() {
    let mut b = wdf::WdfBuilder::default();
    b.add("ini/Test.ini", b"hello".to_vec());
    b.add("c3/mesh/001.c3", vec![1, 2, 3]);
    let archive = wdf::Wdf::parse(b.build()).unwrap();
    assert_eq!(archive.read("INI\\test.ini"), Some(&b"hello"[..]));
    assert_eq!(archive.read("c3/mesh/001.c3"), Some(&[1u8, 2, 3][..]));
    assert_eq!(archive.read("missing"), None);
}

#[test]
fn string_id_resolves_known_client_paths() {
    let Some(dir) = client_dir() else { return };
    let archives: Vec<_> = ["c3.wdf", "data.wdf"]
        .iter()
        .map(|n| wdf::Wdf::open(dir.join(n)).unwrap())
        .collect();
    // Collect loose file names from ani/*.ani that reference packed data paths.
    let ani_dir = dir.join("ani");
    let mut tried = 0;
    let mut hits = 0;
    for entry in std::fs::read_dir(&ani_dir).unwrap().flatten() {
        let text = String::from_utf8_lossy(&std::fs::read(entry.path()).unwrap()).into_owned();
        for line in text.lines() {
            if let Some((_, v)) = line.split_once('=') {
                let v = v.trim();
                if v.contains('/') || v.contains('\\') {
                    tried += 1;
                    if archives.iter().any(|a| a.find(v).is_some()) {
                        hits += 1;
                    }
                }
            }
            if tried >= 2000 {
                break;
            }
        }
    }
    eprintln!("path ids: {hits}/{tried} resolved");
    assert!(
        hits > 0,
        "no ani path resolved; hash or normalization wrong"
    );
    let _ = hash::path_id("x");
}

#[test]
fn dmaps_round_trip_byte_identical() {
    let Some(dir) = client_dir() else { return };
    let mut ok = 0;
    let mut failed = Vec::new();
    for entry in std::fs::read_dir(dir.join("map").join("map"))
        .unwrap()
        .flatten()
    {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("dmap"))
        {
            continue;
        }
        let raw = std::fs::read(&path).unwrap();
        match DMap::parse(&raw) {
            Ok(map) if map.to_bytes() == raw => ok += 1,
            Ok(_) => failed.push(format!("{}: bytes differ", path.display())),
            Err(e) => failed.push(format!("{}: {e}", path.display())),
        }
    }
    eprintln!("dmaps ok: {ok}");
    assert!(
        failed.is_empty(),
        "{} failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
    assert!(ok > 0);
}

#[test]
fn puls_round_trip_and_resolve_ani() {
    use c3_formats_tq::{
        ani::Ani,
        pul::{EMPTY, Puzzle},
    };
    let Some(dir) = client_dir() else { return };
    let mut ok = 0;
    let mut failed = Vec::new();
    for entry in std::fs::read_dir(dir.join("map").join("puzzle"))
        .unwrap()
        .flatten()
    {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pul"))
        {
            continue;
        }
        let raw = std::fs::read(&path).unwrap();
        let p = match Puzzle::parse(&raw) {
            Ok(p) => p,
            Err(e) => {
                failed.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        if p.to_bytes() != raw {
            failed.push(format!("{}: bytes differ", path.display()));
            continue;
        }
        // Every used tile must exist in the referenced ANI.
        let ani_path = dir.join(p.ani_file.as_str().replace('\\', "/"));
        if let Ok(ani) = Ani::open(&ani_path) {
            let missing = p
                .tiles
                .iter()
                .filter(|&&t| t != EMPTY)
                .filter(|&&t| ani.get(&Puzzle::tile_title(t)).is_none())
                .count();
            if missing > 0 {
                eprintln!(
                    "{}: {missing} tiles missing in {}",
                    path.display(),
                    ani_path.display()
                );
            }
        }
        ok += 1;
    }
    eprintln!("puls ok: {ok}");
    assert!(
        failed.is_empty(),
        "{} failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
    assert!(ok > 0);
}

#[test]
fn anis_parse_and_reparse_identically() {
    use c3_formats_tq::ani::Ani;
    let Some(dir) = client_dir() else { return };
    let mut sections = 0;
    let mut files = 0;
    for entry in std::fs::read_dir(dir.join("ani")).unwrap().flatten() {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ani"))
        {
            continue;
        }
        let ani = Ani::open(&path).unwrap();
        let again = Ani::parse(&ani.to_bytes()).unwrap();
        assert_eq!(ani, again, "{}", path.display());
        sections += ani.sections.len();
        files += 1;
    }
    eprintln!("anis: {files} files, {sections} sections");
    assert!(files > 0);
}

#[test]
fn every_packed_texture_decodes() {
    use c3_formats_tq::texture::decode_image;
    let Some(dir) = client_dir() else { return };
    let (mut ok, mut failed) = (0, Vec::new());
    for name in ["c3.wdf", "data.wdf"] {
        let a = wdf::Wdf::open(dir.join(name)).unwrap();
        for e in &a.entries {
            let b = a.entry_bytes(e);
            if !b.starts_with(b"DDS ") {
                continue;
            }
            match decode_image(b) {
                Ok(img) if img.rgba.len() == (img.width * img.height * 4) as usize => ok += 1,
                Ok(_) => failed.push(format!("{name}:{:#x} bad size", e.uid)),
                Err(err) => failed.push(format!("{name}:{:#x} {err}", e.uid)),
            }
        }
    }
    eprintln!("dds decoded: {ok}");
    assert!(
        failed.is_empty(),
        "{} failed, first: {:?}",
        failed.len(),
        failed.first()
    );
}

#[test]
fn ani_frame_paths_exist_in_packages() {
    use c3_formats_tq::ani::Ani;
    let Some(dir) = client_dir() else { return };
    let archives: Vec<_> = ["c3.wdf", "data.wdf"]
        .iter()
        .map(|n| wdf::Wdf::open(dir.join(n)).unwrap())
        .collect();
    let ani = Ani::open(dir.join("ani").join("Common.Ani")).unwrap();
    let (mut found, mut total) = (0, 0);
    for s in &ani.sections {
        for f in &s.frames {
            let p = String::from_utf8_lossy(f);
            total += 1;
            if archives.iter().any(|a| a.find(&p).is_some()) || dir.join(p.as_ref()).is_file() {
                found += 1;
            }
        }
    }
    eprintln!("Common.Ani frames found: {found}/{total}");
    assert!(found * 10 >= total * 9, "too many missing frames");
}

/// DMap with every fixed string canonicalized (bytes after the NUL zeroed).
fn canonical(mut d: DMap) -> DMap {
    use c3_formats_tq::dmap::{InteractiveObj as I, SceneObj as S};
    d.puzzle_file = d.puzzle_file.canonical();
    for o in &mut d.interactive {
        match o {
            I::Cover {
                ani_file,
                ani_title,
                ..
            } => {
                *ani_file = ani_file.canonical();
                *ani_title = ani_title.canonical();
            }
            I::Terrain { file, .. } | I::Sound { file, .. } => *file = file.canonical(),
            I::Effect { index, .. } | I::EffectNew { index, .. } => *index = index.canonical(),
        }
    }
    for l in &mut d.scene_layers {
        for o in &mut l.objects {
            match o {
                S::Scene {
                    ani_file,
                    ani_title,
                    ..
                } => {
                    *ani_file = ani_file.canonical();
                    *ani_title = ani_title.canonical();
                }
                S::Puzzle { file } => *file = file.canonical(),
            }
        }
    }
    d
}

#[test]
fn every_map_imports_to_scene_and_exports_back() {
    use c3_formats_tq::{scene_io, vfs::ClientFs};
    let Some(dir) = client_dir() else { return };
    let fs = ClientFs::open(&dir).unwrap();
    let (mut ok, mut failed) = (0, Vec::new());
    for entry in std::fs::read_dir(dir.join("map").join("map"))
        .unwrap()
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.to_ascii_lowercase().ends_with(".dmap") {
            continue;
        }
        let path = format!("map/map/{name}");
        let scene = match scene_io::import_map(&fs, &path) {
            Ok(imported) => {
                for w in &imported.warnings {
                    eprintln!("{name}: {w}");
                }
                imported.scene
            }
            Err(e) => {
                failed.push(format!("{name}: import: {e}"));
                continue;
            }
        };
        let original = canonical(DMap::parse(&std::fs::read(entry.path()).unwrap()).unwrap());
        if scene_io::export_dmap(&scene).to_bytes() == original.to_bytes() {
            ok += 1;
        } else {
            failed.push(format!("{name}: export differs"));
        }
    }
    eprintln!("scene round-trips: {ok}");
    assert!(
        failed.is_empty(),
        "{} failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

/// Every packed .c3 parses as a container and round-trips; typed PHY/MOTI chunks
/// decode and re-encode to the exact chunk bytes.
#[test]
fn c3_models_round_trip() {
    use c3_formats_tq::c3::{C3File, ChunkKind, motion::Motion, phy::Mesh};
    use std::collections::BTreeMap;
    let Some(dir) = client_dir() else { return };
    let a = wdf::Wdf::open(dir.join("c3.wdf")).unwrap();
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let (mut files, mut failed) = (0, Vec::<String>::new());
    for e in &a.entries {
        let b = a.entry_bytes(e);
        if !b.starts_with(b"MAXFILE") {
            continue;
        }
        files += 1;
        let file = match C3File::parse(b) {
            Ok(f) => f,
            Err(err) => {
                failed.push(format!("{:#x}: container: {err}", e.uid));
                continue;
            }
        };
        if file.to_bytes() != b {
            failed.push(format!("{:#x}: container bytes differ", e.uid));
        }
        for c in &file.chunks {
            *kinds.entry(c.id_str()).or_default() += 1;
            let result = match c.kind() {
                ChunkKind::Phy(layout) => Mesh::parse(&c.body, layout).map(|m| m.to_bytes()),
                ChunkKind::Motion => Motion::parse(&c.body).map(|m| m.to_bytes()),
                _ => continue,
            };
            match result {
                Ok(bytes) if bytes == c.body => {}
                Ok(_) => failed.push(format!("{:#x}: {} re-encode differs", e.uid, c.id_str())),
                Err(err) => failed.push(format!("{:#x}: {}: {err}", e.uid, c.id_str())),
            }
        }
    }
    eprintln!("c3 files: {files}, chunks: {kinds:?}");
    assert!(files > 0);
    assert!(
        failed.is_empty(),
        "{} failed, first 10:\n{}",
        failed.len(),
        failed
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Every packed .c3 converts to an engine model with valid indices, and paired
/// motions pose every vertex to a finite position.
#[test]
fn c3_models_import_to_engine() {
    use c3_formats_tq::{c3::C3File, model_io};
    let Some(dir) = client_dir() else { return };
    let a = wdf::Wdf::open(dir.join("c3.wdf")).unwrap();
    let (mut parts, mut animated, mut failed) = (0, 0, Vec::<String>::new());
    for e in &a.entries {
        let b = a.entry_bytes(e);
        if !b.starts_with(b"MAXFILE") {
            continue;
        }
        let model = model_io::import_model(&C3File::parse(b).unwrap()).unwrap();
        for p in &model.parts {
            parts += 1;
            let n = p.mesh.vertices.len() as u32;
            if p.mesh.indices.iter().any(|&i| i >= n) {
                failed.push(format!("{:#x} {}: index out of range", e.uid, p.mesh.name));
            }
            if let Some(clip) = &p.clip {
                animated += 1;
                let posed = p
                    .mesh
                    .posed_positions(Some(clip), clip.frame_count as f32 / 2.0);
                if posed.iter().any(|v| !v.is_finite()) {
                    failed.push(format!(
                        "{:#x} {}: non-finite posed vertex",
                        e.uid, p.mesh.name
                    ));
                }
            }
        }
    }
    eprintln!("model parts: {parts}, animated: {animated}");
    assert!(
        failed.is_empty(),
        "{} failed, first: {:?}",
        failed.len(),
        failed.first()
    );
}
