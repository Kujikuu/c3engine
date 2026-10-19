//! Map tools: questions about a map, edits to it, and new maps.

use crate::tools::{Tool, answer, arg, opt_arg, outcomes, tool};
use crate::workspace::{DocRef, Workspace};
use c3_edit::{SceneCommand, SceneQuery, SettingsCommand};
use c3_play::map_list::MapList;
use serde_json::{Value, json};

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        tool(
            "query_map",
            "Asks about a map without changing it. `query` is a SceneQuery (see \
             command_schemas kind map_query): {\"query\":\"summary\"}, {\"query\":\"npcs\"}, \
             {\"query\":\"spawns\"}, {\"query\":\"portals\"}, {\"query\":\"cell\",\"cell\":[x,y]}, \
             {\"query\":\"walkability\",\"min\":[x,y],\"max\":[x,y]}, \
             {\"query\":\"objects\",\"center\":[x,y],\"radius\":r}.",
            json!({
                "map": { "type": "string", "description": "A map key from project_overview" },
                "query": { "type": "object" },
            }),
            &["map", "query"],
            query_map,
        ),
        tool(
            "edit_map",
            "Changes a map with SceneCommands (see command_schemas kind map), run in order as \
             one undo step: walkability, surfaces, heights, portals, objects, NPCs, monster \
             spawns, regions, ground, layers, size. Refused commands are skipped and say why. \
             Changes stay unsaved until `save`.",
            json!({
                "map": { "type": "string" },
                "commands": { "type": "array", "items": { "type": "object" } },
                "label": { "type": "string", "description": "The undo step's name" },
            }),
            &["map", "commands"],
            edit_map,
        ),
        tool(
            "create_map",
            "Makes a new empty project map (all walkable, the engine's ground) and lists it in \
             the settings' maps with a free id.",
            json!({
                "name": { "type": "string" },
                "width": { "type": "integer", "description": "Cells across (default 64)" },
                "height": { "type": "integer", "description": "Cells down (default 64)" },
            }),
            &["name"],
            create_map,
        ),
    ]
}

fn query_map(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let map: String = arg(args, "map")?;
    let query: SceneQuery = arg(args, "query")?;
    let result = ws.scene(&map)?.query(&query).map_err(|e| e.to_string())?;
    answer(result)
}

fn edit_map(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let map: String = arg(args, "map")?;
    let commands: Vec<SceneCommand> = arg(args, "commands")?;
    let label = opt_arg::<String>(args, "label")?.unwrap_or_else(|| "Agent edit".into());
    let results = ws.scene(&map)?.execute_batch(label, &commands);
    ws.changed(&DocRef::Map(map));
    outcomes(results)
}

fn create_map(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let name: String = arg(args, "name")?;
    let width = opt_arg::<u32>(args, "width")?.unwrap_or(64).clamp(1, 1024);
    let height = opt_arg::<u32>(args, "height")?.unwrap_or(64).clamp(1, 1024);
    if name.trim().is_empty() || name.contains(['/', '\\', ':']) {
        return Err(format!("\"{name}\" can't name a map"));
    }
    let project = ws.project().clone();
    let file = project.map_file(&name);
    if file.exists() {
        return Err(format!("the project already has a map \"{name}\""));
    }
    let settings = ws.settings()?.settings().clone();
    let cell = settings.camera.new_map_cell.map(|v| v as f32);
    let scene =
        c3_play::basic::blank_scene(width, height, cell, c3_play::basic::GroundImage::BUILTIN);
    (file.parent())
        .map_or(Ok(()), std::fs::create_dir_all)
        .map_err(|e| e.to_string())?;
    c3_scene::native::save_scene(&scene, &file).map_err(|e| e.to_string())?;
    // Listed with the next free id, as the Studio's New Map does.
    let list = MapList::new(&project, ws.game(), &settings);
    let id = list.by_key(&name).map(|m| m.id()).filter(|&id| id > 0);
    let id = id.unwrap_or_else(|| {
        list.iter()
            .flat_map(|m| m.ids.iter().copied())
            .max()
            .unwrap_or(0)
            + 1
    });
    let mut maps = settings.maps.clone();
    maps.push(c3_core::settings::MapEntry {
        key: name.clone(),
        id,
        ..Default::default()
    });
    let command = SettingsCommand::Set {
        path: "maps".into(),
        value: serde_json::to_value(&maps).map_err(|e| e.to_string())?,
    };
    ws.settings()?
        .execute(&command)
        .map_err(|e| e.to_string())?;
    ws.changed(&DocRef::Settings);
    ws.refresh();
    Ok(json!({ "made": format!("map {name}, {width}×{height} cells, id {id}") }))
}
