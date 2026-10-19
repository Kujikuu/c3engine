//! A blueprint as it is stored (`scripts/blueprints/<name>.c3bp`, RON): its variables, its
//! event graphs and its functions, each a set of nodes wired pin to pin.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What flows along a wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PinType {
    /// The order nodes run in (white wires).
    Exec,
    Number,
    Text,
    Bool,
    /// A player, monster or NPC (its id).
    Entity,
    /// An item type.
    Item,
    /// A map id.
    Map,
    /// A list of values.
    List,
    /// Named numbers: a fighter's stats (`level`, `attack_min`, ...) or a skill's.
    Record,
}

impl PinType {
    /// Whether a wire may carry a `self` value into a pin of type `to`.
    pub fn fits(self, to: PinType) -> bool {
        self == to
            || matches!(
                (self, to),
                // Ids are numbers.
                (Self::Entity | Self::Item | Self::Map, Self::Number)
                    | (Self::Number, Self::Entity | Self::Item | Self::Map)
                    // Anything reads as text.
                    | (Self::Number | Self::Bool | Self::Entity | Self::Item | Self::Map, Self::Text)
            )
    }

    /// The type called `name` (as [`Self::name`] gives it).
    pub fn from_name(name: &str) -> Option<Self> {
        [
            Self::Exec,
            Self::Number,
            Self::Text,
            Self::Bool,
            Self::Entity,
            Self::Item,
            Self::Map,
            Self::List,
            Self::Record,
        ]
        .into_iter()
        .find(|t| t.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Exec => "exec",
            Self::Number => "number",
            Self::Text => "text",
            Self::Bool => "bool",
            Self::Entity => "entity",
            Self::Item => "item",
            Self::Map => "map",
            Self::List => "list",
            Self::Record => "record",
        }
    }
}

/// A value typed into an unwired pin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Number(f64),
    Text(String),
    Bool(bool),
}

/// One node: which library node it is, where it sits, and the values of unwired inputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: u32,
    /// The library node (`player.give_money`, `flow.branch`, `event.login`).
    pub def: String,
    pub pos: [f32; 2],
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, Literal>,
}

/// A pin of a node.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PinRef {
    pub node: u32,
    pub pin: String,
}

/// A wire from an output pin to an input pin.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Wire {
    pub from: PinRef,
    pub to: PinRef,
}

/// Nodes and the wires between them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    #[serde(default)]
    pub wires: Vec<Wire>,
}

impl Graph {
    pub fn node(&self, id: u32) -> Option<&GraphNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// The wire into input `pin` of `node`, if one.
    pub fn wire_into(&self, node: u32, pin: &str) -> Option<&Wire> {
        (self.wires.iter()).find(|w| w.to.node == node && w.to.pin == pin)
    }

    /// The wires out of output `pin` of `node`.
    pub fn wires_from<'a>(&'a self, node: u32, pin: &'a str) -> impl Iterator<Item = &'a Wire> {
        (self.wires.iter()).filter(move |w| w.from.node == node && w.from.pin == pin)
    }

    /// A free node id.
    pub fn next_id(&self) -> u32 {
        self.nodes.iter().map(|n| n.id).max().map_or(1, |m| m + 1)
    }
}

/// Where a variable lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VariableScope {
    /// While one event or function runs.
    Local,
    /// Kept with the player (saved with the character).
    Player,
    /// Shared by the whole world (every player's graphs see it).
    World,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub ty: PinType,
    pub scope: VariableScope,
    pub default: Literal,
}

/// A function of the blueprint: its inputs (outputs of its entry node), what it returns,
/// and its graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    #[serde(default)]
    pub inputs: Vec<(String, PinType)>,
    /// What it gives back, if anything (a pure function when it has no exec pins).
    #[serde(default)]
    pub output: Option<PinType>,
    pub graph: Graph,
}

/// A blueprint: what a game rule or behaviour is, made of graphs instead of code.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Blueprint {
    #[serde(default)]
    pub variables: Vec<Variable>,
    /// The event graph: event nodes and what they do.
    #[serde(default)]
    pub events: Graph,
    #[serde(default)]
    pub functions: Vec<Function>,
}
