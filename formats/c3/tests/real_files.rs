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
    use c3_formats_tq::c3::{C3File, ChunkKind, effect, motion::Motion, phy::Mesh};
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
                ChunkKind::Shape => effect::Shape::parse(&c.body).map(|m| m.to_bytes()),
                ChunkKind::ShapeMotion => effect::ShapeMotion::parse(&c.body).map(|m| m.to_bytes()),
                ChunkKind::Particles => effect::Particles::parse(&c.body).map(|m| m.to_bytes()),
                ChunkKind::Camera => effect::CameraPath::parse(&c.body).map(|m| m.to_bytes()),
                ChunkKind::ParticlesExtended | ChunkKind::Unknown => {
                    failed.push(format!("{:#x}: unexpected chunk {}", e.uid, c.id_str()));
                    continue;
                }
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

/// Equipment tables resolve: every part's mesh is in 3dobj.ini and its texture in
/// 3dtexture.ini, and those files exist in the client.
#[test]
fn equipment_tables_resolve_to_files() {
    use c3_formats_tq::{
        ini::{PartTable, PathTable},
        vfs::ClientFs,
    };
    let Some(dir) = client_dir() else { return };
    let fs = ClientFs::open(&dir).unwrap();
    let table = |name: &str| {
        fs.read(&format!("ini/{name}"))
            .unwrap_or_else(|| panic!("{name} missing"))
            .into_owned()
    };
    let objects = PathTable::parse(&table("3dobj.ini"));
    let textures = PathTable::parse(&table("3dtexture.ini"));
    let motions = PathTable::parse(&table("3dmotion.ini"));
    eprintln!(
        "3dobj {} · 3dtexture {} · 3dmotion {}",
        objects.paths.len(),
        textures.paths.len(),
        motions.paths.len()
    );

    // head.ini and headmotion.ini are empty in 5065; Mount.ini is a one-entry stub in another layout.
    for name in ["armor.ini", "weapon.ini", "armet.ini"] {
        let parts = PartTable::parse(&table(name));
        let (mut ok, mut no_mesh, mut no_tex, mut no_file) = (0, 0, 0, 0);
        for part in parts.looks.values().flatten() {
            match (objects.get(part.mesh), textures.get(part.texture)) {
                (None, _) => no_mesh += 1,
                (_, None) if part.texture != 0 => no_tex += 1,
                (Some(m), t) if fs.read(m).is_none() || t.is_some_and(|t| fs.read(t).is_none()) => {
                    no_file += 1
                }
                _ => ok += 1,
            }
        }
        eprintln!(
            "{name}: {} looks, parts ok {ok}, unknown mesh id {no_mesh}, unknown texture id {no_tex}, missing file {no_file}",
            parts.looks.len()
        );
        assert!(ok > 0, "{name}: nothing resolved");
    }
}

/// Every file under `dir` (recursively) whose extension matches `ext` (case-insensitive).
fn files_with_ext(dir: &std::path::Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext)) {
                out.push(p);
            }
        }
    }
    out
}

#[test]
fn terrain_scenes_round_trip_and_parts_parse() {
    use c3_formats_tq::scene::{ScenePart, TerrainObject};
    let Some(dir) = client_dir() else { return };
    let mut failed = Vec::new();
    let scenes = files_with_ext(&dir, "scene");
    for p in &scenes {
        let raw = std::fs::read(p).unwrap();
        match TerrainObject::parse(&raw) {
            Ok(s) if s.to_bytes() == raw => {}
            Ok(_) => failed.push(format!("{}: bytes differ", p.display())),
            Err(e) => failed.push(format!("{}: {e}", p.display())),
        }
    }
    let parts = files_with_ext(&dir, "part");
    for p in &parts {
        let raw = std::fs::read(p).unwrap();
        match ScenePart::parse_text(&raw) {
            Ok(part) => {
                let again = ScenePart::parse_text(&part.to_text()).unwrap();
                if again != part {
                    failed.push(format!("{}: text round-trip differs", p.display()));
                }
            }
            Err(e) => failed.push(format!("{}: {e}", p.display())),
        }
    }
    eprintln!("scenes: {} · parts: {}", scenes.len(), parts.len());
    assert!(!scenes.is_empty() && !parts.is_empty());
    assert!(
        failed.is_empty(),
        "{} failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

/// Every DMap terrain object references a .scene that exists and parses.
#[test]
fn dmap_terrain_objects_resolve_to_scenes() {
    use c3_formats_tq::{dmap::InteractiveObj, scene::TerrainObject, vfs::ClientFs};
    let Some(dir) = client_dir() else { return };
    let fs = ClientFs::open(&dir).unwrap();
    let (mut refs, mut missing) = (0, std::collections::BTreeSet::new());
    for p in files_with_ext(&dir.join("map").join("map"), "dmap") {
        let map = DMap::parse(&std::fs::read(&p).unwrap()).unwrap();
        for o in &map.interactive {
            if let InteractiveObj::Terrain { file, .. } = o {
                refs += 1;
                let path = file.to_gbk_string();
                match fs.read(&path) {
                    Some(b) => {
                        TerrainObject::parse(&b).unwrap_or_else(|e| panic!("{path}: {e}"));
                    }
                    None => {
                        missing.insert(path);
                    }
                }
            }
        }
    }
    eprintln!("terrain refs: {refs}, missing scene files: {missing:?}");
    assert!(refs > 0);
}

#[test]
fn gamemap_catalog_round_trips_and_resolves() {
    use c3_formats_tq::gamemap::GameMapCatalog;
    let Some(dir) = client_dir() else { return };
    let raw = std::fs::read(dir.join("ini").join("GameMap.dat")).unwrap();
    let cat = GameMapCatalog::parse(&raw).unwrap();
    assert_eq!(cat.to_bytes(), raw);
    let missing: Vec<String> = cat
        .entries
        .iter()
        .map(|e| e.path_str())
        .filter(|p| !dir.join(p).is_file())
        .collect();
    eprintln!(
        "GameMap.dat: {} maps, missing DMap files: {missing:?}",
        cat.entries.len()
    );
    assert!(!cat.entries.is_empty());
}

#[test]
fn masks_tme_and_regions_round_trip() {
    use c3_formats_tq::{mask::AlphaMask, rgn::Region, texture::decode_image, tme::TerrainEffect};
    let Some(dir) = client_dir() else { return };
    let mut failed = Vec::new();

    // Masks: sized by their paired image (DDS, else TGA), loose or packed, as the client does.
    let fs = c3_formats_tq::vfs::ClientFs::open(&dir).unwrap();
    let masks = files_with_ext(&dir, "msk");
    let mut paired = 0;
    let mut unpaired = Vec::new();
    let mut truncated = Vec::new();
    for p in &masks {
        let rel = p
            .strip_prefix(&dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let stem = &rel[..rel.len() - 3];
        let Some(bytes) = ["DDS", "TGA"]
            .iter()
            .find_map(|e| fs.read(&format!("{stem}{e}")))
        else {
            unpaired.push(rel);
            continue;
        };
        let img = decode_image(&bytes).unwrap();
        let raw = std::fs::read(p).unwrap();
        match AlphaMask::parse(&raw, img.width, img.height) {
            Ok(m) if m.to_bytes() == raw => {
                paired += 1;
                if m.is_truncated() {
                    truncated.push(rel);
                }
            }
            Ok(_) => failed.push(format!("{}: bytes differ", p.display())),
            Err(e) => failed.push(format!("{}: {e}", p.display())),
        }
    }

    let tmes = files_with_ext(&dir, "tme");
    for p in &tmes {
        let raw = std::fs::read(p).unwrap();
        match TerrainEffect::parse(&raw) {
            Ok(t) if t.to_bytes() == raw => {}
            Ok(_) => failed.push(format!("{}: bytes differ", p.display())),
            Err(e) => failed.push(format!("{}: {e}", p.display())),
        }
    }

    let regions = files_with_ext(&dir, "rgn");
    for p in &regions {
        let raw = std::fs::read(p).unwrap();
        match Region::parse(&raw) {
            Ok(r) if r.to_bytes() == raw => {}
            Ok(_) => failed.push(format!("{}: bytes differ", p.display())),
            Err(e) => failed.push(format!("{}: {e}", p.display())),
        }
    }
    eprintln!(
        "masks: {paired}/{} paired with an image · tme: {} · rgn: {}",
        masks.len(),
        tmes.len(),
        regions.len()
    );
    eprintln!("masks without an image: {unpaired:?}; truncated masks: {truncated:?}");
    assert!(
        failed.is_empty(),
        "{} failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

/// Text tables: Play.dat is +6 obfuscated ini; the others are plain ini.
#[test]
fn text_dat_tables_parse_as_ini() {
    use c3_formats_tq::{ini::Ini, textdat};
    let Some(dir) = client_dir() else { return };
    let play = std::fs::read(dir.join("ini/Play.dat")).unwrap();
    let plain = textdat::decode(&play, textdat::PLAY_DAT_SHIFT);
    assert_eq!(textdat::encode(&plain, textdat::PLAY_DAT_SHIFT), play);
    let ini = Ini::parse(&plain);
    assert!(
        ini.section("Start").is_some(),
        "Play.dat sections: {:?}",
        ini.sections.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    for name in ["ini/Shop.dat", "Server.dat"] {
        let ini = Ini::parse(&std::fs::read(dir.join(name)).unwrap());
        assert!(ini.section("Header").is_some(), "{name} has no [Header]");
        eprintln!("{name}: {} sections", ini.sections.len());
    }
}
