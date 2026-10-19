//! Blueprints: game rules and behaviours made of node graphs instead of code (Unreal-style).
//! A project keeps them in `scripts/blueprints/<name>.c3bp`; each compiles to the Rhai script
//! `scripts/graphs/<name>.rhai`, which the game runs as it runs hand-written scripts (its event
//! functions alongside `events.rhai`, its `talk()` for NPCs naming it).

mod compile;
mod library;
mod model;
mod pins;

pub use compile::{GraphRef, Problem, compile};
pub use library::{DefKind, NODES, NodeDef, node_def, returns};
pub use model::{
    Blueprint, Function, Graph, GraphNode, Literal, PinRef, PinType, Variable, VariableScope, Wire,
};
pub use pins::{EXEC_IN, Pins, formula_answer, pins};

use crate::host::names_with_suffix;
use serde::{Deserialize, Serialize};
use std::io::{Error, ErrorKind, Result};
use std::path::Path;

/// File extension of blueprints.
pub const BLUEPRINT_SUFFIX: &str = ".c3bp";
/// Where blueprints are kept, under the scripts folder.
pub const BLUEPRINT_DIR: &str = "blueprints";
/// Where their scripts are written, under the scripts folder.
pub const GRAPHS_DIR: &str = "graphs";

const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct BlueprintFile {
    format: u32,
    blueprint: Blueprint,
}

/// The script blueprint `name` compiles to (`graphs/<name>`), as scripts are named.
pub fn graph_script(name: &str) -> String {
    format!("{GRAPHS_DIR}/{name}")
}

/// The blueprints under `scripts`, by name, sorted.
pub fn blueprint_names(scripts: &Path) -> Vec<String> {
    names_with_suffix(&scripts.join(BLUEPRINT_DIR), BLUEPRINT_SUFFIX)
}

pub fn load_blueprint(file: &Path) -> Result<Blueprint> {
    let text = std::fs::read_to_string(file)?;
    let read: BlueprintFile = ron::from_str(&text)
        .map_err(|e| Error::new(ErrorKind::InvalidData, format!("{}: {e}", file.display())))?;
    Ok(read.blueprint)
}

pub fn save_blueprint(blueprint: &Blueprint, file: &Path) -> Result<()> {
    let data = BlueprintFile {
        format: FORMAT_VERSION,
        blueprint: blueprint.clone(),
    };
    let text = ron::ser::to_string_pretty(&data, ron::ser::PrettyConfig::default())
        .map_err(|e| Error::new(ErrorKind::InvalidData, e.to_string()))?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, text)
}

/// Compiles blueprint `name` of `scripts` and writes its script; the problems, if it has
/// any (its old script is then removed, so nothing half-made runs).
pub fn build_blueprint(scripts: &Path, name: &str) -> Result<Vec<Problem>> {
    let blueprint = load_blueprint(
        &scripts
            .join(BLUEPRINT_DIR)
            .join(format!("{name}{BLUEPRINT_SUFFIX}")),
    )?;
    let out = scripts
        .join(GRAPHS_DIR)
        .join(format!("{name}{}", crate::SCRIPT_SUFFIX));
    match compile(name, &blueprint) {
        Ok(code) => {
            if let Some(dir) = out.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&out, code)?;
            Ok(Vec::new())
        }
        Err(problems) => {
            let _ = std::fs::remove_file(&out);
            Ok(problems)
        }
    }
}

/// Compiles every blueprint of `scripts`; what is wrong, one line per problem.
pub fn build_all(scripts: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    for name in blueprint_names(scripts) {
        match build_blueprint(scripts, &name) {
            Ok(found) => problems.extend(found.into_iter().map(|p| {
                let at = p.node.map_or(String::new(), |n| format!(", node {n}"));
                format!("Blueprint {name}{at}: {}", p.message)
            })),
            Err(e) => problems.push(format!("Blueprint {name}: {e}")),
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn node(id: u32, def: &str, values: &[(&str, Literal)]) -> GraphNode {
        GraphNode {
            id,
            def: def.into(),
            pos: [0.0, 0.0],
            values: values
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    fn wire(from: u32, out: &str, to: u32, input: &str) -> Wire {
        Wire {
            from: PinRef {
                node: from,
                pin: out.into(),
            },
            to: PinRef {
                node: to,
                pin: input.into(),
            },
        }
    }

    /// On a kill: monster 5 pays 10 silver and counts kills, others get a message; then,
    /// 2 seconds later, a word.
    fn bounty() -> Blueprint {
        Blueprint {
            variables: vec![Variable {
                name: "kills".into(),
                ty: PinType::Number,
                scope: VariableScope::Player,
                default: Literal::Number(0.0),
            }],
            events: Graph {
                nodes: vec![
                    node(1, "event.kill", &[]),
                    node(2, "flow.branch", &[]),
                    node(3, "logic.equal", &[("b", Literal::Number(5.0))]),
                    node(4, "player.give_money", &[("amount", Literal::Number(10.0))]),
                    node(
                        5,
                        "player.message",
                        &[("text", Literal::Text("Not that one.".into()))],
                    ),
                    node(6, "flow.set", &[("name", Literal::Text("kills".into()))]),
                    node(7, "math.add", &[("b", Literal::Number(1.0))]),
                    node(8, "flow.get", &[("name", Literal::Text("kills".into()))]),
                    node(9, "flow.wait", &[("seconds", Literal::Number(2.0))]),
                    node(
                        10,
                        "player.message",
                        &[("text", Literal::Text("Well done.".into()))],
                    ),
                ],
                wires: vec![
                    wire(1, "then", 2, "exec"),
                    wire(1, "monster", 3, "a"),
                    wire(3, "value", 2, "condition"),
                    wire(2, "true", 4, "exec"),
                    wire(2, "false", 5, "exec"),
                    wire(4, "then", 6, "exec"),
                    wire(8, "value", 7, "a"),
                    wire(7, "value", 6, "value"),
                    wire(6, "then", 9, "exec"),
                    wire(9, "then", 10, "exec"),
                ],
            },
            functions: Vec::new(),
        }
    }

    #[test]
    fn events_compile_to_script_functions() {
        let code = compile("bounty", &bounty()).unwrap();
        assert!(code.contains("fn on_kill(p_monster) {"), "{code}");
        assert!(code.contains("if (p_monster == 5) {"), "{code}");
        assert!(code.contains("give_money(10);"), "{code}");
        assert!(
            code.contains("set_flag(\"kills\", (flag(\"kills\") + 1));"),
            "{code}"
        );
        assert!(code.contains("start_timer(\"bp:bounty:9\", 2);"), "{code}");
        assert!(
            code.contains("if p_name == \"bp:bounty:9\" { wait_9(); return; }"),
            "{code}"
        );
        assert!(code.contains("fn wait_9() {"), "{code}");
        // What it compiles to is a script the engine reads.
        let host = crate::ScriptHost::new(std::env::temp_dir(), None);
        let problems = host.check(&code, None);
        assert!(problems.is_empty(), "{problems:?}\n{code}");
    }

    /// Every library action and pure node compiles to Rhai the engine reads.
    #[test]
    fn every_library_node_compiles() {
        let mut graph = Graph::default();
        graph.nodes.push(node(
            1,
            "event.custom",
            &[("name", Literal::Text("all".into()))],
        ));
        let mut last = (1, "then");
        let mut id = 2;
        for def in NODES {
            match def.kind {
                DefKind::Action => {
                    graph.nodes.push(node(id, def.id, &[]));
                    graph.wires.push(wire(last.0, last.1, id, "exec"));
                    last = (id, "then");
                    id += 1;
                }
                DefKind::Pure if def.outputs.iter().all(|(_, t)| t.fits(PinType::Text)) => {
                    graph.nodes.push(node(id, def.id, &[]));
                    graph.nodes.push(node(id + 1, "player.message", &[]));
                    graph.wires.push(wire(id, "value", id + 1, "text"));
                    graph.wires.push(wire(last.0, last.1, id + 1, "exec"));
                    last = (id + 1, "then");
                    id += 2;
                }
                _ => {}
            }
        }
        let bp = Blueprint {
            events: graph,
            ..Blueprint::default()
        };
        let code = compile("all", &bp).unwrap();
        let host = crate::ScriptHost::new(std::env::temp_dir(), None);
        let problems = host.check(&code, None);
        assert!(problems.is_empty(), "{problems:?}\n{code}");
    }

    /// A formula graph answers the rules: hit = attacker's attack_max * 2.
    #[test]
    fn formula_graphs_answer_the_rules() {
        let bp = Blueprint {
            events: Graph {
                nodes: vec![
                    node(1, "formula.hit", &[]),
                    node(2, "flow.return", &[]),
                    node(3, "math.multiply", &[("b", Literal::Number(2.0))]),
                    node(
                        4,
                        "record.get",
                        &[("field", Literal::Text("attack_max".into()))],
                    ),
                ],
                wires: vec![
                    wire(1, "then", 2, "exec"),
                    wire(1, "attacker", 4, "record"),
                    wire(4, "value", 3, "a"),
                    wire(3, "value", 2, "value"),
                ],
            },
            ..Blueprint::default()
        };
        let code = compile("rules", &bp).unwrap();
        let formulas = crate::Formulas::compile(&code).unwrap();
        let mut me = rhai::Map::new();
        me.insert("attack_max".into(), 21_i64.into());
        let damage = formulas.int("hit", (me, rhai::Map::new())).unwrap();
        assert_eq!(damage, 42, "{code}");
    }

    #[test]
    fn problems_name_their_nodes() {
        let mut bp = bounty();
        // Wrong type: a text into the branch.
        bp.events
            .wires
            .retain(|w| !(w.to.node == 2 && w.to.pin == "condition"));
        bp.events.nodes.push(node(11, "player.name", &[]));
        bp.events.wires.push(wire(11, "value", 2, "condition"));
        // A second kill event, and an unknown node.
        bp.events.nodes.push(node(12, "event.kill", &[]));
        bp.events.nodes.push(node(13, "nothing.here", &[]));
        let problems = compile("bounty", &bp).unwrap_err();
        let at: Vec<Option<u32>> = problems.iter().map(|p| p.node).collect();
        assert!(at.contains(&Some(2)), "{problems:?}");
        assert!(at.contains(&Some(12)), "{problems:?}");
        assert!(at.contains(&Some(13)), "{problems:?}");
    }

    #[test]
    fn blueprints_save_and_load() {
        let dir = std::env::temp_dir().join(format!("c3-bp-{}", std::process::id()));
        let file = dir.join(BLUEPRINT_DIR).join("bounty.c3bp");
        save_blueprint(&bounty(), &file).unwrap();
        assert_eq!(load_blueprint(&file).unwrap(), bounty());
        assert_eq!(blueprint_names(&dir), ["bounty"]);
        assert!(build_blueprint(&dir, "bounty").unwrap().is_empty());
        assert!(dir.join(GRAPHS_DIR).join("bounty.rhai").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
