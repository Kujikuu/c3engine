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
        assert!(archive.entries.windows(2).all(|w| w[0].uid <= w[1].uid), "{name} index not sorted");
        assert!(wdf::write_preserving(&archive) == raw, "{name} not byte-identical");
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
            if tried >= 2000 { break; }
        }
    }
    eprintln!("path ids: {hits}/{tried} resolved");
    assert!(hits > 0, "no ani path resolved; hash or normalization wrong");
    let _ = hash::path_id("x");
}

#[test]
fn dmaps_round_trip_byte_identical() {
    let Some(dir) = client_dir() else { return };
    let mut ok = 0;
    let mut failed = Vec::new();
    for entry in std::fs::read_dir(dir.join("map").join("map")).unwrap().flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("dmap")) {
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
    assert!(failed.is_empty(), "{} failed:\n{}", failed.len(), failed.join("\n"));
    assert!(ok > 0);
}

#[test]
fn puls_round_trip_and_resolve_ani() {
    use c3_formats_tq::{ani::Ani, pul::{Puzzle, EMPTY}};
    let Some(dir) = client_dir() else { return };
    let mut ok = 0;
    let mut failed = Vec::new();
    for entry in std::fs::read_dir(dir.join("map").join("puzzle")).unwrap().flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pul")) {
            continue;
        }
        let raw = std::fs::read(&path).unwrap();
        let p = match Puzzle::parse(&raw) {
            Ok(p) => p,
            Err(e) => { failed.push(format!("{}: {e}", path.display())); continue; }
        };
        if p.to_bytes() != raw {
            failed.push(format!("{}: bytes differ", path.display()));
            continue;
        }
        // Every used tile must exist in the referenced ANI.
        let ani_path = dir.join(p.ani_file.as_str().replace('\\', "/"));
        if let Ok(ani) = Ani::open(&ani_path) {
            let missing = p.tiles.iter().filter(|&&t| t != EMPTY)
                .filter(|&&t| ani.get(&Puzzle::tile_title(t)).is_none()).count();
            if missing > 0 {
                eprintln!("{}: {missing} tiles missing in {}", path.display(), ani_path.display());
            }
        }
        ok += 1;
    }
    eprintln!("puls ok: {ok}");
    assert!(failed.is_empty(), "{} failed:\n{}", failed.len(), failed.join("\n"));
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
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ani")) {
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
fn ani_frame_paths_exist_in_packages() {
    use c3_formats_tq::ani::Ani;
    let Some(dir) = client_dir() else { return };
    let archives: Vec<_> = ["c3.wdf", "data.wdf"].iter().map(|n| wdf::Wdf::open(dir.join(n)).unwrap()).collect();
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
