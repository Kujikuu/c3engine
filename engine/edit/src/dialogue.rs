//! Dialogue graph editing: commands that add, change and remove nodes and pick the start
//! node, applied by [`DialogueDoc`] with undo. Editors that change a graph freely (the
//! Studio's canvas) hand the result to [`DialogueDoc::change_to`], which records it as the
//! commands that turn one graph into the other.

use crate::history::{History, HistoryEntry};
use crate::{EditError, Outcome};
use c3_script::{DialogueGraph, Node};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A change to a dialogue. Serialized as `{"op": "set_node", "node": {...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DialogueCommand {
    /// Adds `node` (its id must be free).
    AddNode { node: Node },
    /// Replaces the node with `node`'s id.
    SetNode { node: Node },
    /// Removes node `id`; links to it are left for the graph's problems to point out.
    RemoveNode { id: u32 },
    /// Makes node `id` the one `talk()` starts at.
    SetStart { id: u32 },
}

struct Change {
    before: DialogueGraph,
    after: DialogueGraph,
}

/// A dialogue open for editing: every change goes through [`DialogueDoc::execute`].
pub struct DialogueDoc {
    graph: DialogueGraph,
    history: History<Change>,
}

impl DialogueDoc {
    pub fn new(graph: DialogueGraph) -> Self {
        Self {
            graph,
            history: History::default(),
        }
    }

    pub fn graph(&self) -> &DialogueGraph {
        &self.graph
    }

    /// Applies `command`, recording it for undo. Refused commands change nothing.
    pub fn execute(&mut self, command: &DialogueCommand) -> Result<Outcome, EditError> {
        let mut after = self.graph.clone();
        let summary = apply(&mut after, command)?;
        if after == self.graph {
            return Ok(Outcome {
                summary: "Nothing changed".into(),
                changed: false,
            });
        }
        let before = std::mem::replace(&mut self.graph, after.clone());
        self.history
            .record(summary.clone(), Change { before, after });
        Ok(Outcome {
            summary,
            changed: true,
        })
    }

    /// Records the edits that make the graph `graph` as one step: the commands from the
    /// current graph to it, named after what they do.
    pub fn change_to(&mut self, graph: &DialogueGraph) -> Vec<DialogueCommand> {
        let commands = diff(&self.graph, graph);
        if commands.is_empty() {
            return commands;
        }
        self.history.begin_group(describe(&commands));
        for command in &commands {
            // The diff only names nodes that exist on the side it reads them from.
            let _ = self.execute(command);
        }
        self.history.end_group();
        commands
    }

    // ---- history ----

    /// Reverts the last step; returns its label.
    pub fn undo(&mut self) -> Option<String> {
        let step = self.history.take_undo()?;
        if let Some(first) = step.changes.first() {
            self.graph = first.before.clone();
        }
        let label = step.label.clone();
        self.history.undone(step);
        Some(label)
    }

    /// Reapplies the last undone step; returns its label.
    pub fn redo(&mut self) -> Option<String> {
        let step = self.history.take_redo()?;
        if let Some(last) = step.changes.last() {
            self.graph = last.after.clone();
        }
        let label = step.label.clone();
        self.history.redone(step);
        Some(label)
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.history.undo_label()
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.history.redo_label()
    }

    pub fn history(&self) -> Vec<HistoryEntry> {
        self.history.entries()
    }

    pub fn is_dirty(&self) -> bool {
        self.history.is_dirty()
    }

    pub fn mark_saved(&mut self) {
        self.history.mark_saved();
    }
}

fn apply(graph: &mut DialogueGraph, command: &DialogueCommand) -> Result<String, EditError> {
    let index = |graph: &DialogueGraph, id: u32| graph.nodes.iter().position(|n| n.id == id);
    match command {
        DialogueCommand::AddNode { node } => {
            if index(graph, node.id).is_some() {
                return Err(EditError::NodeIdTaken(node.id));
            }
            graph.nodes.push(node.clone());
            Ok(format!("Add node {}", node.id))
        }
        DialogueCommand::SetNode { node } => {
            let i = index(graph, node.id).ok_or(EditError::NoNode(node.id))?;
            graph.nodes[i] = node.clone();
            Ok(format!("Change node {}", node.id))
        }
        DialogueCommand::RemoveNode { id } => {
            let i = index(graph, *id).ok_or(EditError::NoNode(*id))?;
            graph.nodes.remove(i);
            Ok(format!("Remove node {id}"))
        }
        DialogueCommand::SetStart { id } => {
            index(graph, *id).ok_or(EditError::NoNode(*id))?;
            graph.start = *id;
            Ok(format!("Start at node {id}"))
        }
    }
}

/// A step's name: the one command's, or how many nodes it touched.
fn describe(commands: &[DialogueCommand]) -> String {
    match commands {
        [DialogueCommand::AddNode { node }] => format!("Add node {}", node.id),
        [DialogueCommand::SetNode { node }] => format!("Change node {}", node.id),
        [DialogueCommand::RemoveNode { id }] => format!("Remove node {id}"),
        [DialogueCommand::SetStart { id }] => format!("Start at node {id}"),
        many => format!("Change {} nodes", many.len()),
    }
}

/// The commands that turn `from` into `to` (nodes keep their order of `to`'s ids).
fn diff(from: &DialogueGraph, to: &DialogueGraph) -> Vec<DialogueCommand> {
    let mut commands: Vec<DialogueCommand> = (from.nodes.iter())
        .filter(|n| to.node(n.id).is_none())
        .map(|n| DialogueCommand::RemoveNode { id: n.id })
        .collect();
    for node in &to.nodes {
        match from.node(node.id) {
            None => commands.push(DialogueCommand::AddNode { node: node.clone() }),
            Some(old) if old != node => {
                commands.push(DialogueCommand::SetNode { node: node.clone() })
            }
            Some(_) => {}
        }
    }
    if from.start != to.start {
        commands.push(DialogueCommand::SetStart { id: to.start });
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_script::NodeKind;

    fn say(id: u32, line: &str) -> Node {
        Node {
            id,
            pos: [0.0, 0.0],
            title: String::new(),
            kind: NodeKind::Say {
                actions: Vec::new(),
                lines: vec![line.into()],
                choices: Vec::new(),
            },
        }
    }

    #[test]
    fn commands_and_free_edits_undo_as_steps() {
        let mut doc = DialogueDoc::new(DialogueGraph {
            start: 1,
            nodes: vec![say(1, "Hello")],
        });
        doc.execute(&DialogueCommand::AddNode {
            node: say(2, "Bye"),
        })
        .unwrap();
        assert!(
            doc.execute(&DialogueCommand::AddNode { node: say(2, "x") })
                .is_err()
        );
        doc.execute(&DialogueCommand::SetStart { id: 2 }).unwrap();
        assert_eq!(doc.graph().start, 2);
        // A canvas edit: node 1 changed and node 3 added, one step.
        let mut edited = doc.graph().clone();
        edited.nodes[0] = say(1, "Hi");
        edited.nodes.push(say(3, "More"));
        let commands = doc.change_to(&edited);
        assert_eq!(commands.len(), 2);
        assert_eq!(doc.graph(), &edited);
        assert_eq!(doc.undo().as_deref(), Some("Change 2 nodes"));
        assert_eq!(doc.graph().nodes.len(), 2);
        doc.undo();
        assert_eq!(doc.graph().start, 1);
        doc.redo();
        doc.redo();
        assert_eq!(doc.graph(), &edited);
        assert!(doc.is_dirty());
    }
}
