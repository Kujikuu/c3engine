//! Collapse to function: selected nodes of a graph move into a new function, called where
//! they were. Values wired in from outside become the function's inputs; the one exec wire
//! in and the one exec wire out go through the call.

use super::compile::GraphRef;
use super::library::{DefKind, node_def};
use super::model::{Blueprint, Function, Graph, GraphNode, Literal, PinRef, PinType, Wire};
use super::pins::{EXEC_IN, pins};
use std::collections::BTreeSet;

/// Moves `nodes` of graph `which` into a new function `name`, called in their place.
pub fn collapse(
    blueprint: &mut Blueprint,
    which: &GraphRef,
    nodes: &[u32],
    name: &str,
) -> Result<(), String> {
    let chosen: BTreeSet<u32> = nodes.iter().copied().collect();
    if chosen.is_empty() {
        return Err("Select the nodes to collapse".into());
    }
    if blueprint.functions.iter().any(|f| f.name == name) {
        return Err(format!("There is a function called {name} already"));
    }
    let graph = match which {
        GraphRef::Events => &blueprint.events,
        GraphRef::Function(f) => {
            &blueprint
                .functions
                .iter()
                .find(|x| &x.name == f)
                .ok_or("No such function")?
                .graph
        }
    };
    for &id in &chosen {
        let node = graph.node(id).ok_or("A selected node is gone")?;
        let def = node_def(&node.def).ok_or("A selected node is unknown")?;
        if matches!(
            def.kind,
            DefKind::Event { .. } | DefKind::FunctionEntry | DefKind::Return
        ) {
            return Err(format!("\"{}\" can't go into a function", def.title));
        }
    }
    let inside = |pin: &PinRef| chosen.contains(&pin.node);
    let type_of = |pin: &PinRef, input: bool| -> PinType {
        let Some(node) = graph.node(pin.node) else {
            return PinType::Number;
        };
        let Some(def) = node_def(&node.def) else {
            return PinType::Number;
        };
        let p = pins(blueprint, which, node, def);
        let list = if input { &p.inputs } else { &p.outputs };
        list.iter()
            .find(|(n, _)| n == &pin.pin)
            .map_or(PinType::Number, |(_, t)| *t)
    };
    // Wires crossing into and out of the selection.
    let entering: Vec<&Wire> = graph
        .wires
        .iter()
        .filter(|w| !inside(&w.from) && inside(&w.to))
        .collect();
    let leaving: Vec<&Wire> = graph
        .wires
        .iter()
        .filter(|w| inside(&w.from) && !inside(&w.to))
        .collect();
    let exec_in: Vec<&&Wire> = entering
        .iter()
        .filter(|w| type_of(&w.to, true) == PinType::Exec)
        .collect();
    let exec_out: Vec<&&Wire> = leaving
        .iter()
        .filter(|w| type_of(&w.from, false) == PinType::Exec)
        .collect();
    if leaving.len() > exec_out.len() {
        return Err(
            "A value made inside is used outside; only exec may leave the selection".into(),
        );
    }
    let targets: BTreeSet<&PinRef> = exec_in.iter().map(|w| &w.to).collect();
    if targets.len() > 1
        || exec_out
            .iter()
            .map(|w| &w.to)
            .collect::<BTreeSet<_>>()
            .len()
            > 1
    {
        return Err("The selection must have one way in and one way out".into());
    }
    // Each value wired in from outside: a function input.
    let mut sources: Vec<(PinRef, PinType)> = Vec::new();
    for w in entering
        .iter()
        .filter(|w| type_of(&w.to, true) != PinType::Exec)
    {
        if !sources.iter().any(|(s, _)| s == &w.from) {
            sources.push((w.from.clone(), type_of(&w.from, false)));
        }
    }
    let input_name = |i: usize, s: &PinRef| format!("{}{}", s.pin.replace(' ', "_"), i + 1);
    let mut body = Graph::default();
    let entry_id = chosen.iter().max().copied().unwrap_or(0) + 1;
    let (min_x, min_y) = (graph.nodes.iter())
        .filter(|n| chosen.contains(&n.id))
        .fold((f32::MAX, f32::MAX), |(x, y), n| {
            (x.min(n.pos[0]), y.min(n.pos[1]))
        });
    body.nodes.push(GraphNode {
        id: entry_id,
        def: "flow.entry".into(),
        pos: [min_x - 220.0, min_y],
        values: Default::default(),
    });
    body.nodes.extend(
        graph
            .nodes
            .iter()
            .filter(|n| chosen.contains(&n.id))
            .cloned(),
    );
    body.wires.extend(
        graph
            .wires
            .iter()
            .filter(|w| inside(&w.from) && inside(&w.to))
            .cloned(),
    );
    if let Some(target) = targets.first() {
        body.wires.push(Wire {
            from: PinRef {
                node: entry_id,
                pin: "then".into(),
            },
            to: (*target).clone(),
        });
    }
    for w in entering
        .iter()
        .filter(|w| type_of(&w.to, true) != PinType::Exec)
    {
        let i = sources.iter().position(|(s, _)| s == &w.from).unwrap_or(0);
        body.wires.push(Wire {
            from: PinRef {
                node: entry_id,
                pin: input_name(i, &w.from),
            },
            to: w.to.clone(),
        });
    }
    let function = Function {
        name: name.to_owned(),
        inputs: sources
            .iter()
            .enumerate()
            .map(|(i, (s, t))| (input_name(i, s), *t))
            .collect(),
        output: None,
        graph: body,
    };
    // The call in their place.
    let call_id = graph.next_id().max(entry_id + 1);
    let mut call_wires: Vec<Wire> = Vec::new();
    for w in &exec_in {
        call_wires.push(Wire {
            from: w.from.clone(),
            to: PinRef {
                node: call_id,
                pin: EXEC_IN.into(),
            },
        });
    }
    if let Some(w) = exec_out.first() {
        call_wires.push(Wire {
            from: PinRef {
                node: call_id,
                pin: "then".into(),
            },
            to: w.to.clone(),
        });
    }
    for (i, (s, _)) in sources.iter().enumerate() {
        call_wires.push(Wire {
            from: s.clone(),
            to: PinRef {
                node: call_id,
                pin: input_name(i, s),
            },
        });
    }
    let graph = match which {
        GraphRef::Events => &mut blueprint.events,
        GraphRef::Function(f) => {
            &mut blueprint
                .functions
                .iter_mut()
                .find(|x| &x.name == f)
                .ok_or("No such function")?
                .graph
        }
    };
    graph.nodes.retain(|n| !chosen.contains(&n.id));
    graph
        .wires
        .retain(|w| !chosen.contains(&w.from.node) && !chosen.contains(&w.to.node));
    graph.nodes.push(GraphNode {
        id: call_id,
        def: "flow.call".into(),
        pos: [min_x, min_y],
        values: [("name".to_owned(), Literal::Text(name.to_owned()))]
            .into_iter()
            .collect(),
    });
    graph.wires.extend(call_wires);
    blueprint.functions.push(function);
    Ok(())
}
