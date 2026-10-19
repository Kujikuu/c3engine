//! A node's pins as an editor shows them: exec pins first, then data pins. Variables,
//! function calls, function starts and returns take their pins from what they name.

use super::compile::GraphRef;
use super::library::{DefKind, NodeDef, returns};
use super::model::{Blueprint, GraphNode, Literal, PinType};

/// The exec input every running node has.
pub const EXEC_IN: &str = "exec";

/// A node's inputs and outputs, in order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pins {
    pub inputs: Vec<(String, PinType)>,
    pub outputs: Vec<(String, PinType)>,
}

fn owned(pins: &[(&str, PinType)]) -> Vec<(String, PinType)> {
    pins.iter().map(|(n, t)| ((*n).to_owned(), *t)).collect()
}

/// The pins of `node` (of library node `def`) in graph `which` of `blueprint`.
pub fn pins(blueprint: &Blueprint, which: &GraphRef, node: &GraphNode, def: &NodeDef) -> Pins {
    let named = |key: &str| match node.values.get(key) {
        Some(Literal::Text(t)) => Some(t.as_str()),
        _ => None,
    };
    let variable = || named("name").and_then(|n| blueprint.variables.iter().find(|v| v.name == n));
    let function = || named("name").and_then(|n| blueprint.functions.iter().find(|f| f.name == n));
    let this_function = || match which {
        GraphRef::Function(name) => blueprint.functions.iter().find(|f| &f.name == name),
        GraphRef::Events => None,
    };
    let mut inputs = Vec::new();
    if def.runs() {
        inputs.push((EXEC_IN.to_owned(), PinType::Exec));
    }
    let mut outputs: Vec<(String, PinType)> = (def.exec_outs().iter())
        .map(|p| ((*p).to_owned(), PinType::Exec))
        .collect();
    match def.kind {
        DefKind::GetVariable => {
            outputs.push(("value".into(), variable().map_or(PinType::Number, |v| v.ty)));
        }
        DefKind::SetVariable => {
            inputs.push(("value".into(), variable().map_or(PinType::Number, |v| v.ty)));
        }
        DefKind::Call => {
            if let Some(f) = function() {
                inputs.extend(f.inputs.iter().cloned());
                if let Some(ty) = f.output {
                    outputs.push(("value".into(), ty));
                }
            }
        }
        DefKind::FunctionEntry => {
            if let Some(f) = this_function() {
                outputs.extend(f.inputs.iter().cloned());
            }
        }
        DefKind::Return => {
            let answer = match which {
                GraphRef::Function(_) => this_function().and_then(|f| f.output),
                // In the event graph: a formula's answer.
                GraphRef::Events => None,
            };
            if let Some(ty) = answer {
                inputs.push(("value".into(), ty));
            } else if matches!(which, GraphRef::Events) {
                inputs.push(("value".into(), PinType::Number));
            }
        }
        DefKind::Reroute => {
            let ty = named("type")
                .and_then(PinType::from_name)
                .unwrap_or(PinType::Number);
            inputs.push(("in".into(), ty));
            outputs.push(("out".into(), ty));
        }
        _ => {
            inputs.extend(owned(def.inputs));
            outputs.extend(owned(def.outputs));
        }
    }
    Pins { inputs, outputs }
}

/// What a formula event of the event graph gives back, for its Return nodes.
pub fn formula_answer(def: &NodeDef) -> Option<PinType> {
    match def.kind {
        DefKind::Event { function } => returns(function),
        _ => None,
    }
}
