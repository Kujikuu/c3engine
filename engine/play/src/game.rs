//! What a game being played is made of, for every host alike (Play mode, the client, the
//! server): the simulation of a map with the game's rules, the project's quests and scripts.

use crate::module::GameRules;
use c3_game::{GameplaySettings, Sim};
use c3_scene::Scene;
use c3_script::{Quest, ScriptHost, quest};
use std::path::PathBuf;

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
    /// The numbers the simulation runs by.
    pub settings: GameplaySettings,
    /// Players may attack each other on the map.
    pub pk: bool,
}

/// The game's rules, with the tables' problems marked as such.
fn rules_of(
    game: &dyn GameRules,
    tables: Option<&PathBuf>,
) -> (Box<dyn c3_game::Rules>, Vec<String>) {
    let (rules, problems) = game.rules(tables.map(PathBuf::as_path));
    let problems = problems
        .into_iter()
        .map(|p| format!("Game rules: {p}"))
        .collect();
    (rules, problems)
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

/// The simulation of `scene` with the game's rules, the project's quests and scripts; and
/// what was wrong with the data.
pub fn new_sim(scene: &Scene, setup: &GameSetup, game: &dyn GameRules) -> (Sim, Vec<String>) {
    // Blueprints compile first: their scripts are events and formulas the rules read.
    let built = setup
        .scripts
        .as_deref()
        .map(c3_script::blueprint::build_all);
    let (rules, mut problems) = rules_of(game, setup.tables.as_ref());
    problems.extend(built.into_iter().flatten());
    let mut sim = Sim::new(
        scene.grid.clone(),
        scene.npcs.clone(),
        scene.spawns.clone(),
        rules,
        setup.seed,
    );
    sim.set_settings(setup.settings.clone());
    sim.set_quests(quests(setup.tables.as_ref(), &mut problems));
    sim.set_map_ids(setup.map_ids.clone());
    sim.set_pk(setup.pk);
    sim.set_portals(&scene.portals);
    sim.set_regions(scene.regions.clone());
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
pub fn mirror_sim(
    scene: &Scene,
    tables: Option<&PathBuf>,
    game: &dyn GameRules,
    settings: &GameplaySettings,
) -> (Sim, Vec<String>) {
    let (rules, mut problems) = rules_of(game, tables);
    let mut sim = Sim::new(scene.grid.clone(), scene.npcs.clone(), Vec::new(), rules, 0);
    sim.set_settings(settings.clone());
    sim.set_regions(scene.regions.clone());
    sim.set_quests(quests(tables, &mut problems));
    (sim, problems)
}

/// A seed from the clock, for games that need not repeat.
pub fn time_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_secs())
}
