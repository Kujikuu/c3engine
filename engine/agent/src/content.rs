//! Behaviour tools: dialogues (graphs NPCs talk by), scripts (Rhai) and blueprints (node
//! graphs of game rules), each read with what is wrong with it.

use crate::tools::{Tool, answer, arg, opt_arg, outcomes, tool};
use crate::workspace::{DocRef, Workspace};
use c3_edit::DialogueCommand;
use c3_script::blueprint::{self, Blueprint};
use c3_script::{DialogueGraph, ScriptHost};
use serde_json::{Value, json};
use std::path::PathBuf;

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        tool(
            "read_dialogue",
            "Reads a dialogue graph (Say nodes with lines, actions and answers; Branch nodes \
             with conditions) and its problems. NPCs run it by naming it as their script.",
            json!({ "name": { "type": "string" } }),
            &["name"],
            read_dialogue,
        ),
        tool(
            "edit_dialogue",
            "Changes a dialogue graph with DialogueCommands (see command_schemas kind \
             dialogue), one undo step: add_node, set_node, remove_node, set_start.",
            json!({
                "name": { "type": "string" },
                "commands": { "type": "array", "items": { "type": "object" } },
            }),
            &["name", "commands"],
            edit_dialogue,
        ),
        tool(
            "read_script",
            "Reads a Rhai script (`events` is the game's event hooks, `rules` its formulas, \
             others are NPC scripts) and its problems.",
            json!({ "name": { "type": "string" } }),
            &["name"],
            read_script,
        ),
        tool(
            "write_script",
            "Replaces a script's text (one undo step) and answers its problems. See \
             script_reference for the functions scripts can call.",
            json!({ "name": { "type": "string" }, "text": { "type": "string" } }),
            &["name", "text"],
            write_script,
        ),
        tool(
            "read_blueprint",
            "Reads a blueprint (variables, the event graph, functions; nodes by library id \
             with pin values and wires) as JSON, and its compile problems.",
            json!({ "name": { "type": "string" } }),
            &["name"],
            read_blueprint,
        ),
        tool(
            "write_blueprint",
            "Replaces a blueprint with `blueprint` (the JSON read_blueprint gives), one undo \
             step, and answers its compile problems. See node_library for the nodes.",
            json!({ "name": { "type": "string" }, "blueprint": { "type": "object" } }),
            &["name", "blueprint"],
            write_blueprint,
        ),
        tool(
            "create",
            "Makes a new empty dialogue, script or blueprint (a script may start with `text`).",
            json!({
                "kind": { "type": "string", "enum": ["dialogue", "script", "blueprint"] },
                "name": { "type": "string" },
                "text": { "type": "string" },
            }),
            &["kind", "name"],
            create,
        ),
    ]
}

fn read_dialogue(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let name: String = arg(args, "name")?;
    let graph = ws.dialogue(&name)?.graph();
    Ok(json!({ "graph": answer(graph)?, "problems": dialogue_problems(graph) }))
}

fn dialogue_problems(graph: &DialogueGraph) -> Vec<String> {
    (graph.problems().into_iter())
        .map(|p| match p.node {
            Some(node) => format!("node {node}: {}", p.message),
            None => p.message,
        })
        .collect()
}

fn edit_dialogue(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let name: String = arg(args, "name")?;
    let commands: Vec<DialogueCommand> = arg(args, "commands")?;
    let doc = ws.dialogue(&name)?;
    let results: Vec<_> = commands.iter().map(|c| doc.execute(c)).collect();
    let problems = dialogue_problems(doc.graph());
    ws.changed(&DocRef::Dialogue(name));
    let mut done = outcomes(results)?;
    done["problems"] = json!(problems);
    Ok(done)
}

/// What is wrong with script `text`, one line each.
fn script_problems(ws: &dyn Workspace, text: &str) -> Vec<String> {
    let project = ws.project();
    let host = ScriptHost::new(project.scripts_dir(), Some(project.tables_dir()));
    (host.check(text, None).into_iter())
        .map(|d| format!("{:?} at {}:{}: {}", d.severity, d.line, d.column, d.message))
        .collect()
}

fn read_script(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let name: String = arg(args, "name")?;
    let text = ws.script(&name)?.text().to_owned();
    Ok(json!({ "problems": script_problems(ws, &text), "text": text }))
}

fn write_script(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let name: String = arg(args, "name")?;
    let text: String = arg(args, "text")?;
    let problems = script_problems(ws, &text);
    let outcome = ws.script(&name)?.set_text(text, "Agent edit");
    ws.changed(&DocRef::Script(name));
    Ok(json!({ "done": outcome.summary, "problems": problems }))
}

/// What is wrong with `blueprint` named `name`, one line each.
fn blueprint_problems(name: &str, blueprint: &Blueprint) -> Vec<String> {
    match blueprint::compile(name, blueprint) {
        Ok(_) => Vec::new(),
        Err(problems) => (problems.into_iter())
            .map(|p| {
                let graph = match p.graph {
                    blueprint::GraphRef::Events => "event graph".to_owned(),
                    blueprint::GraphRef::Function(f) => format!("function {f}"),
                };
                match p.node {
                    Some(node) => format!("{graph}, node {node}: {}", p.message),
                    None => format!("{graph}: {}", p.message),
                }
            })
            .collect(),
    }
}

fn read_blueprint(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let name: String = arg(args, "name")?;
    let blueprint = ws.blueprint(&name)?.value();
    Ok(json!({
        "blueprint": answer(blueprint)?,
        "problems": blueprint_problems(&name, blueprint),
    }))
}

fn write_blueprint(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let name: String = arg(args, "name")?;
    let blueprint: Blueprint = arg(args, "blueprint")?;
    let problems = blueprint_problems(&name, &blueprint);
    let outcome = ws.blueprint(&name)?.set(blueprint, "Agent edit");
    ws.changed(&DocRef::Blueprint(name));
    Ok(json!({ "done": outcome.summary, "problems": problems }))
}

/// Where a new `kind` called `name` is kept.
fn new_file(ws: &dyn Workspace, kind: &str, name: &str) -> Result<PathBuf, String> {
    let project = ws.project();
    let scripts = project.scripts_dir();
    Ok(match kind {
        "dialogue" => project.dialogue_file(name),
        "script" => scripts.join(format!("{name}{}", c3_script::SCRIPT_SUFFIX)),
        "blueprint" => (scripts.join(blueprint::BLUEPRINT_DIR))
            .join(format!("{name}{}", blueprint::BLUEPRINT_SUFFIX)),
        _ => {
            return Err(format!(
                "can't make a \"{kind}\": dialogue, script or blueprint"
            ));
        }
    })
}

fn create(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let kind: String = arg(args, "kind")?;
    let name: String = arg(args, "name")?;
    if name.trim().is_empty() || name.contains(['\\', ':', '.']) {
        return Err(format!("\"{name}\" can't name a {kind}"));
    }
    let file = new_file(ws, &kind, &name)?;
    if file.exists() {
        return Err(format!("the project already has a {kind} \"{name}\""));
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let written = match kind.as_str() {
        "dialogue" => c3_script::save_dialogue(&DialogueGraph::default(), &file),
        "blueprint" => blueprint::save_blueprint(&Blueprint::default(), &file),
        _ => std::fs::write(&file, opt_arg::<String>(args, "text")?.unwrap_or_default()),
    };
    written.map_err(|e| e.to_string())?;
    ws.refresh();
    Ok(json!({ "made": format!("{kind} {name}") }))
}
