//! Play-testing without a window: a player on a map of the project, doing what the steps say,
//! and what happened (events, dialogue pages, command replies) step by step.

use crate::tools::{Tool, arg, opt_arg, tool};
use crate::workspace::Workspace;
use c3_game::{Event, Input};
use c3_play::game::{GameSetup, new_sim};
use c3_scene::CellPos;
use serde::Deserialize;
use serde_json::{Value, json};

/// Game time a step is given to play out, unless it says otherwise.
const SETTLE_MS: u64 = 1_000;
/// Longest a playtest may run, in game time.
const MAX_MS: u64 = 10 * 60 * 1_000;

/// One thing the test player does.
#[derive(Debug, Deserialize)]
enum Step {
    /// Lets `ms` of game time pass.
    Wait(u64),
    /// Anything a player asks of the game (`{"Walk":[x,y]}`, `{"Say":"/level 10"}`,
    /// `{"Talk":npc}`, `{"Answer":0}`...).
    #[serde(untagged)]
    Input(Input),
}

pub(crate) fn tools() -> Vec<Tool> {
    vec![tool(
        "playtest",
        "Plays the saved project without a window: a game-master test player on `map` (else \
         the start map) at `cell`, doing `steps` in order; each step then plays for 1 s of \
         game time (`{\"Wait\":ms}` waits longer). Steps are player inputs: {\"Walk\":[x,y]}, \
         {\"Jump\":[x,y]}, {\"Talk\":npc_id}, {\"Answer\":n}, {\"CloseDialogue\":null}, \
         {\"Attack\":entity}, {\"Say\":\"text or /command\"}, {\"Use\":item_id}, \
         {\"Buy\":{\"npc\":id,\"kind\":type}}... Answers what happened at each step (events, the \
         open dialogue page) and the player at the end. Save first: it plays the files.",
        json!({
            "map": { "type": "string" },
            "cell": { "type": "array", "items": { "type": "integer" } },
            "level": { "type": "integer" },
            "profession": { "type": "integer", "description": "The game's profession number" },
            "steps": { "type": "array", "items": {} },
        }),
        &["steps"],
        playtest,
    )]
}

fn playtest(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let steps: Vec<Step> = arg(args, "steps")?;
    let project = ws.project().clone();
    let settings = ws.settings()?.settings().clone();
    let game = ws.game();
    let start = &settings.gameplay.start_map;
    let map = match opt_arg::<String>(args, "map")? {
        Some(map) => map,
        None if !start.is_empty() => start.clone(),
        None => game
            .default_map(&project)
            .ok_or("the project has no map to play")?,
    };
    let scene = game.load_map(&project, &map)?;
    let mut gameplay = settings.gameplay.clone();
    gameplay.gm_everyone = true;
    let map_ids = game.map_ids(&map);
    let setup = GameSetup {
        scripts: Some(project.scripts_dir()),
        tables: Some(project.tables_dir()),
        pk: c3_play::settings::pk_map(&settings, &map_ids),
        pk_free: c3_play::settings::pk_free_map(&settings, &map_ids),
        revive: c3_play::settings::revive_point(&settings, &map_ids),
        map_ids,
        seed: 1,
        settings: gameplay,
        breakpoints: Default::default(),
    };
    let rules = game.rules();
    let (mut sim, problems) = new_sim(&scene, &setup, &*rules);
    let middle = (scene.grid.width as i32 / 2, scene.grid.height as i32 / 2);
    let cell: CellPos = match opt_arg::<[i32; 2]>(args, "cell")? {
        Some([x, y]) => (x, y),
        None => settings.gameplay.start_cell.map_or(middle, |[x, y]| (x, y)),
    };
    let cell = c3_scene::path::nearest_walkable(&scene.grid, cell, 20).unwrap_or(cell);
    let level = opt_arg::<u32>(args, "level")?.unwrap_or(1);
    let mut save = c3_play::settings::new_character(&settings.gameplay, "Tester", cell);
    save.level = level.max(1);
    save.profession = opt_arg::<u32>(args, "profession")?.unwrap_or(0);
    let player = sim.add_saved_player(&save);
    sim.logged_in(player);
    let mut log = vec![report(&mut sim, player, json!("enter"))];
    let mut played = 0;
    for step in steps {
        let (label, ms) = match step {
            Step::Wait(ms) => (json!({ "Wait": ms }), ms),
            Step::Input(input) => {
                let label = serde_json::to_value(&input).unwrap_or_default();
                sim.input(player, input);
                (label, SETTLE_MS)
            }
        };
        let ms = ms.min(MAX_MS.saturating_sub(played));
        sim.advance(ms);
        played += ms;
        log.push(report(&mut sim, player, label));
    }
    let end = sim.player(player).map(|p| {
        json!({
            "cell": p.cell, "level": p.level, "life": p.life, "money": p.money,
            "bag": p.bag, "equipment": p.equipment, "flags": p.flags,
            "alive": p.is_alive(), "profession": p.profession,
            "attributes": p.attributes, "stats": {
                "life": p.stats.max_life, "mana": p.stats.max_mana,
                "attack": [p.stats.attack_min, p.stats.attack_max],
                "defence": p.stats.defence, "dodge": p.stats.dodge,
            },
        })
    });
    Ok(json!({
        "map": map,
        "problems": problems,
        "steps": log,
        "player": end,
        "played_ms": played,
    }))
}

/// What happened since the last report: the events (steps folded into where things ended up)
/// and the dialogue page the player has open.
fn report(sim: &mut c3_game::Sim, player: c3_game::EntityId, step: Value) -> Value {
    let mut events = Vec::new();
    let mut moved = std::collections::BTreeMap::new();
    for event in sim.take_events() {
        match event {
            Event::Moved { id, to, .. } => {
                moved.insert(id, to);
            }
            event => events.push(serde_json::to_value(event).unwrap_or_default()),
        }
    }
    let at = sim.player(player).map(|p| p.cell);
    let dialogue = sim.player(player).and_then(|p| p.dialogue.as_ref()).map(
        |d| json!({ "npc": d.npc.name, "page": serde_json::to_value(&d.page).unwrap_or_default() }),
    );
    json!({
        "step": step,
        "events": events,
        "moved": moved.len(),
        "player_cell": at,
        "dialogue": dialogue,
    })
}
