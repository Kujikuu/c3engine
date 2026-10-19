//! Conquer's maps by their key and what a new player starts with.

use crate::Content;
use crate::character::DEFAULT_HERO;
use c3_core::settings::NewCharacter;
use c3_formats_tq::scene_io;
use c3_project::Project;
use c3_scene::{Scene, TileLayer};
use std::path::{Path, PathBuf};

/// Money a new character starts with, to try shops.
pub const START_MONEY: u64 = 1000;

/// Loads map `key` of `project`: one of its engine scenes by name, or a content map file
/// with the population the project keeps for it.
pub fn load_map(project: &Project, content: &Content, key: &str) -> Result<Scene, String> {
    if let Some(scene) = c3_play::load_project_scene(project, key) {
        return scene;
    }
    let mut scene = content.load_map(key)?.scene;
    let population = c3_scene::native::load_population(&project.population_file(map_stem(key)))
        .map_err(|e| e.to_string())?;
    scene.set_population(population);
    Ok(scene)
}

/// The name a client map's population file goes by: its file name without extension.
fn map_stem(key: &str) -> &str {
    let stem = key.rsplit(['/', '\\']).next().unwrap_or(key);
    stem.rsplit_once('.').map_or(stem, |(s, _)| s)
}

/// Writes map `key` of `project`: a project scene, or a client map (its DMap, its ground
/// when `ground` changed, and its population beside the project's maps). Content used in
/// place belongs to the original client: its files keep a `.bak` of the original.
pub fn save_map(
    project: &Project,
    key: &str,
    scene: &Scene,
    ground: Option<&TileLayer>,
) -> Result<(), String> {
    if let Some(saved) = c3_play::module::save_project_scene(project, key, scene) {
        return saved;
    }
    let (_, root) = project
        .tq_client()
        .ok_or("the project has no client content")?;
    let backup = !project.content_in_project();
    let file = root.join(Path::new(key));
    write_map_file(&file, &scene_io::export_dmap(scene).to_bytes(), backup)?;
    if let Some(ground) = ground {
        let file = root.join(Path::new(&ground.source.replace('\\', "/")));
        write_map_file(&file, &scene_io::export_puzzle(ground).to_bytes(), backup)?;
    }
    let file = project.population_file(map_stem(key));
    c3_scene::native::save_population(&scene.population(), &file).map_err(|e| e.to_string())
}

/// Writes `bytes` to `file` atomically, first copying the original to `file.bak` when
/// `backup` (once: the first original is kept).
pub fn write_map_file(file: &Path, bytes: &[u8], backup: bool) -> Result<(), String> {
    let with = |suffix: &str| {
        let mut name = file.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name)
    };
    let bak = with(".bak");
    if backup && file.exists() && !bak.exists() {
        std::fs::copy(file, &bak).map_err(|e| format!("Backup failed: {e}"))?;
    }
    let tmp = with(".tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("Save failed: {e}"))?;
    std::fs::rename(&tmp, file).map_err(|e| format!("Save failed: {e}"))
}

/// What a new Conquer player starts with: the default look's gear and some money.
pub fn new_character() -> NewCharacter {
    let gear = [
        DEFAULT_HERO.armor,
        DEFAULT_HERO.armet,
        DEFAULT_HERO.r_weapon,
        DEFAULT_HERO.l_weapon,
    ];
    NewCharacter {
        money: START_MONEY,
        equipment: gear.into_iter().filter(|&k| k != 0).collect(),
        ..NewCharacter::default()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn backup_written_once_and_save_is_atomic() {
        let dir = std::env::temp_dir().join(format!("c3conquer-mapfile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("x.DMap");
        std::fs::write(&file, b"original").unwrap();
        super::write_map_file(&file, b"v1", true).unwrap();
        super::write_map_file(&file, b"v2", true).unwrap();
        super::write_map_file(&dir.join("y.DMap"), b"new", false).unwrap();
        assert!(!dir.join("y.DMap.bak").exists());
        assert_eq!(std::fs::read(&file).unwrap(), b"v2");
        assert_eq!(std::fs::read(dir.join("x.DMap.bak")).unwrap(), b"original");
        assert!(!dir.join("x.DMap.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
