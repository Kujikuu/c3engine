//! An agent making a small game from nothing through the tools alone.

use crate::{Headless, Workspace, call};
use c3_play::basic::BasicGame;
use c3_project::{Manifest, Project};
use serde_json::{Value, json};
use std::rc::Rc;

fn scratch_project(test: &str) -> (Headless, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("c3agent-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let project = Project::create(&root, Manifest::new("Agent test")).unwrap();
    (Headless::new(project, Rc::new(BasicGame::default())), root)
}

fn run(ws: &mut Headless, tool: &str, args: Value) -> Value {
    call(ws, tool, &args).unwrap_or_else(|e| panic!("{tool}: {e}"))
}

#[test]
fn an_agent_makes_a_talking_npc_and_plays_it() {
    let (mut ws, root) = scratch_project("npc");
    run(
        &mut ws,
        "create_map",
        json!({ "name": "Meadow", "width": 32, "height": 32 }),
    );
    run(
        &mut ws,
        "create",
        json!({ "kind": "dialogue", "name": "elder" }),
    );
    let npc = json!({
        "id": 7, "name": "Elder", "cell": [16, 16], "look": 1, "script": "dialogue/elder"
    });
    let done = run(
        &mut ws,
        "edit_map",
        json!({
            "map": "Meadow", "commands": [{ "op": "add_npc", "npc": npc }]
        }),
    );
    assert!(done["results"][0]["done"].is_string(), "{done}");
    let graph = run(&mut ws, "read_dialogue", json!({ "name": "elder" }));
    let mut node = graph["graph"]["nodes"][0].clone();
    node["kind"]["Say"]["lines"] = json!(["Welcome to the meadow, {player}."]);
    run(
        &mut ws,
        "edit_dialogue",
        json!({
            "name": "elder", "commands": [{ "op": "set_node", "node": node }]
        }),
    );
    assert_eq!(ws.unsaved(), ["map:Meadow", "settings", "dialogue:elder"]);
    // Undo and redo step the agent's own changes.
    run(&mut ws, "undo", json!({ "document": "map:Meadow" }));
    let npcs = run(
        &mut ws,
        "query_map",
        json!({ "map": "Meadow", "query": { "query": "npcs" } }),
    );
    assert_eq!(npcs, json!([]));
    run(&mut ws, "redo", json!({ "document": "map:Meadow" }));
    let saved = run(&mut ws, "save", json!({}));
    assert_eq!(saved["saved"].as_array().unwrap().len(), 3);

    let test = run(
        &mut ws,
        "playtest",
        json!({
            "map": "Meadow", "cell": [14, 16], "steps": [{ "Talk": 7 }]
        }),
    );
    let page = &test["steps"][1]["dialogue"]["page"];
    assert!(
        page.to_string().contains("Welcome to the meadow, Tester."),
        "{test}"
    );
    let overview = run(&mut ws, "project_overview", json!({}));
    assert_eq!(overview["maps"][0]["key"], "Meadow");
    assert_eq!(overview["dialogues"], json!(["elder"]));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn tables_scripts_and_blueprints_change_through_tools() {
    let (mut ws, root) = scratch_project("data");
    let columns = json!([{ "name": "name", "kind": "Text" }, { "name": "price", "kind": "Int" }]);
    run(
        &mut ws,
        "create_table",
        json!({ "name": "goods", "columns": columns }),
    );
    run(
        &mut ws,
        "edit_table",
        json!({ "table": "goods", "commands": [
        { "op": "add_row", "values": [1, "Apple", 5] },
        { "op": "set_cell", "key": 1, "column": "price", "value": 7 },
    ] }),
    );
    let rows = run(
        &mut ws,
        "read_table",
        json!({ "table": "goods", "search": "app" }),
    );
    assert_eq!(
        rows["rows"],
        json!([{ "id": 1, "name": "Apple", "price": 7 }])
    );

    run(
        &mut ws,
        "create",
        json!({ "kind": "script", "name": "events" }),
    );
    let bad = run(
        &mut ws,
        "write_script",
        json!({ "name": "events", "text": "fn on_login( {" }),
    );
    assert_eq!(bad["problems"].as_array().unwrap().len(), 1, "{bad}");

    run(
        &mut ws,
        "create",
        json!({ "kind": "blueprint", "name": "rules" }),
    );
    let read = run(&mut ws, "read_blueprint", json!({ "name": "rules" }));
    assert_eq!(read["problems"], json!([]));
    let nodes = run(&mut ws, "node_library", json!({ "search": "login" }));
    assert!(!nodes["nodes"].as_array().unwrap().is_empty());
    run(
        &mut ws,
        "edit_settings",
        json!({ "commands": [
        { "op": "set", "path": "gameplay.new_character.money", "value": 250 }
    ] }),
    );
    assert_eq!(
        run(
            &mut ws,
            "read_settings",
            json!({ "path": "gameplay.new_character.money" })
        ),
        250
    );
    run(&mut ws, "save", json!({}));
    let table = c3_data::load_table(&ws.project().table_file("goods")).unwrap();
    assert_eq!(table.rows.len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
