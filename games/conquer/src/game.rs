//! Conquer's maps by their key and the character a new player starts as.

use crate::Content;
use crate::character::DEFAULT_HERO;
use c3_game::PlayerSave;
use c3_project::Project;
use c3_scene::{CellPos, Scene};

/// Money a new character starts with, to try shops.
pub const START_MONEY: u64 = 1000;

/// Loads map `key` of `project`: one of its engine scenes by name, or a content map file
/// with the population the project keeps for it.
pub fn load_map(project: &Project, content: &Content, key: &str) -> Result<Scene, String> {
    if let Some(scene) = c3_play::load_project_scene(project, key) {
        return scene;
    }
    let mut scene = content.load_map(key)?.scene;
    let stem = key.rsplit(['/', '\\']).next().unwrap_or(key);
    let stem = stem.rsplit_once('.').map_or(stem, |(s, _)| s);
    let population = c3_scene::native::load_population(&project.population_file(stem))
        .map_err(|e| e.to_string())?;
    scene.set_population(population);
    Ok(scene)
}

/// The character a new Conquer player starts as: the default look's gear and some money.
pub fn new_character(name: &str, cell: CellPos) -> PlayerSave {
    let gear = [
        DEFAULT_HERO.armor,
        DEFAULT_HERO.armet,
        DEFAULT_HERO.r_weapon,
        DEFAULT_HERO.l_weapon,
    ];
    PlayerSave {
        money: START_MONEY,
        equipment: gear.into_iter().filter(|&k| k != 0).collect(),
        ..PlayerSave::new(name, cell)
    }
}
