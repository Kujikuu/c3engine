//! Dialogue graphs: an NPC's dialogue drawn as nodes instead of written as code. A *Say* node
//! does its actions, shows lines and offers answers, each leading to another node (or closing
//! the dialogue); a *Branch* node goes on to the first case whose condition holds. Graphs
//! live in `dialogues/*.dialogue.ron` and are turned into an ordinary script
//! (`scripts/dialogue/<name>.rhai`) that NPCs run like any other.
//!
//! Lines may name `{player}`, `{npc}`, `{level}` and `{money}`; they are filled in as the
//! dialogue runs.

use crate::quest::{DONE, NOT_TAKEN, Quest, TAKEN};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::io::{Error, ErrorKind, Result};
use std::path::Path;

/// File extension of dialogue graphs.
pub const DIALOGUE_SUFFIX: &str = ".dialogue.ron";

/// Folder (under `scripts/`) the scripts made from dialogues go to.
pub const GENERATED_DIR: &str = "dialogue";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DialogueGraph {
    /// The node `talk()` starts at.
    pub start: u32,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Node {
    pub id: u32,
    /// Where the editor draws it.
    pub pos: [f32; 2],
    /// A note for the editor (what the node is for).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    pub kind: NodeKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum NodeKind {
    /// Does `actions`, then the NPC says `lines` and offers `choices`.
    Say {
        #[serde(default)]
        actions: Vec<Action>,
        lines: Vec<String>,
        #[serde(default)]
        choices: Vec<Choice>,
    },
    /// Goes on to the first case whose condition holds, else to `otherwise`.
    Branch {
        cases: Vec<Case>,
        #[serde(default)]
        otherwise: Option<u32>,
    },
}

/// An answer the player can pick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Choice {
    pub label: String,
    /// Offered only when this holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<Condition>,
    /// The node it leads to; `None` closes the dialogue.
    #[serde(default)]
    pub next: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Case {
    pub condition: Condition,
    /// `None` closes the dialogue.
    pub next: Option<u32>,
}

/// Something true or false about the player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Condition {
    /// Level at least this.
    Level(i64),
    /// Money at least this.
    Money(i64),
    /// Carries at least `count` of `item`.
    Item { item: i64, count: i64 },
    /// Variable `name` equals `value`.
    Flag { name: String, value: i64 },
    /// The quest has not been taken.
    QuestNotTaken(i64),
    /// The quest is taken and not yet done.
    QuestTaken(i64),
    /// The quest is taken and its goal is met.
    QuestReady(i64),
    /// The quest is done.
    QuestDone(i64),
}

/// Something a Say node does to the player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Action {
    GiveMoney(i64),
    TakeMoney(i64),
    GiveItem { item: i64, count: i64 },
    TakeItem { item: i64, count: i64 },
    SetFlag { name: String, value: i64 },
    Message(String),
    Teleport { map: i64, x: i64, y: i64 },
    OpenShop,
    StartQuest(i64),
    CompleteQuest(i64),
}

impl Condition {
    /// Every kind of condition, as the editor offers them.
    pub const KINDS: [&str; 8] = [
        "Level at least",
        "Money at least",
        "Carries item",
        "Variable equals",
        "Quest not taken",
        "Quest taken",
        "Quest ready",
        "Quest done",
    ];

    /// A condition of kind `KINDS[index]` with default values.
    pub fn of_kind(index: usize) -> Self {
        match index {
            0 => Self::Level(1),
            1 => Self::Money(1),
            2 => Self::Item { item: 0, count: 1 },
            3 => Self::Flag {
                name: "step".into(),
                value: 1,
            },
            4 => Self::QuestNotTaken(1),
            5 => Self::QuestTaken(1),
            6 => Self::QuestReady(1),
            _ => Self::QuestDone(1),
        }
    }

    /// Index of this condition's kind in `KINDS`.
    pub fn kind(&self) -> usize {
        match self {
            Self::Level(_) => 0,
            Self::Money(_) => 1,
            Self::Item { .. } => 2,
            Self::Flag { .. } => 3,
            Self::QuestNotTaken(_) => 4,
            Self::QuestTaken(_) => 5,
            Self::QuestReady(_) => 6,
            Self::QuestDone(_) => 7,
        }
    }

    /// The condition as Rhai.
    fn code(&self) -> String {
        let state = |q: &i64| format!("quest_state({q})");
        match self {
            Self::Level(n) => format!("level() >= {n}"),
            Self::Money(n) => format!("money() >= {n}"),
            Self::Item { item, count } => format!("has_item({item}, {count})"),
            Self::Flag { name, value } => format!("flag({}) == {value}", string(name)),
            Self::QuestNotTaken(q) => format!("{} == {NOT_TAKEN}", state(q)),
            Self::QuestTaken(q) => format!("{} == {TAKEN}", state(q)),
            Self::QuestReady(q) => format!("quest_ready({q})"),
            Self::QuestDone(q) => format!("{} == {DONE}", state(q)),
        }
    }
}

impl Action {
    /// Every kind of action, as the editor offers them.
    pub const KINDS: [&str; 10] = [
        "Give money",
        "Take money",
        "Give item",
        "Take item",
        "Set variable",
        "System message",
        "Teleport",
        "Open shop",
        "Start quest",
        "Complete quest",
    ];

    /// An action of kind `KINDS[index]` with default values.
    pub fn of_kind(index: usize) -> Self {
        match index {
            0 => Self::GiveMoney(100),
            1 => Self::TakeMoney(100),
            2 => Self::GiveItem { item: 0, count: 1 },
            3 => Self::TakeItem { item: 0, count: 1 },
            4 => Self::SetFlag {
                name: "step".into(),
                value: 1,
            },
            5 => Self::Message(String::new()),
            6 => Self::Teleport { map: 0, x: 0, y: 0 },
            7 => Self::OpenShop,
            8 => Self::StartQuest(1),
            _ => Self::CompleteQuest(1),
        }
    }

    /// Index of this action's kind in `KINDS`.
    pub fn kind(&self) -> usize {
        match self {
            Self::GiveMoney(_) => 0,
            Self::TakeMoney(_) => 1,
            Self::GiveItem { .. } => 2,
            Self::TakeItem { .. } => 3,
            Self::SetFlag { .. } => 4,
            Self::Message(_) => 5,
            Self::Teleport { .. } => 6,
            Self::OpenShop => 7,
            Self::StartQuest(_) => 8,
            Self::CompleteQuest(_) => 9,
        }
    }

    /// The action as a Rhai statement.
    fn code(&self) -> String {
        match self {
            Self::GiveMoney(n) => format!("give_money({n});"),
            Self::TakeMoney(n) => format!("take_money({n});"),
            Self::GiveItem { item, count } => format!("give_item({item}, {count});"),
            Self::TakeItem { item, count } => format!("take_item({item}, {count});"),
            Self::SetFlag { name, value } => format!("set_flag({}, {value});", string(name)),
            Self::Message(text) => format!("message({});", line(text)),
            Self::Teleport { map, x, y } => format!("teleport({map}, {x}, {y});"),
            Self::OpenShop => "open_shop();".into(),
            Self::StartQuest(q) => format!("start_quest({q});"),
            Self::CompleteQuest(q) => format!("complete_quest({q});"),
        }
    }
}

/// `text` as a Rhai string literal.
fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Placeholders lines may use, and the function filling each in.
pub const PLACEHOLDERS: [(&str, &str); 4] = [
    ("{player}", "player_name()"),
    ("{npc}", "npc_name()"),
    ("{level}", "level()"),
    ("{money}", "money()"),
];

/// Placeholder functions that give text.
const TEXT_CALLS: [&str; 2] = ["player_name()", "npc_name()"];

/// A line as a Rhai string expression, its placeholders filled in.
fn line(text: &str) -> String {
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some((at, (name, call))) = PLACEHOLDERS
        .iter()
        .filter_map(|p| rest.find(p.0).map(|i| (i, *p)))
        .min_by_key(|(i, _)| *i)
    {
        if at > 0 {
            parts.push(string(&rest[..at]));
        }
        parts.push(call.to_owned());
        rest = &rest[at + name.len()..];
    }
    if !rest.is_empty() || parts.is_empty() {
        parts.push(string(rest));
    }
    // Starting with text makes `+` join text (`level()` alone is a number).
    if !parts[0].starts_with('"') && !TEXT_CALLS.contains(&parts[0].as_str()) {
        parts.insert(0, "\"\"".into());
    }
    parts.join(" + ")
}

/// The script function of node `id`.
fn function(id: u32) -> String {
    format!("node_{id}")
}

/// Something wrong with a graph, found by [`DialogueGraph::problems`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphProblem {
    /// The node it is in (`None`: the whole graph).
    pub node: Option<u32>,
    pub message: String,
}

impl Default for DialogueGraph {
    /// A greeting and a goodbye.
    fn default() -> Self {
        Self {
            start: 1,
            nodes: vec![Node {
                id: 1,
                pos: [40.0, 40.0],
                title: "Greeting".into(),
                kind: NodeKind::Say {
                    actions: Vec::new(),
                    lines: vec!["Hello, {player}.".into()],
                    choices: vec![Choice {
                        label: "Goodbye".into(),
                        condition: None,
                        next: None,
                    }],
                },
            }],
        }
    }
}

impl NodeKind {
    /// A Say node with one line and a goodbye.
    pub fn say(text: &str) -> Self {
        Self::Say {
            actions: Vec::new(),
            lines: vec![text.to_owned()],
            choices: vec![Choice {
                label: "Goodbye".into(),
                condition: None,
                next: None,
            }],
        }
    }

    /// The nodes this one leads to (unset links skipped).
    pub fn links(&self) -> Vec<u32> {
        match self {
            Self::Say { choices, .. } => choices.iter().filter_map(|c| c.next).collect(),
            Self::Branch { cases, otherwise } => cases
                .iter()
                .filter_map(|c| c.next)
                .chain(*otherwise)
                .collect(),
        }
    }

    /// Every link of this node, to change it (`None`: closes the dialogue).
    pub fn links_mut(&mut self) -> Vec<&mut Option<u32>> {
        match self {
            Self::Say { choices, .. } => choices.iter_mut().map(|c| &mut c.next).collect(),
            Self::Branch { cases, otherwise } => cases
                .iter_mut()
                .map(|c| &mut c.next)
                .chain(std::iter::once(otherwise))
                .collect(),
        }
    }
}

impl DialogueGraph {
    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: u32) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// An id no node uses.
    pub fn next_id(&self) -> u32 {
        self.nodes.iter().map(|n| n.id).max().map_or(1, |m| m + 1)
    }

    /// Adds a node of `kind` at `pos`; returns its id.
    pub fn add(&mut self, kind: NodeKind, pos: [f32; 2]) -> u32 {
        let id = self.next_id();
        self.nodes.push(Node {
            id,
            pos,
            title: String::new(),
            kind,
        });
        id
    }

    /// Removes node `id` and every link to it.
    pub fn remove(&mut self, id: u32) {
        self.nodes.retain(|n| n.id != id);
        for n in &mut self.nodes {
            for link in n.kind.links_mut() {
                if *link == Some(id) {
                    *link = None;
                }
            }
        }
    }

    /// Everything wrong with the graph: no start, links to missing nodes, nodes that say
    /// nothing, nodes nothing leads to, branches that loop without saying anything.
    pub fn problems(&self) -> Vec<GraphProblem> {
        let mut out = Vec::new();
        let at = |node, message: String| GraphProblem {
            node: Some(node),
            message,
        };
        if self.node(self.start).is_none() {
            out.push(GraphProblem {
                node: None,
                message: "the start node is missing".into(),
            });
        }
        let mut linked = vec![self.start];
        linked.extend(self.nodes.iter().flat_map(|n| n.kind.links()));
        for n in &self.nodes {
            for link in n.kind.links() {
                if self.node(link).is_none() {
                    out.push(at(
                        n.id,
                        format!("leads to node {link}, which doesn't exist"),
                    ));
                }
            }
            if let NodeKind::Say { lines, choices, .. } = &n.kind
                && lines.iter().all(|l| l.trim().is_empty())
                && choices.is_empty()
            {
                out.push(at(n.id, "says nothing, so the dialogue just closes".into()));
            }
            if !linked.contains(&n.id) {
                out.push(at(n.id, "nothing leads here".into()));
            }
        }
        for n in &self.nodes {
            if matches!(n.kind, NodeKind::Branch { .. }) && self.branch_loops(n.id) {
                out.push(at(
                    n.id,
                    "branches lead back here without saying anything".into(),
                ));
            }
        }
        out
    }

    /// Whether following only branch nodes from `from` comes back to it.
    fn branch_loops(&self, from: u32) -> bool {
        let mut seen = Vec::new();
        let mut todo = vec![from];
        while let Some(id) = todo.pop() {
            let Some(NodeKind::Branch { .. }) = self.node(id).map(|n| &n.kind) else {
                continue;
            };
            for link in self.node(id).map(|n| n.kind.links()).unwrap_or_default() {
                if link == from {
                    return true;
                }
                if !seen.contains(&link) {
                    seen.push(link);
                    todo.push(link);
                }
            }
        }
        false
    }

    /// The graph as a script; `name` is the dialogue it came from.
    pub fn to_script(&self, name: &str) -> String {
        let mut s = format!(
            "// Made from dialogues/{name}{DIALOGUE_SUFFIX} by the Studio's dialogue editor.\n\
             // Edit the dialogue there: this file is rewritten whenever it is saved.\n\n"
        );
        let call = |next: Option<u32>| match next.filter(|&n| self.node(n).is_some()) {
            Some(n) => format!("{}();", function(n)),
            None => "// The dialogue closes.".to_owned(),
        };
        let _ = writeln!(s, "fn talk() {{\n    {}\n}}", call(Some(self.start)));
        for n in &self.nodes {
            let _ = write!(s, "\nfn {}() {{\n", function(n.id));
            if !n.title.is_empty() {
                let _ = writeln!(s, "    // {}", n.title.replace('\n', " "));
            }
            match &n.kind {
                NodeKind::Say {
                    actions,
                    lines,
                    choices,
                } => {
                    for a in actions {
                        let _ = writeln!(s, "    {}", a.code());
                    }
                    for l in lines.iter().filter(|l| !l.trim().is_empty()) {
                        let _ = writeln!(s, "    text({});", line(l));
                    }
                    for c in choices {
                        let option = match c.next.filter(|&n| self.node(n).is_some()) {
                            Some(next) => {
                                format!("option({}, \"{}\");", line(&c.label), function(next))
                            }
                            None => format!("option({});", line(&c.label)),
                        };
                        match &c.condition {
                            Some(cond) => {
                                let _ = writeln!(s, "    if {} {{ {option} }}", cond.code());
                            }
                            None => {
                                let _ = writeln!(s, "    {option}");
                            }
                        }
                    }
                }
                NodeKind::Branch { cases, otherwise } => {
                    for (i, c) in cases.iter().enumerate() {
                        let keyword = if i == 0 { "if" } else { "} else if" };
                        let _ = writeln!(s, "    {keyword} {} {{", c.condition.code());
                        let _ = writeln!(s, "        {}", call(c.next));
                    }
                    if cases.is_empty() {
                        let _ = writeln!(s, "    {}", call(*otherwise));
                    } else {
                        let _ = writeln!(s, "    }} else {{\n        {}\n    }}", call(*otherwise));
                    }
                }
            }
            s.push_str("}\n");
        }
        s
    }

    /// The usual dialogue of an NPC giving `quest`: offer it (if the player's level allows),
    /// remind while it runs, reward when it's ready, thank once it's done.
    pub fn quest_giver(quest: &Quest) -> Self {
        let q = quest.id;
        let col = |x: f32, y: f32| [40.0 + x * 300.0, 40.0 + y * 170.0];
        let say = |id, pos, title: &str, actions, lines: Vec<String>, choices| Node {
            id,
            pos,
            title: title.to_owned(),
            kind: NodeKind::Say {
                actions,
                lines,
                choices,
            },
        };
        let answer = |label: &str, next| Choice {
            label: label.to_owned(),
            condition: None,
            next,
        };
        let case = |condition, next| Case {
            condition,
            next: Some(next),
        };
        let about = if quest.description.is_empty() {
            format!("It is about {}.", quest.name)
        } else {
            quest.description.clone()
        };
        Self {
            start: 1,
            nodes: vec![
                Node {
                    id: 1,
                    pos: col(0.0, 1.5),
                    title: format!("Where is the player with \"{}\"?", quest.name),
                    kind: NodeKind::Branch {
                        cases: vec![
                            case(Condition::QuestDone(q), 2),
                            case(Condition::QuestReady(q), 3),
                            case(Condition::QuestTaken(q), 4),
                            case(Condition::Level(quest.min_level.max(1)), 5),
                        ],
                        otherwise: Some(6),
                    },
                },
                say(
                    2,
                    col(1.0, 0.0),
                    "Done",
                    Vec::new(),
                    vec!["Thank you again for your help, {player}.".into()],
                    vec![answer("Goodbye", None)],
                ),
                say(
                    3,
                    col(1.0, 1.0),
                    "Reward",
                    vec![Action::CompleteQuest(q)],
                    vec!["You did it, {player}! Here is your reward.".into()],
                    vec![answer("Thank you", None)],
                ),
                say(
                    4,
                    col(1.0, 2.0),
                    "Reminder",
                    Vec::new(),
                    vec![about.clone(), "Come back when it is done.".into()],
                    vec![answer("I'm on it", None)],
                ),
                say(
                    5,
                    col(1.0, 3.0),
                    "Offer",
                    Vec::new(),
                    vec!["{player}, I need your help.".into(), about],
                    vec![answer("I'll do it", Some(7)), answer("Not now", None)],
                ),
                say(
                    6,
                    col(1.0, 4.0),
                    "Too weak",
                    Vec::new(),
                    vec![format!(
                        "Come back when you are level {}, {{player}}.",
                        quest.min_level
                    )],
                    vec![answer("Goodbye", None)],
                ),
                say(
                    7,
                    col(2.0, 3.0),
                    "Accepted",
                    vec![Action::StartQuest(q)],
                    vec!["Thank you! Come back to me when it is done.".into()],
                    vec![answer("Goodbye", None)],
                ),
            ],
        }
    }
}

/// The file form of a dialogue.
#[derive(Serialize, Deserialize)]
struct DialogueFile {
    format: u32,
    dialogue: DialogueGraph,
}

const FORMAT_VERSION: u32 = 1;

fn invalid(file: &Path, why: impl std::fmt::Display) -> Error {
    Error::new(ErrorKind::InvalidData, format!("{}: {why}", file.display()))
}

/// Writes `graph` to `file`, through a temporary file.
pub fn save_dialogue(graph: &DialogueGraph, file: &Path) -> Result<()> {
    let text = DialogueFile {
        format: FORMAT_VERSION,
        dialogue: graph.clone(),
    };
    let ron = ron::ser::to_string_pretty(&text, ron::ser::PrettyConfig::default())
        .map_err(|e| invalid(file, e))?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension("tmp");
    std::fs::write(&tmp, ron)?;
    std::fs::rename(&tmp, file)
}

/// Reads the dialogue at `file`.
pub fn load_dialogue(file: &Path) -> Result<DialogueGraph> {
    let text = std::fs::read_to_string(file)?;
    let parsed: DialogueFile = ron::from_str(&text).map_err(|e| invalid(file, e))?;
    if parsed.format > FORMAT_VERSION {
        return Err(invalid(
            file,
            format!(
                "format {} is newer than this engine supports",
                parsed.format
            ),
        ));
    }
    Ok(parsed.dialogue)
}

/// Dialogue names under `dir` (`guard` for `dir/guard.dialogue.ron`), sorted.
pub fn dialogue_names(dir: &Path) -> Vec<String> {
    crate::host::names_with_suffix(dir, DIALOGUE_SUFFIX)
}

/// The script made from dialogue `name`.
pub fn generated_script(name: &str) -> String {
    format!("{GENERATED_DIR}/{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_fill_in_placeholders() {
        assert_eq!(line("Hi"), "\"Hi\"");
        assert_eq!(line("Hi {player}!"), "\"Hi \" + player_name() + \"!\"");
        assert_eq!(line("{level}"), "\"\" + level()");
        assert_eq!(line("{player}!"), "player_name() + \"!\"");
        assert_eq!(line("say \"x\"\n"), "\"say \\\"x\\\"\\n\"");
        assert_eq!(line(""), "\"\"");
    }

    #[test]
    fn finds_problems() {
        let mut g = DialogueGraph::default();
        assert!(g.problems().is_empty(), "{:?}", g.problems());
        let lonely = g.add(NodeKind::say("Hm"), [0.0, 0.0]);
        if let NodeKind::Say { choices, .. } = &mut g.nodes[0].kind {
            choices[0].next = Some(99);
        }
        let p = g.problems();
        assert!(
            p.iter()
                .any(|p| p.node == Some(1) && p.message.contains("99"))
        );
        assert!(p.iter().any(|p| p.node == Some(lonely)));
        g.remove(lonely);
        let loop_a = g.add(
            NodeKind::Branch {
                cases: Vec::new(),
                otherwise: None,
            },
            [0.0, 0.0],
        );
        let loop_b = g.add(
            NodeKind::Branch {
                cases: Vec::new(),
                otherwise: Some(loop_a),
            },
            [0.0, 0.0],
        );
        if let NodeKind::Branch { otherwise, .. } = &mut g.node_mut(loop_a).unwrap().kind {
            *otherwise = Some(loop_b);
        }
        assert!(g.problems().iter().any(|p| p.message.contains("back here")));
    }

    #[test]
    fn removing_a_node_unlinks_it() {
        let mut g = DialogueGraph::default();
        let next = g.add(NodeKind::say("Bye"), [0.0, 0.0]);
        if let NodeKind::Say { choices, .. } = &mut g.nodes[0].kind {
            choices[0].next = Some(next);
        }
        g.remove(next);
        assert_eq!(g.nodes[0].kind.links(), Vec::<u32>::new());
    }
}
