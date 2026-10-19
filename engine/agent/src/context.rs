//! Context tools: what the project is made of, the schemas of commands, the node library, the
//! script functions and the engine's documents; plus undo, redo and saving.

use crate::tools::{Tool, answer, arg, opt_arg, tool};
use crate::workspace::{self, DocRef, Workspace};
use c3_play::map_list::MapList;
use c3_script::blueprint::NODES;
use serde_json::{Value, json};

/// The engine's documents an agent may read, by topic.
const DOCS: &[(&str, &str, &str)] = &[
    (
        "agents",
        "How to work on a project with these tools",
        include_str!("../../../docs/AGENTS.md"),
    ),
    (
        "architecture",
        "The engine's crates and how they fit",
        include_str!("../../../docs/ARCHITECTURE.md"),
    ),
    (
        "formats",
        "The engine's file formats",
        include_str!("../../../docs/FORMATS.md"),
    ),
    (
        "decisions",
        "Why things are as they are",
        include_str!("../../../docs/DECISIONS.md"),
    ),
    (
        "vision",
        "What the engine is for",
        include_str!("../../../docs/VISION.md"),
    ),
    (
        "roadmap",
        "What is done and what comes next",
        include_str!("../../../docs/ROADMAP.md"),
    ),
];

pub(crate) fn tools() -> Vec<Tool> {
    let topics: Vec<&str> = DOCS.iter().map(|(t, ..)| *t).collect();
    vec![
        tool(
            "project_overview",
            "Start here. The project at a glance: its game, maps (key, ids, name), tables, \
             dialogues, scripts, blueprints, unsaved documents and what is wrong with its \
             settings.",
            json!({}),
            &[],
            overview,
        ),
        tool(
            "command_schemas",
            "The JSON schema of a kind of command or question: map (SceneCommand), map_query \
             (SceneQuery), table (TableCommand), settings (SettingsCommand), dialogue \
             (DialogueCommand), dialogue_graph (a whole DialogueGraph) or assets \
             (AssetCommand).",
            json!({ "kind": { "type": "string", "enum": [
                "map", "map_query", "table", "settings", "dialogue", "dialogue_graph", "assets"
            ] } }),
            &["kind"],
            schemas,
        ),
        tool(
            "node_library",
            "The blueprint nodes: id, title, category, kind and typed pins, filtered by \
             `category` or by `search` in the id or title. Blueprints name nodes by id.",
            json!({
                "category": { "type": "string" },
                "search": { "type": "string" },
            }),
            &[],
            node_library,
        ),
        tool(
            "script_reference",
            "The functions Rhai scripts (and so blueprints and dialogues) can call, each \
             with what it does.",
            json!({}),
            &[],
            script_reference,
        ),
        tool(
            "read_docs",
            "Reads one of the engine's documents. Topics: agents (read first), \
             architecture, formats, decisions, vision, roadmap.",
            json!({ "topic": { "type": "string", "enum": topics } }),
            &["topic"],
            read_docs,
        ),
        tool(
            "undo",
            "Undoes the last change of a document: map:<key>, table:<name>, settings, \
             dialogue:<name>, script:<name>, blueprint:<name> or assets.",
            json!({ "document": { "type": "string" } }),
            &["document"],
            |ws, args| step(ws, args, false),
        ),
        tool(
            "redo",
            "Redoes the last undone change of a document (named as for undo).",
            json!({ "document": { "type": "string" } }),
            &["document"],
            |ws, args| step(ws, args, true),
        ),
        tool(
            "save",
            "Saves every document with changes (dialogues and blueprints also write their \
             scripts). Nothing an agent changes reaches the files, or Play, before this.",
            json!({}),
            &[],
            |ws, _| Ok(json!({ "saved": ws.save()? })),
        ),
    ]
}

fn overview(ws: &mut dyn Workspace, _: &Value) -> Result<Value, String> {
    let project = ws.project().clone();
    let settings = ws.settings()?.settings().clone();
    let maps: Vec<Value> = (MapList::new(&project, ws.game(), &settings).iter())
        .map(|m| json!({ "key": m.key, "ids": m.ids, "name": m.name }))
        .collect();
    let scripts = project.scripts_dir();
    Ok(json!({
        "project": project.name(),
        "game": ws.game().name(),
        "maps": maps,
        "start_map": settings.gameplay.start_map,
        "tables": project.table_names(),
        "dialogues": c3_script::dialogue_names(&project.dialogues_dir()),
        "scripts": c3_script::script_names(&scripts),
        "blueprints": c3_script::blueprint::blueprint_names(&scripts),
        "npc_looks": ws.game().npc_looks().len(),
        "unsaved": ws.unsaved(),
        "settings_problems": settings.problems(),
    }))
}

fn schemas(_: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let kind: String = arg(args, "kind")?;
    Ok(match kind.as_str() {
        "map" => c3_edit::scene_command_schema(),
        "map_query" => c3_edit::scene_query_schema(),
        "table" => c3_edit::table_command_schema(),
        "settings" => c3_edit::settings_command_schema(),
        "dialogue" => answer(schemars::schema_for!(c3_edit::DialogueCommand))?,
        "dialogue_graph" => answer(schemars::schema_for!(c3_script::DialogueGraph))?,
        "assets" => answer(schemars::schema_for!(c3_edit::AssetCommand))?,
        _ => return Err(format!("no schema of \"{kind}\"")),
    })
}

fn node_library(_: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let category = opt_arg::<String>(args, "category")?.map(|c| c.to_lowercase());
    let search = opt_arg::<String>(args, "search")?.map(|s| s.to_lowercase());
    let pins = |pins: &[(&str, c3_script::blueprint::PinType)]| -> Vec<String> {
        pins.iter()
            .map(|(name, ty)| format!("{name}: {ty:?}"))
            .collect()
    };
    let nodes: Vec<Value> = (NODES.iter())
        .filter(|n| {
            category
                .as_ref()
                .is_none_or(|c| n.category.to_lowercase() == *c)
        })
        .filter(|n| {
            search.as_ref().is_none_or(|s| {
                n.id.to_lowercase().contains(s) || n.title.to_lowercase().contains(s)
            })
        })
        .map(|n| {
            json!({
                "id": n.id,
                "title": n.title,
                "category": n.category,
                "kind": format!("{:?}", n.kind),
                "inputs": pins(n.inputs),
                "outputs": pins(n.outputs),
                "exec_outs": n.exec_outs(),
            })
        })
        .collect();
    let mut categories: Vec<&str> = NODES.iter().map(|n| n.category).collect();
    categories.sort_unstable();
    categories.dedup();
    Ok(json!({ "nodes": nodes, "categories": categories }))
}

fn script_reference(_: &mut dyn Workspace, _: &Value) -> Result<Value, String> {
    let functions: Vec<Value> = (c3_script::API.iter())
        .map(|(call, what)| json!({ "call": call, "does": what }))
        .collect();
    Ok(json!({ "functions": functions }))
}

fn read_docs(_: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let topic: String = arg(args, "topic")?;
    let (_, _, text) = (DOCS.iter())
        .find(|(t, ..)| *t == topic)
        .ok_or_else(|| format!("no document \"{topic}\""))?;
    Ok(Value::String((*text).to_owned()))
}

fn step(ws: &mut dyn Workspace, args: &Value, redo: bool) -> Result<Value, String> {
    let doc = DocRef::parse(&arg::<String>(args, "document")?)?;
    let label = workspace::step(ws, &doc, redo)?;
    let what = if redo { "redone" } else { "undone" };
    Ok(match label {
        Some(label) => json!({ what: label }),
        None => json!({ what: Value::Null, "note": "nothing to step" }),
    })
}
