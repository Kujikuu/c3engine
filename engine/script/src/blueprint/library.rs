//! The nodes graphs are made of. Each says its pins and the Rhai it becomes: `{pin}` stands
//! for what flows into input `pin`. Events become the script functions the game calls;
//! actions run in exec order; pure nodes are expressions, worked out where they are used.

use super::model::PinType::{self, Bool, Cell, Entity, Item, List, Map, Number, Record, Text};

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
    /// Walks the player to `x`, `y`, then goes on (`then`) once it gets there.
    MoveTo,
    /// Reads or sets the variable named by the node's `name` value.
    GetVariable,
    SetVariable,
    /// Calls the blueprint function named by the node's `name` value.
    Call,
    /// Ends a function, giving back `value`.
    Return,
    /// A function graph's start; its outputs are the function's inputs.
    FunctionEntry,
    /// A bend in a data wire: `in` to `out`, of the type the node's `type` value names.
    Reroute,
    /// A bend in an exec wire.
    RerouteExec,
    /// A note on the canvas (its `text` value); it does nothing.
    Comment,
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
            DefKind::Event { .. }
            | DefKind::Action
            | DefKind::Wait
            | DefKind::MoveTo
            | DefKind::SetVariable => &["then"],
            DefKind::Call | DefKind::FunctionEntry | DefKind::RerouteExec => &["then"],
            DefKind::Branch => &["true", "false"],
            DefKind::Sequence => &["then 0", "then 1", "then 2", "then 3"],
            DefKind::ForEach => &["body", "done"],
            DefKind::Pure
            | DefKind::GetVariable
            | DefKind::Return
            | DefKind::Reroute
            | DefKind::Comment => &[],
        }
    }

    /// Whether it has an exec input (it runs when reached).
    pub fn runs(&self) -> bool {
        !matches!(
            self.kind,
            DefKind::Event { .. }
                | DefKind::Pure
                | DefKind::GetVariable
                | DefKind::FunctionEntry
                | DefKind::Reroute
                | DefKind::Comment
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
            Record => &[("value", Record)],
            Cell => &[("value", Cell)],
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

/// A formula the game's rules ask for: an event whose graph ends at a Return giving the
/// answer (see [`returns`]).
const fn formula(
    id: &'static str,
    title: &'static str,
    function: &'static str,
    outputs: &'static [(&'static str, PinType)],
) -> NodeDef {
    NodeDef {
        category: "Formulas",
        ..event(id, title, function, outputs)
    }
}

/// What the game's formula `function` gives back, when it is one.
pub fn returns(function: &str) -> Option<PinType> {
    match function {
        "player_stats" => Some(Record),
        "level_exp" | "hit" | "hit_exp" | "kill_exp" | "skill_hit" | "skill_heal" => Some(Number),
        "drops" => Some(List),
        _ => None,
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
    event("event.death", "On dying", "on_death", &[("killer", Entity)]),
    event(
        "event.level_up",
        "On levelling up",
        "on_level_up",
        &[("level", Number)],
    ),
    event(
        "event.leave_map",
        "On leaving a map",
        "on_leave",
        &[("map", Map)],
    ),
    event("event.talk", "On talking (an NPC)", "talk", &[]),
    // A function of its own name (an answer's `next`, or one the game calls).
    event("event.custom", "Custom event", "", &[]),
    // The rules' formulas (each ends at a Return giving the answer).
    formula(
        "formula.player_stats",
        "Player stats at a level",
        "player_stats",
        &[("level", Number)],
    ),
    formula(
        "formula.level_exp",
        "Experience to the next level",
        "level_exp",
        &[("level", Number)],
    ),
    formula(
        "formula.hit",
        "Damage of a hit",
        "hit",
        &[("attacker", Record), ("target", Record)],
    ),
    formula(
        "formula.hit_exp",
        "Experience for a hit",
        "hit_exp",
        &[("level", Number), ("target", Record), ("damage", Number)],
    ),
    formula(
        "formula.drops",
        "What a monster leaves",
        "drops",
        &[("monster", Record)],
    ),
    formula(
        "formula.kill_exp",
        "Experience for a kill",
        "kill_exp",
        &[("level", Number), ("target", Record)],
    ),
    formula(
        "formula.skill_hit",
        "Damage of a skill",
        "skill_hit",
        &[("attacker", Record), ("target", Record), ("skill", Record)],
    ),
    formula(
        "formula.skill_heal",
        "Healing of a skill",
        "skill_heal",
        &[("healer", Record), ("skill", Record)],
    ),
    pure(
        "record.get",
        "Read a number",
        "Formulas",
        &[("record", Record), ("field", Text)],
        Number,
        "{record}[{field}]",
    ),
    pure(
        "record.stats",
        "Stats",
        "Formulas",
        &[
            ("life", Number),
            ("mana", Number),
            ("attack_min", Number),
            ("attack_max", Number),
            ("defence", Number),
            ("dodge", Number),
            ("dexterity", Number),
        ],
        Record,
        "#{life: {life}, mana: {mana}, attack_min: {attack_min}, attack_max: {attack_max}, defence: {defence}, dodge: {dodge}, dexterity: {dexterity}}",
    ),
    // Lists (a drops formula's answer: item types, and money).
    pure(
        "list.items",
        "Drop items",
        "Formulas",
        &[("a", Item), ("b", Item), ("c", Item)],
        List,
        "[{a}, {b}, {c}].filter(|x| x != 0)",
    ),
    pure(
        "list.money",
        "Add money",
        "Formulas",
        &[("list", List), ("amount", Number)],
        List,
        "{list} + [#{money: {amount}}]",
    ),
    pure(
        "list.size",
        "List size",
        "Formulas",
        &[("list", List)],
        Number,
        "{list}.len()",
    ),
    // Cells.
    pure(
        "cell.make",
        "Cell",
        "Cells",
        &[("x", Number), ("y", Number)],
        Cell,
        "[{x}, {y}]",
    ),
    pure(
        "cell.x",
        "Cell x",
        "Cells",
        &[("cell", Cell)],
        Number,
        "{cell}[0]",
    ),
    pure(
        "cell.y",
        "Cell y",
        "Cells",
        &[("cell", Cell)],
        Number,
        "{cell}[1]",
    ),
    pure(
        "cell.player",
        "Player cell",
        "Cells",
        &[],
        Cell,
        "[player_x(), player_y()]",
    ),
    pure(
        "cell.distance",
        "Distance",
        "Cells",
        &[("a", Cell), ("b", Cell)],
        Number,
        "max(abs({a}[0] - {b}[0]), abs({a}[1] - {b}[1]))",
    ),
    pure(
        "cell.equal",
        "Same cell",
        "Cells",
        &[("a", Cell), ("b", Cell)],
        Bool,
        "({a} == {b})",
    ),
    action(
        "cell.teleport",
        "Teleport to cell",
        "Cells",
        &[("map", Map), ("cell", Cell)],
        &[],
        "teleport({map}, {cell}[0], {cell}[1]);",
    ),
    action(
        "cell.spawn",
        "Spawn monster at cell",
        "Cells",
        &[("monster", Number), ("cell", Cell)],
        &[],
        "spawn_monster({monster}, {cell}[0], {cell}[1]);",
    ),
    flow(
        "cell.move_to",
        "Walk to cell",
        DefKind::MoveTo,
        &[("cell", Cell)],
        &[],
    ),
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
        "flow.move_to",
        "Walk to",
        DefKind::MoveTo,
        &[("x", Number), ("y", Number)],
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
    flow("flow.reroute", "Reroute", DefKind::Reroute, &[], &[]),
    flow(
        "flow.reroute_exec",
        "Reroute (exec)",
        DefKind::RerouteExec,
        &[],
        &[],
    ),
    NodeDef {
        category: "Notes",
        ..flow("note.comment", "Comment", DefKind::Comment, &[], &[])
    },
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
        "message(fill({text}));",
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
    pure("player.life", "Life", "Player", &[], Number, "life()"),
    pure(
        "player.max_life",
        "Max life",
        "Player",
        &[],
        Number,
        "max_life()",
    ),
    action(
        "player.heal",
        "Heal",
        "Player",
        &[("amount", Number)],
        &[],
        "heal({amount});",
    ),
    action(
        "player.hurt",
        "Hurt",
        "Player",
        &[("amount", Number)],
        &[],
        "hurt({amount});",
    ),
    action(
        "player.give_exp",
        "Give experience",
        "Player",
        &[("amount", Number)],
        &[],
        "give_exp({amount});",
    ),
    pure("player.map", "Map", "Player", &[], Map, "map_id()"),
    pure("player.x", "Cell x", "Player", &[], Number, "player_x()"),
    pure("player.y", "Cell y", "Player", &[], Number, "player_y()"),
    // Statuses.
    pure(
        "status.has",
        "Has status",
        "Statuses",
        &[("status", Number)],
        Bool,
        "has_status({status})",
    ),
    action(
        "status.add",
        "Add status",
        "Statuses",
        &[("status", Number)],
        &[],
        "add_status({status});",
    ),
    // The world.
    action(
        "world.announce",
        "Announce",
        "World",
        &[("text", Text)],
        &[],
        "announce(fill({text}));",
    ),
    action(
        "world.spawn",
        "Spawn monster",
        "World",
        &[("monster", Number), ("x", Number), ("y", Number)],
        &[],
        "spawn_monster({monster}, {x}, {y});",
    ),
    pure(
        "world.flag",
        "World variable",
        "World",
        &[("name", Text)],
        Number,
        "world_flag({name})",
    ),
    action(
        "world.set_flag",
        "Set world variable",
        "World",
        &[("name", Text), ("value", Number)],
        &[],
        "set_world_flag({name}, {value});",
    ),
    pure(
        "player.profession",
        "Profession",
        "Player",
        &[],
        Number,
        "profession()",
    ),
    // Teams.
    pure("team.in", "In a team", "Teams", &[], Bool, "in_team()"),
    pure(
        "team.size",
        "Team size",
        "Teams",
        &[],
        Number,
        "team_size()",
    ),
    pure(
        "team.leader",
        "Team leader",
        "Teams",
        &[],
        Text,
        "team_leader()",
    ),
    action(
        "team.message",
        "Tell the team",
        "Teams",
        &[("text", Text)],
        &[],
        "team_message({text});",
    ),
    // Effects, sounds and the game UI.
    action(
        "fx.effect",
        "Play effect",
        "Effects",
        &[("effect", Text)],
        &[],
        "play_effect({effect});",
    ),
    action(
        "fx.sound",
        "Play sound",
        "Effects",
        &[("sound", Text)],
        &[],
        "play_sound({sound});",
    ),
    action(
        "ui.open",
        "Open window",
        "UI",
        &[("dialog", Number)],
        &[],
        "open_window({dialog});",
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
        "text(fill({text}));",
    ),
    action(
        "dialogue.option",
        "Answer",
        "Dialogue",
        &[("label", Text), ("next", Text)],
        &[],
        "option(fill({label}), {next});",
    ),
    action(
        "dialogue.close_option",
        "Answer (closes)",
        "Dialogue",
        &[("label", Text)],
        &[],
        "option(fill({label}));",
    ),
    action(
        "dialogue.picture",
        "Show picture",
        "Dialogue",
        &[("picture", Number)],
        &[],
        "picture({picture});",
    ),
    action(
        "dialogue.shop",
        "Open shop",
        "Dialogue",
        &[],
        &[],
        "open_shop();",
    ),
    pure("player.pk", "PK points", "Player", &[], Number, "pk()"),
    action(
        "player.add_pk",
        "Add PK points",
        "Player",
        &[("points", Number)],
        &[],
        "add_pk({points});",
    ),
    pure(
        "item.worn",
        "Worn item",
        "Items",
        &[("slot", Number)],
        Item,
        "worn({slot})",
    ),
    pure(
        "time.within",
        "Time within",
        "World",
        &[("mode", Number), ("window", Text)],
        Bool,
        "time_within({mode}, {window})",
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
        "math.remainder",
        "Remainder",
        "Math",
        &[("a", Number), ("b", Number)],
        Number,
        "({a} % {b})",
    ),
    pure(
        "math.chance",
        "Chance",
        "Math",
        &[("times", Number), ("out_of", Number)],
        Bool,
        "(random(0, {out_of} - 1) < {times})",
    ),
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
        "math.random",
        "Random",
        "Math",
        &[("low", Number), ("high", Number)],
        Number,
        "random({low}, {high})",
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
