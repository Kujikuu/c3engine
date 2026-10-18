//! What a Conquer game is made of, for every host alike (Play mode, the client, the server):
//! a project's maps by their key, the simulation of one with Conquer's rules, the project's
//! quests and scripts, and the character a new player starts as.

use crate::Content;
use crate::character::DEFAULT_HERO;
use crate::rules::ConquerRules;
use c3_game::{PlayerSave, Rules, Sim};
use c3_project::Project;
use c3_scene::{CellPos, Scene};
use c3_script::{Quest, ScriptHost, quest};
use std::path::PathBuf;

/// Money a new character starts with, to try shops.
pub const START_MONEY: u64 = 1000;

/// Loads map `key` of `project`: one of its engine scenes by name, or a content map file
/// with the population the project keeps for it.
pub fn load_map(project: &Project, content: &Content, key: &str) -> Result<Scene, String> {
    if project.map_names().iter().any(|n| n == key) {
        return c3_scene::native::load_scene(&project.map_file(key))
            .map_err(|e| format!("map {key}: {e}"));
    }
    let mut scene = content.load_map(key)?.scene;
    let stem = key.rsplit(['/', '\\']).next().unwrap_or(key);
    let stem = stem.rsplit_once('.').map_or(stem, |(s, _)| s);
    let population = c3_scene::native::load_population(&project.population_file(stem))
        .map_err(|e| e.to_string())?;
    scene.set_population(population);
    Ok(scene)
}

/// Where a game's data is and how its simulation starts.
#[derive(Debug, Clone, Default)]
pub struct GameSetup {
    /// The project's `scripts/` (NPCs talk only with it).
    pub scripts: Option<PathBuf>,
    /// The project's `tables/` (rules and quests).
    pub tables: Option<PathBuf>,
    /// The game's ids for the map (script teleports within it work).
    pub map_ids: Vec<u32>,
    pub seed: u64,
}

/// Conquer's rules from the project's tables (or built-in ones), with the tables' problems.
fn rules(tables: Option<&PathBuf>) -> (Box<dyn Rules>, Vec<String>) {
    match tables {
        Some(dir) => {
            let (rules, problems) = ConquerRules::from_tables(dir);
            let problems = problems
                .into_iter()
                .map(|p| format!("Game rules: {p}"))
                .collect();
            (Box::new(rules), problems)
        }
        None => (Box::new(ConquerRules::default()), Vec::new()),
    }
}

/// The project's quests, if it has a quests table.
fn quests(tables: Option<&PathBuf>, problems: &mut Vec<String>) -> Vec<Quest> {
    let Some(dir) = tables else {
        return Vec::new();
    };
    let file = dir.join(format!("{}{}", quest::QUEST_TABLE, c3_data::TABLE_SUFFIX));
    match c3_data::load_table(&file) {
        Ok(table) => Quest::all(&table),
        Err(e) => {
            if file.exists() {
                problems.push(format!("Quests: {e}"));
            }
            Vec::new()
        }
    }
}

/// The simulation of `scene` with Conquer's rules, the project's quests and scripts; and
/// what was wrong with the data.
pub fn new_sim(scene: &Scene, setup: &GameSetup) -> (Sim, Vec<String>) {
    let (rules, mut problems) = rules(setup.tables.as_ref());
    let mut sim = Sim::new(
        scene.grid.clone(),
        scene.npcs.clone(),
        scene.spawns.clone(),
        rules,
        setup.seed,
    );
    sim.set_quests(quests(setup.tables.as_ref(), &mut problems));
    if let Some(dir) = &setup.scripts {
        sim.set_scripts(
            ScriptHost::new(dir, setup.tables.clone()),
            setup.map_ids.clone(),
        );
    }
    (sim, problems)
}

/// A client's mirror of a server's simulation of `scene`: the same rules and quests (for
/// names and the quest log), no monsters of its own and no scripts.
pub fn mirror_sim(scene: &Scene, tables: Option<&PathBuf>) -> (Sim, Vec<String>) {
    let (rules, mut problems) = rules(tables);
    let mut sim = Sim::new(scene.grid.clone(), scene.npcs.clone(), Vec::new(), rules, 0);
    sim.set_quests(quests(tables, &mut problems));
    (sim, problems)
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
