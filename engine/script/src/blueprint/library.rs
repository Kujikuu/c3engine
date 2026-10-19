//! The nodes graphs are made of. Each says its pins and the Rhai it becomes: `{pin}` stands
//! for what flows into input `pin`. Events become the script functions the game calls;
//! actions run in exec order; pure nodes are expressions, worked out where they are used.

use super::model::PinType::{self, Bool, Entity, Item, List, Map, Number, Text};

/// How a node takes part in a graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefKind {
    /// Where an event's graph starts: the script function `function`, its parameters the
    /// node's outputs.
    Event {
        function: &'static str,
    },
    /// Runs in exec order (exec in, exec out `then`); its outputs hold what it gave back.
    Action,
    /// An expression, worked out where it is used.
    Pure,
    /// `if cond`: exec outs `true` and `false`.
    Branch,
    /// Exec outs `then 0`, `then 1`, ... in turn.
    Sequence,
    /// Runs exec out `body` for each element of `list` (output `item`), then `done`.
    ForEach,
    /// Waits `seconds`, then goes on (`then`) – the rest runs later, from a timer.
    Wait,
    /// Reads or sets the variable named by the node's `name` value.
    GetVariable,
    SetVariable,
    /// Calls the blueprint function named by the node's `name` value.
    Call,
    /// Ends a function, giving back `value`.
    Return,
    /// A function graph's start; its outputs are the function's inputs.
    FunctionEntry,
}

/// A library node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeDef {
    pub id: &'static str,
    pub title: &'static str,
    /// The palette group.
    pub category: &'static str,
    pub kind: DefKind,
    pub inputs: &'static [(&'static str, PinType)],
    pub outputs: &'static [(&'static str, PinType)],
    /// The Rhai: an expression (pure nodes, and actions giving back one output) or a
    /// statement (actions without outputs).
    pub code: &'static str,
}

impl NodeDef {
    /// The exec outputs it continues through.
    pub fn exec_outs(&self) -> &'static [&'static str] {
        match self.kind {
            DefKind::Event { .. } | DefKind::Action | DefKind::Wait | DefKind::SetVariable => {
                &["then"]
            }
            DefKind::Call | DefKind::FunctionEntry => &["then"],
            DefKind::Branch => &["true", "false"],
            DefKind::Sequence => &["then 0", "then 1", "then 2", "then 3"],
            DefKind::ForEach => &["body", "done"],
            DefKind::Pure | DefKind::GetVariable | DefKind::Return => &[],
        }
    }

    /// Whether it has an exec input (it runs when reached).
    pub fn runs(&self) -> bool {
        !matches!(
            self.kind,
            DefKind::Event { .. } | DefKind::Pure | DefKind::GetVariable | DefKind::FunctionEntry
        )
    }

    pub fn input(&self, pin: &str) -> Option<PinType> {
        self.inputs.iter().find(|(n, _)| *n == pin).map(|(_, t)| *t)
    }

    pub fn output(&self, pin: &str) -> Option<PinType> {
        self.outputs
            .iter()
            .find(|(n, _)| *n == pin)
            .map(|(_, t)| *t)
    }
}

const fn event(
    id: &'static str,
    title: &'static str,
    function: &'static str,
    outputs: &'static [(&'static str, PinType)],
) -> NodeDef {
    NodeDef {
        id,
        title,
        category: "Events",
        kind: DefKind::Event { function },
        inputs: &[],
        outputs,
        code: "",
    }
}

const fn action(
    id: &'static str,
    title: &'static str,
    category: &'static str,
    inputs: &'static [(&'static str, PinType)],
    outputs: &'static [(&'static str, PinType)],
    code: &'static str,
) -> NodeDef {
    NodeDef {
        id,
        title,
        category,
        kind: DefKind::Action,
        inputs,
        outputs,
        code,
    }
}

const fn pure(
    id: &'static str,
    title: &'static str,
    category: &'static str,
    inputs: &'static [(&'static str, PinType)],
    output: PinType,
    code: &'static str,
) -> NodeDef {
    NodeDef {
        id,
        title,
        category,
        kind: DefKind::Pure,
        inputs,
        outputs: match output {
            Number => &[("value", Number)],
            Text => &[("value", Text)],
            Bool => &[("value", Bool)],
            Entity => &[("value", Entity)],
            Item => &[("value", Item)],
            Map => &[("value", Map)],
            List => &[("value", List)],
            PinType::Exec => &[],
        },
        code,
    }
}

const fn flow(
    id: &'static str,
    title: &'static str,
    kind: DefKind,
    inputs: &'static [(&'static str, PinType)],
    outputs: &'static [(&'static str, PinType)],
) -> NodeDef {
    NodeDef {
        id,
        title,
        category: "Flow",
        kind,
        inputs,
        outputs,
        code: "",
    }
}

/// Every node, in palette order.
pub const NODES: &[NodeDef] = &[
    // Events: the hooks the game calls.
    event("event.login", "On login", "on_login", &[]),
    event(
        "event.enter_map",
        "On entering a map",
        "on_enter",
        &[("map", Map)],
    ),
    event(
        "event.kill",
        "On killing a monster",
        "on_kill",
        &[("monster", Number)],
    ),
    event(
        "event.use_item",
        "On using an item",
        "on_use",
        &[("item", Item)],
    ),
    event(
        "event.enter_region",
        "On entering a region",
        "on_enter_region",
        &[("region", Number)],
    ),
    event(
        "event.leave_region",
        "On leaving a region",
        "on_leave_region",
        &[("region", Number)],
    ),
    event("event.timer", "On a timer", "on_timer", &[("name", Text)]),
    event("event.talk", "On talking (an NPC)", "talk", &[]),
    // A function of its own name (an answer's `next`, or one the game calls).
    event("event.custom", "Custom event", "", &[]),
    // Flow.
    flow(
        "flow.branch",
        "Branch",
        DefKind::Branch,
        &[("condition", Bool)],
        &[],
    ),
    flow("flow.sequence", "Sequence", DefKind::Sequence, &[], &[]),
    flow(
        "flow.for_each",
        "For each",
        DefKind::ForEach,
        &[("list", List)],
        &[("item", Number)],
    ),
    flow(
        "flow.wait",
        "Wait",
        DefKind::Wait,
        &[("seconds", Number)],
        &[],
    ),
    flow(
        "flow.get",
        "Get variable",
        DefKind::GetVariable,
        &[],
        &[("value", Number)],
    ),
    flow(
        "flow.set",
        "Set variable",
        DefKind::SetVariable,
        &[("value", Number)],
        &[],
    ),
    flow("flow.call", "Call function", DefKind::Call, &[], &[]),
    flow(
        "flow.return",
        "Return",
        DefKind::Return,
        &[("value", Number)],
        &[],
    ),
    flow(
        "flow.entry",
        "Function start",
        DefKind::FunctionEntry,
        &[],
        &[],
    ),
    // The player.
    pure(
        "player.name",
        "Player name",
        "Player",
        &[],
        Text,
        "player_name()",
    ),
    pure(
        "player.level",
        "Player level",
        "Player",
        &[],
        Number,
        "level()",
    ),
    pure(
        "player.money",
        "Player money",
        "Player",
        &[],
        Number,
        "money()",
    ),
    action(
        "player.give_money",
        "Give money",
        "Player",
        &[("amount", Number)],
        &[],
        "give_money({amount});",
    ),
    action(
        "player.take_money",
        "Take money",
        "Player",
        &[("amount", Number)],
        &[("taken", Bool)],
        "take_money({amount})",
    ),
    pure(
        "player.currency",
        "Currency",
        "Player",
        &[("currency", Text)],
        Number,
        "currency({currency})",
    ),
    action(
        "player.give_currency",
        "Give currency",
        "Player",
        &[("currency", Text), ("amount", Number)],
        &[],
        "give_currency({currency}, {amount});",
    ),
    action(
        "player.take_currency",
        "Take currency",
        "Player",
        &[("currency", Text), ("amount", Number)],
        &[("taken", Bool)],
        "take_currency({currency}, {amount})",
    ),
    action(
        "player.teleport",
        "Teleport",
        "Player",
        &[("map", Map), ("x", Number), ("y", Number)],
        &[],
        "teleport({map}, {x}, {y});",
    ),
    action(
        "player.message",
        "Message",
        "Player",
        &[("text", Text)],
        &[],
        "message({text});",
    ),
    action(
        "player.start_timer",
        "Start timer",
        "Player",
        &[("name", Text), ("seconds", Number)],
        &[],
        "start_timer({name}, {seconds});",
    ),
    pure(
        "player.flag",
        "Flag",
        "Player",
        &[("name", Text)],
        Number,
        "flag({name})",
    ),
    action(
        "player.set_flag",
        "Set flag",
        "Player",
        &[("name", Text), ("value", Number)],
        &[],
        "set_flag({name}, {value});",
    ),
    // Items.
    pure(
        "item.count",
        "Item count",
        "Items",
        &[("item", Item)],
        Number,
        "item_count({item})",
    ),
    pure(
        "item.has",
        "Has item",
        "Items",
        &[("item", Item), ("count", Number)],
        Bool,
        "has_item({item}, {count})",
    ),
    action(
        "item.give",
        "Give item",
        "Items",
        &[("item", Item), ("count", Number)],
        &[("given", Bool)],
        "give_item({item}, {count})",
    ),
    action(
        "item.take",
        "Take item",
        "Items",
        &[("item", Item), ("count", Number)],
        &[("taken", Bool)],
        "take_item({item}, {count})",
    ),
    // Skills.
    pure(
        "skill.knows",
        "Knows skill",
        "Skills",
        &[("skill", Number)],
        Bool,
        "knows_skill({skill})",
    ),
    action(
        "skill.learn",
        "Learn skill",
        "Skills",
        &[("skill", Number)],
        &[("learned", Bool)],
        "learn_skill({skill})",
    ),
    // Quests.
    pure(
        "quest.state",
        "Quest state",
        "Quests",
        &[("quest", Number)],
        Text,
        "quest_state({quest})",
    ),
    pure(
        "quest.ready",
        "Quest ready",
        "Quests",
        &[("quest", Number)],
        Bool,
        "quest_ready({quest})",
    ),
    pure(
        "quest.progress",
        "Quest progress",
        "Quests",
        &[("quest", Number)],
        Number,
        "quest_progress({quest})",
    ),
    action(
        "quest.start",
        "Start quest",
        "Quests",
        &[("quest", Number)],
        &[("started", Bool)],
        "start_quest({quest})",
    ),
    action(
        "quest.complete",
        "Complete quest",
        "Quests",
        &[("quest", Number)],
        &[("completed", Bool)],
        "complete_quest({quest})",
    ),
    // NPC dialogue.
    pure("npc.id", "NPC id", "Dialogue", &[], Number, "npc_id()"),
    pure("npc.name", "NPC name", "Dialogue", &[], Text, "npc_name()"),
    action(
        "dialogue.text",
        "Say",
        "Dialogue",
        &[("text", Text)],
        &[],
        "text({text});",
    ),
    action(
        "dialogue.option",
        "Answer",
        "Dialogue",
        &[("label", Text), ("next", Text)],
        &[],
        "option({label}, {next});",
    ),
    action(
        "dialogue.shop",
        "Open shop",
        "Dialogue",
        &[],
        &[],
        "open_shop();",
    ),
    // Tables.
    pure(
        "table.lookup",
        "Look up",
        "Tables",
        &[("table", Text), ("key", Number), ("column", Text)],
        Number,
        "lookup({table}, {key}, {column})",
    ),
    pure(
        "table.has_row",
        "Has row",
        "Tables",
        &[("table", Text), ("key", Number)],
        Bool,
        "has_row({table}, {key})",
    ),
    // Math and logic.
    pure(
        "math.add",
        "Add",
        "Math",
        &[("a", Number), ("b", Number)],
        Number,
        "({a} + {b})",
    ),
    pure(
        "math.subtract",
        "Subtract",
        "Math",
        &[("a", Number), ("b", Number)],
        Number,
        "({a} - {b})",
    ),
    pure(
        "math.multiply",
        "Multiply",
        "Math",
        &[("a", Number), ("b", Number)],
        Number,
        "({a} * {b})",
    ),
    pure(
        "math.divide",
        "Divide",
        "Math",
        &[("a", Number), ("b", Number)],
        Number,
        "({a} / {b})",
    ),
    pure(
        "math.min",
        "Min",
        "Math",
        &[("a", Number), ("b", Number)],
        Number,
        "min({a}, {b})",
    ),
    pure(
        "math.max",
        "Max",
        "Math",
        &[("a", Number), ("b", Number)],
        Number,
        "max({a}, {b})",
    ),
    pure(
        "logic.equal",
        "Equal",
        "Logic",
        &[("a", Number), ("b", Number)],
        Bool,
        "({a} == {b})",
    ),
    pure(
        "logic.less",
        "Less than",
        "Logic",
        &[("a", Number), ("b", Number)],
        Bool,
        "({a} < {b})",
    ),
    pure(
        "logic.greater",
        "Greater than",
        "Logic",
        &[("a", Number), ("b", Number)],
        Bool,
        "({a} > {b})",
    ),
    pure(
        "logic.and",
        "And",
        "Logic",
        &[("a", Bool), ("b", Bool)],
        Bool,
        "({a} && {b})",
    ),
    pure(
        "logic.or",
        "Or",
        "Logic",
        &[("a", Bool), ("b", Bool)],
        Bool,
        "({a} || {b})",
    ),
    pure("logic.not", "Not", "Logic", &[("a", Bool)], Bool, "(!{a})"),
    pure(
        "text.join",
        "Join text",
        "Text",
        &[("a", Text), ("b", Text)],
        Text,
        "(\"\" + {a} + {b})",
    ),
    pure(
        "text.equal",
        "Text equal",
        "Text",
        &[("a", Text), ("b", Text)],
        Bool,
        "({a} == {b})",
    ),
];

/// The library node `id`.
pub fn node_def(id: &str) -> Option<&'static NodeDef> {
    NODES.iter().find(|d| d.id == id)
}
