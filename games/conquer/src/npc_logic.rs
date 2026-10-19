//! TQ's NPC logic as blueprints. A TQ NPC names up to eight tasks (`cq_task`): clicking it
//! runs the first task whose check passes (items carried by name, money, profession, PK
//! range), and a task leads into a chain of actions (`cq_action`): each runs, then goes on to
//! `id_next` when it succeeded or `id_nextfail` when it didn't. Dialogue lines, answers
//! (which name the task they lead to), checks and changes to the player are actions.
//!
//! Each NPC's tasks become one blueprint (`tq/npc_<first task>`, shared by NPCs with the same
//! tasks): its talk event checks the tasks in order; every task an answer leads to is a custom
//! event of that name (`task_<id>`); a chain that more than one place leads to is a function
//! (`a<id>`), so nothing is written twice and loops become calls. An action this converter
//! has no nodes for becomes a comment and the chain goes on as if it succeeded; the report
//! counts them by type. Behaviour follows `tq_re/spec/actions`.

use c3_data::sql::SqlValue;
use c3_script::blueprint::{Blueprint, Function, Graph, GraphNode, Literal, PinRef, Wire};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// One `cq_action` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub id: u32,
    pub next: u32,
    pub fail: u32,
    pub kind: u32,
    pub data: i64,
    pub param: String,
}

/// One `cq_task` row: what it asks of the player and where it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: u32,
    pub next: u32,
    pub fail: u32,
    /// Items the player must carry, by name (empty: none).
    pub items: [String; 2],
    pub money: i64,
    /// The profession asked for (0: any).
    pub profession: i64,
    pub min_pk: i64,
    pub max_pk: i64,
}

/// TQ's NPC logic of a dump.
#[derive(Debug, Default)]
pub struct NpcLogic {
    pub actions: HashMap<u32, Action>,
    pub tasks: HashMap<u32, Task>,
}

fn text(values: &[SqlValue], i: usize) -> String {
    values.get(i).cloned().flatten().unwrap_or_default()
}

fn number(values: &[SqlValue], i: usize) -> i64 {
    text(values, i).trim().parse().unwrap_or(0)
}

impl NpcLogic {
    /// The `cq_action` and `cq_task` rows of an SQL dump.
    pub fn from_sql(sql: &str) -> Self {
        let mut logic = Self::default();
        for line in sql.lines() {
            if let Some(v) = crate::server_data::insert_values(line, "cq_action") {
                if v.len() == 6 {
                    let id = number(&v, 0) as u32;
                    logic.actions.insert(
                        id,
                        Action {
                            id,
                            next: number(&v, 1) as u32,
                            fail: number(&v, 2) as u32,
                            kind: number(&v, 3) as u32,
                            data: number(&v, 4),
                            param: text(&v, 5),
                        },
                    );
                }
            } else if let Some(v) = crate::server_data::insert_values(line, "cq_task")
                && v.len() >= 10
            {
                let id = number(&v, 0) as u32;
                logic.tasks.insert(
                    id,
                    Task {
                        id,
                        next: number(&v, 1) as u32,
                        fail: number(&v, 2) as u32,
                        items: [text(&v, 3), text(&v, 4)],
                        money: number(&v, 5),
                        profession: number(&v, 6),
                        min_pk: number(&v, 8),
                        max_pk: number(&v, 9),
                    },
                );
            }
        }
        logic
    }
}

/// NPCs' logic converted: the blueprints, which one each NPC runs (by NPC id), and the
/// report.
#[derive(Debug, Default)]
pub struct Converted {
    pub blueprints: Vec<(String, Blueprint)>,
    /// NPC id → the script it names (`graphs/<blueprint>`).
    pub scripts: BTreeMap<u32, String>,
    pub report: Vec<String>,
}

/// A place an exec wire leaves from: a node's exec output.
type Exit = (u32, &'static str);

/// What one action became: the node a chain enters it at (none: it does nothing, the chain
/// goes straight on), where it goes on when it succeeded and when it failed (none: the chain
/// stops there).
struct Emitted {
    entry: Option<u32>,
    ok: Option<Exit>,
    fail: Option<Exit>,
}

/// Converts the logic of `npcs` (id → its tasks, in order) with `items` (an item type's name
/// → its types).
pub fn convert(
    logic: &NpcLogic,
    npcs: &BTreeMap<u32, Vec<u32>>,
    items: &HashMap<String, Vec<u32>>,
) -> Converted {
    let mut out = Converted::default();
    let mut by_tasks: BTreeMap<Vec<u32>, String> = BTreeMap::new();
    let mut skipped: BTreeMap<u32, usize> = BTreeMap::new();
    let mut converted_actions = 0;
    let mut missing = BTreeSet::new();
    for (&npc, tasks) in npcs {
        let tasks: Vec<u32> = tasks.iter().copied().filter(|&t| t != 0).collect();
        if tasks.is_empty() {
            continue;
        }
        let name = match by_tasks.get(&tasks) {
            Some(name) => name.clone(),
            None => {
                let mut name = format!("tq/npc_{}", tasks[0]);
                if by_tasks.values().any(|n| *n == name) {
                    name = format!("{name}_{npc}");
                }
                let mut builder = Builder::new(logic, items);
                builder.npc(&tasks);
                converted_actions += builder.actions_seen.len();
                for (kind, n) in builder.skipped {
                    *skipped.entry(kind).or_default() += n;
                }
                missing.extend(builder.missing);
                out.blueprints.push((name.clone(), builder.blueprint));
                by_tasks.insert(tasks.clone(), name.clone());
                name
            }
        };
        out.scripts
            .insert(npc, c3_script::blueprint::graph_script(&name));
    }
    let not_converted: usize = skipped.values().sum();
    out.report.push(format!(
        "NPC logic: {} NPCs run {} blueprints made of {converted_actions} TQ actions; \
         {not_converted} actions have no nodes yet (comments in the blueprints)",
        out.scripts.len(),
        out.blueprints.len(),
    ));
    if !skipped.is_empty() {
        let mut kinds: Vec<(u32, usize)> = skipped.into_iter().collect();
        kinds.sort_by_key(|(k, n)| (std::cmp::Reverse(*n), *k));
        let list: Vec<String> = kinds.iter().map(|(k, n)| format!("{k} ({n})")).collect();
        out.report.push(format!(
            "  not converted, by action type (count): {}",
            list.join(", ")
        ));
    }
    if !missing.is_empty() {
        out.report.push(format!(
            "  {} tasks or actions named but not in the dump (their chains stop there)",
            missing.len()
        ));
    }
    out
}

/// Builds one NPC's blueprint.
struct Builder<'a> {
    logic: &'a NpcLogic,
    items: &'a HashMap<String, Vec<u32>>,
    blueprint: Blueprint,
    /// How many places lead to each action (more than one: it is a function).
    refs: HashMap<u32, usize>,
    /// Functions made or to make, and tasks given an event.
    functions: BTreeSet<u32>,
    to_make: Vec<u32>,
    task_events: BTreeSet<u32>,
    to_event: Vec<u32>,
    /// Chains a timer runs later (1071), each by its function's name.
    timers: BTreeSet<u32>,
    actions_seen: BTreeSet<u32>,
    skipped: BTreeMap<u32, usize>,
    missing: BTreeSet<u32>,
}

/// A graph being written and where its next node goes.
struct Canvas {
    graph: Graph,
    /// The next free row of each column.
    rows: HashMap<i32, f32>,
    /// Where the current lane starts.
    top: f32,
}

impl Canvas {
    fn new() -> Self {
        Self {
            graph: Graph::default(),
            rows: HashMap::new(),
            top: 0.0,
        }
    }

    /// Adds node `def` with `values` in column `column`; its id.
    fn node(&mut self, def: &str, values: &[(&str, Literal)], column: i32) -> u32 {
        let id = self.graph.next_id();
        let row = self.rows.entry(column).or_insert(self.top);
        let pos = [column as f32 * 280.0, *row];
        *row += 140.0;
        self.graph.nodes.push(GraphNode {
            id,
            def: def.to_owned(),
            pos,
            values: (values.iter())
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
        });
        id
    }

    /// Starts a new lane below everything placed so far (each event reads left to right).
    fn lane(&mut self) {
        let bottom = self.rows.values().copied().fold(0.0, f32::max);
        for row in self.rows.values_mut() {
            *row = bottom + 80.0;
        }
        self.top = bottom + 80.0;
    }

    fn wire(&mut self, from: (u32, &str), to: (u32, &str)) {
        self.graph.wires.push(Wire {
            from: PinRef {
                node: from.0,
                pin: from.1.to_owned(),
            },
            to: PinRef {
                node: to.0,
                pin: to.1.to_owned(),
            },
        });
    }

    /// Wires exec `from` into node `to`.
    fn exec(&mut self, from: Exit, to: u32) {
        self.wire(from, (to, c3_script::blueprint::EXEC_IN));
    }

    /// Wires pure node `from`'s value into input `pin` of `to`.
    fn data(&mut self, from: u32, to: u32, pin: &str) {
        self.wire((from, "value"), (to, pin));
    }
}

fn num(n: i64) -> Literal {
    Literal::Number(n as f64)
}

fn txt(t: &str) -> Literal {
    Literal::Text(t.to_owned())
}

/// TQ dialogue text as the engine's: `~` is a space, `%user_name` the player's name...
fn line(param: &str) -> String {
    let mut text = param.replace('~', " ");
    for (tq, engine) in [
        ("%user_name", "{player}"),
        ("%user_lev", "{level}"),
        ("%user_money", "{money}"),
        ("%npc_name", "{npc}"),
    ] {
        text = text.replace(tq, engine);
    }
    text
}

impl<'a> Builder<'a> {
    fn new(logic: &'a NpcLogic, items: &'a HashMap<String, Vec<u32>>) -> Self {
        Self {
            logic,
            items,
            blueprint: Blueprint::default(),
            refs: HashMap::new(),
            functions: BTreeSet::new(),
            to_make: Vec::new(),
            task_events: BTreeSet::new(),
            to_event: Vec::new(),
            timers: BTreeSet::new(),
            actions_seen: BTreeSet::new(),
            skipped: BTreeMap::new(),
            missing: BTreeSet::new(),
        }
    }

    /// Counts how many places lead to each action reachable from `tasks`.
    fn count_refs(&mut self, tasks: &[u32]) {
        let mut stack: Vec<(bool, u32)> = tasks.iter().map(|&t| (true, t)).collect();
        let mut seen_tasks = BTreeSet::new();
        let mut seen = BTreeSet::new();
        let lead = |refs: &mut HashMap<u32, usize>, stack: &mut Vec<(bool, u32)>, id: u32| {
            if id != 0 {
                *refs.entry(id).or_default() += 1;
                stack.push((false, id));
            }
        };
        while let Some((is_task, id)) = stack.pop() {
            if is_task {
                let Some(task) = self.logic.tasks.get(&id) else {
                    continue;
                };
                if seen_tasks.insert(id) {
                    lead(&mut self.refs, &mut stack, task.next);
                    lead(&mut self.refs, &mut stack, task.fail);
                }
                continue;
            }
            if !seen.insert(id) {
                continue;
            }
            let Some(action) = self.logic.actions.get(&id) else {
                continue;
            };
            lead(&mut self.refs, &mut stack, action.next);
            lead(&mut self.refs, &mut stack, action.fail);
            for target in self.called(action) {
                // Called chains are functions: count them twice.
                *self.refs.entry(target).or_default() += 2;
                stack.push((false, target));
            }
            if let Some(task) = self.answer_task(action) {
                stack.push((true, task));
            }
        }
    }

    /// The chains an action runs apart from its own (122's random pick).
    fn called(&self, action: &Action) -> Vec<u32> {
        match action.kind {
            122 => (action.param.split_whitespace())
                .filter_map(|t| t.parse().ok())
                .filter(|&t| t != 0)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The task an answer (102) or a typed reply (103) leads to.
    fn answer_task(&self, action: &Action) -> Option<u32> {
        let tokens: Vec<&str> = action.param.split_whitespace().collect();
        let task = match action.kind {
            102 | 103 => tokens.get(1)?.parse().ok()?,
            _ => return None,
        };
        (task != 0).then_some(task)
    }

    /// The whole blueprint of an NPC with `tasks`.
    fn npc(&mut self, tasks: &[u32]) {
        self.count_refs(tasks);
        let mut events = Canvas::new();
        let talk = events.node("event.talk", &[], 0);
        // The first task whose check passes runs (its id_next; a failed check tries the next).
        let mut from: Option<Exit> = Some((talk, "then"));
        for &id in tasks {
            let Some(task) = self.logic.tasks.get(&id).cloned() else {
                self.missing.insert(id);
                continue;
            };
            let Some(exit) = from else { break };
            let (pass, fail) = self.task_check(&mut events, &task, exit, 1);
            self.chain(&mut events, pass, task.next, 3);
            from = fail;
        }
        // Tasks answers lead to, each a custom event; functions made as chains need them.
        loop {
            if let Some(task) = self.to_event.pop() {
                self.task_event(&mut events, task);
            } else if let Some(action) = self.to_make.pop() {
                self.function(action);
            } else {
                break;
            }
        }
        // Timers run their chains: on a timer named `a<id>`, call function `a<id>`.
        if !self.timers.is_empty() {
            events.lane();
            let timer = events.node("event.timer", &[], 0);
            let mut from: Exit = (timer, "then");
            for &chain in &self.timers {
                let name = format!("a{chain}");
                let eq = events.node("text.equal", &[("b", txt(&name))], 1);
                events.wire((timer, "name"), (eq, "a"));
                let branch = events.node("flow.branch", &[], 2);
                events.data(eq, branch, "condition");
                events.exec(from, branch);
                let call = events.node("flow.call", &[("name", txt(&name))], 3);
                events.exec((branch, "true"), call);
                from = (branch, "false");
            }
        }
        self.blueprint.events = events.graph;
    }

    /// Custom event `task_<id>`: the task's check, then its chains (a failed check with no
    /// chain of its own tells the player, as TQ does).
    fn task_event(&mut self, events: &mut Canvas, id: u32) {
        let Some(task) = self.logic.tasks.get(&id).cloned() else {
            self.missing.insert(id);
            return;
        };
        events.lane();
        let start = events.node("event.custom", &[("name", txt(&format!("task_{id}")))], 0);
        let (pass, fail) = self.task_check(events, &task, (start, "then"), 1);
        self.chain(events, pass, task.next, 3);
        match (fail, task.fail) {
            (Some(exit), 0) => {
                let say = events.node(
                    "player.message",
                    &[("text", txt("You do not meet the requirements."))],
                    3,
                );
                events.exec(exit, say);
            }
            (fail, next) => self.chain(events, fail, next, 3),
        }
    }

    /// The task's check from `from`: where it goes on when it passes and when it fails
    /// (none: it can't fail).
    fn task_check(
        &mut self,
        g: &mut Canvas,
        task: &Task,
        from: Exit,
        column: i32,
    ) -> (Option<Exit>, Option<Exit>) {
        let mut conditions = Vec::new();
        for name in task.items.iter().filter(|n| !n.is_empty() && *n != "0") {
            conditions.push(self.has_named(g, name, column));
        }
        if task.money > 0 {
            let money = g.node("player.money", &[], column);
            let less = g.node("logic.less", &[("b", num(task.money))], column);
            g.data(money, less, "a");
            let not = g.node("logic.not", &[], column);
            g.data(less, not, "a");
            conditions.push(not);
        }
        if task.profession != 0 {
            let prof = g.node("player.profession", &[], column);
            let equal = g.node("logic.equal", &[("b", num(task.profession))], column);
            g.data(prof, equal, "a");
            conditions.push(equal);
        }
        // TQ keeps PK points within ±30000: a wider range asks nothing.
        if task.min_pk > -30_000 {
            conditions.push(self.compare(g, "player.pk", ">=", task.min_pk, column));
        }
        if task.max_pk < 30_000 {
            conditions.push(self.compare(g, "player.pk", "<=", task.max_pk, column));
        }
        let Some(cond) = self.all(g, conditions, column) else {
            return (Some(from), None);
        };
        let branch = g.node("flow.branch", &[], column + 1);
        g.data(cond, branch, "condition");
        g.exec(from, branch);
        (Some((branch, "true")), Some((branch, "false")))
    }

    /// The pure nodes `conditions` joined with And (none: no node).
    fn all(&mut self, g: &mut Canvas, conditions: Vec<u32>, column: i32) -> Option<u32> {
        conditions.into_iter().reduce(|a, b| {
            let and = g.node("logic.and", &[], column);
            g.data(a, and, "a");
            g.data(b, and, "b");
            and
        })
    }

    /// Whether the player carries an item named `name` (any of its types).
    fn has_named(&mut self, g: &mut Canvas, name: &str, column: i32) -> u32 {
        let types = self.items.get(name).cloned().unwrap_or_default();
        let checks: Vec<u32> = (types.iter())
            .map(|&t| {
                g.node(
                    "item.has",
                    &[("item", num(t.into())), ("count", num(1))],
                    column,
                )
            })
            .collect();
        let any = checks.into_iter().reduce(|a, b| {
            let or = g.node("logic.or", &[], column);
            g.data(a, or, "a");
            g.data(b, or, "b");
            or
        });
        // An item the game doesn't know is never carried.
        any.unwrap_or_else(|| g.node("logic.equal", &[("a", num(0)), ("b", num(1))], column))
    }

    /// Pure node `value` (with no inputs) compared with `n` by TQ's `op`.
    fn compare(&mut self, g: &mut Canvas, value: &str, op: &str, n: i64, column: i32) -> u32 {
        let v = g.node(value, &[], column);
        self.compare_node(g, v, op, n, column)
    }

    /// The value of pure node `v` compared with `n` by `op` (`==`, `<`, `<=`, `>`, `>=`).
    fn compare_node(&mut self, g: &mut Canvas, v: u32, op: &str, n: i64, column: i32) -> u32 {
        let (def, value_pin, negate) = match op {
            "<" => ("logic.less", "a", false),
            ">=" => ("logic.less", "a", true),
            ">" => ("logic.greater", "a", false),
            "<=" => ("logic.greater", "a", true),
            _ => ("logic.equal", "a", false),
        };
        let cmp = g.node(def, &[("b", num(n))], column);
        g.data(v, cmp, value_pin);
        if !negate {
            return cmp;
        }
        let not = g.node("logic.not", &[], column);
        g.data(cmp, not, "a");
        not
    }

    /// The chain from action `id` on, entered from `from` (none: unreachable).
    fn chain(&mut self, g: &mut Canvas, from: Option<Exit>, id: u32, column: i32) {
        self.chain_from(g, from, id, column, false);
    }

    /// [`Builder::chain`]; with `here`, action `id` is written here even when it is a
    /// function's (as that function's first step).
    fn chain_from(&mut self, g: &mut Canvas, from: Option<Exit>, id: u32, column: i32, here: bool) {
        let Some(from) = from else { return };
        if id == 0 {
            return;
        }
        if !self.logic.actions.contains_key(&id) {
            self.missing.insert(id);
            return;
        }
        if !here && self.refs.get(&id).copied().unwrap_or(0) > 1 {
            let call = self.call(g, id, column);
            g.exec(from, call);
            return;
        }
        let action = self.logic.actions[&id].clone();
        self.actions_seen.insert(id);
        let emitted = self.action(g, &action, column);
        let (ok, fail) = match emitted.entry {
            Some(entry) => {
                g.exec(from, entry);
                (emitted.ok, emitted.fail)
            }
            // It does nothing: the chain goes straight on.
            None => (Some(from), None),
        };
        if ok.is_some() && fail.is_some() && action.next == action.fail {
            // Both ways lead on to the same chain: one call each.
            let next = action.next;
            for exit in [ok, fail].into_iter().flatten() {
                if next != 0 {
                    let call = self.call(g, next, column + 1);
                    g.exec(exit, call);
                }
            }
            return;
        }
        self.chain(g, ok, action.next, column + 1);
        self.chain(g, fail, action.fail, column + 1);
    }

    /// A call of the function made of the chain from action `id` (made when first called).
    fn call(&mut self, g: &mut Canvas, id: u32, column: i32) -> u32 {
        if self.functions.insert(id) {
            self.to_make.push(id);
        }
        g.node("flow.call", &[("name", txt(&format!("a{id}")))], column)
    }

    /// Function `a<id>`: the chain from action `id`.
    fn function(&mut self, id: u32) {
        let mut g = Canvas::new();
        let entry = g.node("flow.entry", &[], 0);
        self.chain_from(&mut g, Some((entry, "then")), id, 1, true);
        self.blueprint.functions.push(Function {
            name: format!("a{id}"),
            inputs: Vec::new(),
            output: None,
            inline: false,
            graph: g.graph,
        });
    }

    /// An action node `def` with `values`, which always succeeds.
    fn plain(g: &mut Canvas, def: &str, values: &[(&str, Literal)], column: i32) -> Emitted {
        let id = g.node(def, values, column);
        Emitted {
            entry: Some(id),
            ok: Some((id, "then")),
            fail: None,
        }
    }

    /// A Branch on pure node `cond`.
    fn branch(g: &mut Canvas, cond: u32, column: i32) -> Emitted {
        let branch = g.node("flow.branch", &[], column);
        g.data(cond, branch, "condition");
        Emitted {
            entry: Some(branch),
            ok: Some((branch, "true")),
            fail: Some((branch, "false")),
        }
    }

    /// Action node `def` whose bool output `out` says whether it succeeded.
    fn checked(
        g: &mut Canvas,
        def: &str,
        values: &[(&str, Literal)],
        out: &str,
        column: i32,
    ) -> Emitted {
        let node = g.node(def, values, column);
        let branch = g.node("flow.branch", &[], column);
        g.wire((node, out), (branch, "condition"));
        g.exec((node, "then"), branch);
        Emitted {
            entry: Some(node),
            ok: Some((branch, "true")),
            fail: Some((branch, "false")),
        }
    }

    /// Nothing happens and the chain goes on.
    fn nothing() -> Emitted {
        Emitted {
            entry: None,
            ok: None,
            fail: None,
        }
    }

    /// An action with no nodes yet: a comment saying what it was, and the chain goes on.
    fn not_converted(&mut self, g: &mut Canvas, action: &Action, column: i32) -> Emitted {
        *self.skipped.entry(action.kind).or_default() += 1;
        let text = format!(
            "TQ action {} (type {}, data {}, param \"{}\") is not converted",
            action.id, action.kind, action.data, action.param
        );
        g.node("note.comment", &[("text", txt(&text))], column);
        Self::nothing()
    }

    /// The nodes of one action.
    fn action(&mut self, g: &mut Canvas, a: &Action, column: i32) -> Emitted {
        let tokens: Vec<&str> = a.param.split_whitespace().collect();
        let int = |i: usize| tokens.get(i).and_then(|t| t.parse::<i64>().ok());
        match a.kind {
            // Dialogue: a line, an answer leading to a task, a picture, show it (the engine
            // shows the page when the step ends).
            101 => Self::plain(
                g,
                "dialogue.text",
                &[("text", txt(&line(&a.param)))],
                column,
            ),
            102 => {
                let label = line(tokens.first().copied().unwrap_or(""));
                match self.answer_task(a) {
                    Some(task) => {
                        if self.task_events.insert(task) {
                            self.to_event.push(task);
                        }
                        let next = txt(&format!("task_{task}"));
                        Self::plain(
                            g,
                            "dialogue.option",
                            &[("label", txt(&label)), ("next", next)],
                            column,
                        )
                    }
                    None => Self::plain(
                        g,
                        "dialogue.close_option",
                        &[("label", txt(&label))],
                        column,
                    ),
                }
            }
            104 => match int(2) {
                Some(picture) => {
                    Self::plain(g, "dialogue.picture", &[("picture", num(picture))], column)
                }
                None => self.not_converted(g, a, column),
            },
            120 | 1022 | 1085 => Self::nothing(),
            121 => match (int(0), int(1)) {
                (Some(times), Some(out_of)) => {
                    let chance = g.node(
                        "math.chance",
                        &[("times", num(times)), ("out_of", num(out_of))],
                        column,
                    );
                    Self::branch(g, chance, column)
                }
                _ => self.not_converted(g, a, column),
            },
            122 => self.random_action(g, a, column),
            123 => {
                let within = g.node(
                    "time.within",
                    &[("mode", num(a.data)), ("window", txt(&a.param))],
                    column,
                );
                Self::branch(g, within, column)
            }
            125 | 303 => Self::plain(
                g,
                "world.announce",
                &[("text", txt(&line(&a.param)))],
                column,
            ),
            // Items: give, take one, carries one (by type, or by name when data is 0).
            501 => Self::checked(
                g,
                "item.give",
                &[("item", num(a.data)), ("count", num(1))],
                "given",
                column,
            ),
            502 => match self.item_type(a) {
                Some(kind) => Self::checked(
                    g,
                    "item.take",
                    &[("item", num(kind)), ("count", num(1))],
                    "taken",
                    column,
                ),
                None => self.not_converted(g, a, column),
            },
            503 => {
                let has = match a.data {
                    0 => self.has_named(g, &a.param, column),
                    kind => g.node(
                        "item.has",
                        &[("item", num(kind)), ("count", num(1))],
                        column,
                    ),
                };
                Self::branch(g, has, column)
            }
            1001 => self.user_attr(g, a, &tokens, column),
            1002 if a.param.eq_ignore_ascii_case("life") => {
                let max = g.node("player.max_life", &[], column);
                let heal = g.node("player.heal", &[], column);
                g.data(max, heal, "amount");
                Emitted {
                    entry: Some(heal),
                    ok: Some((heal, "then")),
                    fail: None,
                }
            }
            1003 => match (int(0), int(1), int(2)) {
                (Some(map), Some(x), Some(y)) => Self::plain(
                    g,
                    "player.teleport",
                    &[("map", num(map)), ("x", num(x)), ("y", num(y))],
                    column,
                ),
                _ => self.not_converted(g, a, column),
            },
            1010 => Self::plain(
                g,
                "player.message",
                &[("text", txt(&line(&a.param)))],
                column,
            ),
            1020 => match (tokens.first().copied(), int(1)) {
                (Some(op), Some(skill))
                    if op.eq_ignore_ascii_case("check") && tokens.len() == 2 =>
                {
                    let knows = g.node("skill.knows", &[("skill", num(skill))], column);
                    Self::branch(g, knows, column)
                }
                (Some(op), Some(skill)) if op.eq_ignore_ascii_case("learn") => Self::checked(
                    g,
                    "skill.learn",
                    &[("skill", num(skill))],
                    "learned",
                    column,
                ),
                _ => self.not_converted(g, a, column),
            },
            1027 if tokens.len() >= 2 && tokens.get(2).is_none() => {
                Self::plain(g, "fx.effect", &[("effect", txt(tokens[1]))], column)
            }
            1060 | 1061 | 1064 => self.variable(g, a, &tokens, column),
            1065 => match (int(0), int(1)) {
                (Some(slot), Some(sub)) => {
                    // The worn item's type, its kind digits (`type % 1000000 / 1000`).
                    let worn = g.node("item.worn", &[("slot", num(slot))], column);
                    let rem = g.node("math.remainder", &[("b", num(1_000_000))], column);
                    g.data(worn, rem, "a");
                    let div = g.node("math.divide", &[("b", num(1000))], column);
                    g.data(rem, div, "a");
                    let eq = self.compare_node(g, div, "==", sub, column);
                    Self::branch(g, eq, column)
                }
                _ => self.not_converted(g, a, column),
            },
            1073 | 1074 => self.statistic(g, a, column),
            1080 | 1081 => self.timestamp(g, a, column),
            // Male: Conquer's male bodies are 3 and 4 (TQ: lookface 1003, 1004).
            // An NPC goes to a cell of a map (TQ hides one by sending it to 0, 0).
            301 => match (int(0), int(1), int(2)) {
                (Some(map), Some(x), Some(y)) => {
                    let values = [
                        ("npc", num(a.data)),
                        ("map", num(map)),
                        ("x", num(x)),
                        ("y", num(y)),
                    ];
                    Self::plain(g, "npc.move", &values, column)
                }
                _ => self.not_converted(g, a, column),
            },
            1026 => {
                let body = g.node("player.body", &[], column);
                let male = self.compare_node(g, body, ">", 2, column);
                Self::branch(g, male, column)
            }
            1005 => self.hair(g, a, &tokens, column),
            201 => self.npc_attr(g, a, &tokens, column),
            1071 => match (int(0), tokens.get(1).and_then(|t| t.parse::<u32>().ok())) {
                (Some(seconds), Some(chain)) if chain != 0 => {
                    if self.functions.insert(chain) {
                        self.to_make.push(chain);
                    }
                    self.timers.insert(chain);
                    let values = [
                        ("name", txt(&format!("a{chain}"))),
                        ("seconds", num(seconds)),
                    ];
                    Self::plain(g, "player.start_timer", &values, column)
                }
                _ => self.not_converted(g, a, column),
            },
            // Where the player comes back to; going there.
            1004 => match (int(0), int(1), int(2)) {
                (Some(map), Some(x), Some(y)) => {
                    let values = [("map", num(map)), ("x", num(x)), ("y", num(y))];
                    Self::plain(g, "player.set_record", &values, column)
                }
                _ => self.not_converted(g, a, column),
            },
            1006 => Self::checked(g, "player.go_record", &[], "went", column),
            // Weapon skills, as attributes `weapon_<type>` (level) and `weapon_exp_<type>`.
            1021 => match (tokens.first().copied(), int(1), int(2)) {
                (Some("check"), Some(kind), Some(level)) => {
                    let cmp = self.attr_compare(g, &format!("weapon_{kind}"), ">=", level, column);
                    Self::branch(g, cmp, column)
                }
                (Some("learn"), Some(kind), Some(level)) => {
                    let values = [
                        ("name", txt(&format!("weapon_{kind}"))),
                        ("value", num(level)),
                    ];
                    Self::plain(g, "player.set_attr", &values, column)
                }
                (Some("addexp"), Some(kind), Some(exp)) => {
                    let values = [
                        ("name", txt(&format!("weapon_exp_{kind}"))),
                        ("amount", num(exp)),
                    ];
                    let add = g.node("player.add_attr", &values, column);
                    Emitted {
                        entry: Some(add),
                        ok: Some((add, "then")),
                        fail: None,
                    }
                }
                _ => self.not_converted(g, a, column),
            },
            // Forgetting skills (1045 drops them to level 0: forgotten here too).
            1039 | 1044 | 1045 => {
                let skills: Vec<i64> = tokens.iter().filter_map(|t| t.parse().ok()).collect();
                let mut first = None;
                let mut last: Option<Exit> = None;
                for skill in skills {
                    let node = g.node("skill.forget", &[("skill", num(skill))], column);
                    match last {
                        Some(exit) => g.exec(exit, node),
                        None => first = Some(node),
                    }
                    last = Some((node, "then"));
                }
                match first {
                    Some(entry) => Emitted {
                        entry: Some(entry),
                        ok: last,
                        fail: None,
                    },
                    None => self.not_converted(g, a, column),
                }
            }
            // Items of a range of types: carries some, take some.
            506 | 507 => match (int(0), int(1), int(2)) {
                (Some(low), Some(high), Some(count)) => {
                    let count = count.max(1);
                    if a.kind == 506 {
                        let values = [
                            ("low", num(low)),
                            ("high", num(high)),
                            ("count", num(count)),
                        ];
                        Self::checked(g, "item.take_between", &values, "taken", column)
                    } else {
                        let have = g.node(
                            "item.count_between",
                            &[("low", num(low)), ("high", num(high))],
                            column,
                        );
                        let cmp = self.compare_node(g, have, ">=", count, column);
                        Self::branch(g, cmp, column)
                    }
                }
                _ => self.not_converted(g, a, column),
            },
            508 => {
                let space = g.node("player.bag_space", &[], column);
                let cmp = self.compare_node(g, space, ">=", a.data.max(0), column);
                Self::branch(g, cmp, column)
            }
            // Worn items: worn at all, their level and quality digits, their numbers.
            511 => {
                let worn = g.node("item.worn", &[("slot", num(a.data))], column);
                let none = self.compare_node(g, worn, "==", 0, column);
                let not = g.node("logic.not", &[], column);
                g.data(none, not, "a");
                Self::branch(g, not, column)
            }
            510 => self.equip_test(g, a, &tokens, column),
            512 => match (int(0), int(1)) {
                (Some(slot), Some(color)) => {
                    let values = [
                        ("slot", num(slot)),
                        ("name", txt("color")),
                        ("value", num(color)),
                    ];
                    // TQ answers false whatever happens: the chain goes on through id_nextfail.
                    let set = g.node("item.set_worn_attr", &values, column);
                    Emitted {
                        entry: Some(set),
                        ok: None,
                        fail: Some((set, "then")),
                    }
                }
                _ => self.not_converted(g, a, column),
            },
            517 => self.equip_attr(g, a, &tokens, column),
            // Writing a log row: nothing the game shows or keeps.
            127 if a
                .param
                .to_ascii_uppercase()
                .starts_with("INSERT INTO CQ_LOG") =>
            {
                Self::nothing()
            }
            _ => self.not_converted(g, a, column),
        }
    }

    /// 1005: the hair's colour (hundreds digit) or style (last two digits) set.
    fn hair(&mut self, g: &mut Canvas, a: &Action, tokens: &[&str], column: i32) -> Emitted {
        let (Some(what), Some(n)) = (
            tokens.first(),
            tokens.get(1).and_then(|t| t.parse::<i64>().ok()),
        ) else {
            return self.not_converted(g, a, column);
        };
        let hair = g.node("player.hair", &[], column);
        // Without the part being set, then that part added back.
        let (keep_mod, part) = match what.to_ascii_lowercase().as_str() {
            // h % 100 + (h / 1000) * 1000 + N * 100
            "color" => (100, (n % 10) * 100),
            // h - h % 100 + N % 100 (a style below 100 is 300 more)
            "style" => (100, n % 100),
            _ => return self.not_converted(g, a, column),
        };
        let value = if what.eq_ignore_ascii_case("color") {
            let low = g.node("math.remainder", &[("b", num(keep_mod))], column);
            g.data(hair, low, "a");
            let thousands = g.node("math.divide", &[("b", num(1000))], column);
            g.data(hair, thousands, "a");
            let high = g.node("math.multiply", &[("b", num(1000))], column);
            g.data(thousands, high, "a");
            let sum = g.node("math.add", &[], column);
            g.data(low, sum, "a");
            g.data(high, sum, "b");
            let with = g.node("math.add", &[("b", num(part))], column);
            g.data(sum, with, "a");
            with
        } else {
            let low = g.node("math.remainder", &[("b", num(keep_mod))], column);
            g.data(hair, low, "a");
            let base = g.node("math.subtract", &[], column);
            g.data(hair, base, "a");
            g.data(low, base, "b");
            let with = g.node("math.add", &[("b", num(part))], column);
            g.data(base, with, "a");
            // A hair number below 100 is 300 more (TQ's default hair colour).
            let seq = g.node("flow.sequence", &[], column);
            let low = self.compare_node(g, with, "<", 100, column);
            let branch = g.node("flow.branch", &[], column);
            g.data(low, branch, "condition");
            g.exec((seq, "then 0"), branch);
            let plus = g.node("math.add", &[("b", num(300))], column);
            g.data(with, plus, "a");
            for (out, value) in [("true", plus), ("false", with)] {
                let set = g.node("player.set_hair", &[], column + 1);
                g.data(value, set, "hair");
                g.exec((branch, out), set);
            }
            return Emitted {
                entry: Some(seq),
                ok: Some((seq, "then 1")),
                fail: None,
            };
        };
        let set = g.node("player.set_hair", &[], column);
        g.data(value, set, "hair");
        Emitted {
            entry: Some(set),
            ok: Some((set, "then")),
            fail: None,
        }
    }

    /// 201: an NPC's numbers (`data0`..`data3`, owner), kept as world flags
    /// `npc<id>.<field>` (this NPC's when no id is given).
    fn npc_attr(&mut self, g: &mut Canvas, a: &Action, tokens: &[&str], column: i32) -> Emitted {
        let (Some(field), Some(op), Some(value)) = (
            tokens.first().map(|t| t.to_ascii_lowercase()),
            tokens.get(1).copied(),
            tokens.get(2).and_then(|t| t.parse::<i64>().ok()),
        ) else {
            return self.not_converted(g, a, column);
        };
        if !matches!(
            field.as_str(),
            "data0" | "data1" | "data2" | "data3" | "ownerid" | "ownertype"
        ) {
            return self.not_converted(g, a, column);
        }
        // The flag's name: `npc<id>.<field>`, the id this NPC's unless the action names one.
        let id = tokens
            .get(3)
            .and_then(|t| t.parse::<i64>().ok())
            .filter(|&id| id != 0);
        let name = match id {
            Some(id) => g.node(
                "text.join",
                &[
                    ("a", txt(&format!("npc{id}"))),
                    ("b", txt(&format!(".{field}"))),
                ],
                column,
            ),
            None => {
                let npc = g.node("npc.id", &[], column);
                let prefix = g.node("text.join", &[("a", txt("npc"))], column);
                g.data(npc, prefix, "b");
                let full = g.node("text.join", &[("b", txt(&format!(".{field}")))], column);
                g.data(prefix, full, "a");
                full
            }
        };
        match op {
            "=" => {
                let set = g.node("world.set_flag", &[("value", num(value))], column);
                g.data(name, set, "name");
                Emitted {
                    entry: Some(set),
                    ok: Some((set, "then")),
                    fail: None,
                }
            }
            "+=" => {
                let flag = g.node("world.flag", &[], column);
                g.data(name, flag, "name");
                let add = g.node("math.add", &[("b", num(value))], column);
                g.data(flag, add, "a");
                let set = g.node("world.set_flag", &[], column);
                g.data(name, set, "name");
                g.data(add, set, "value");
                Emitted {
                    entry: Some(set),
                    ok: Some((set, "then")),
                    fail: None,
                }
            }
            "==" | "<" | "<=" | ">" | ">=" => {
                let flag = g.node("world.flag", &[], column);
                g.data(name, flag, "name");
                let cmp = self.compare_node(g, flag, op, value, column);
                Self::branch(g, cmp, column)
            }
            _ => self.not_converted(g, a, column),
        }
    }

    /// Attribute `name` compared with `n` by `op`.
    fn attr_compare(&mut self, g: &mut Canvas, name: &str, op: &str, n: i64, column: i32) -> u32 {
        let attr = g.node("player.attr", &[("name", txt(name))], column);
        self.compare_node(g, attr, op, n, column)
    }

    /// 510: the level (`type % 1000 / 10`) or quality (`type % 10`) of the item worn in a
    /// slot, compared.
    fn equip_test(&mut self, g: &mut Canvas, a: &Action, tokens: &[&str], column: i32) -> Emitted {
        let (Some(slot), Some(what), Some(op), Some(n)) = (
            tokens.first().and_then(|t| t.parse::<i64>().ok()),
            tokens.get(1).map(|t| t.to_ascii_lowercase()),
            tokens.get(2).copied(),
            tokens.get(3).and_then(|t| t.parse::<i64>().ok()),
        ) else {
            return self.not_converted(g, a, column);
        };
        let (modulo, divide) = match what.as_str() {
            "level" => (1000, 10),
            "quality" => (10, 1),
            _ => return self.not_converted(g, a, column),
        };
        let worn = g.node("item.worn", &[("slot", num(slot))], column);
        let rem = g.node("math.remainder", &[("b", num(modulo))], column);
        g.data(worn, rem, "a");
        let value = match divide {
            1 => rem,
            d => {
                let div = g.node("math.divide", &[("b", num(d))], column);
                g.data(rem, div, "a");
                div
            }
        };
        let op = match op {
            "<=" | ">=" => op,
            _ => "==",
        };
        let cmp = self.compare_node(g, value, op, n, column);
        // An empty slot fails.
        let none = self.compare_node(g, worn, "==", 0, column);
        let worn_at_all = g.node("logic.not", &[], column);
        g.data(none, worn_at_all, "a");
        let and = g.node("logic.and", &[], column);
        g.data(worn_at_all, and, "a");
        g.data(cmp, and, "b");
        Self::branch(g, and, column)
    }

    /// 517: a number of the item worn in a slot (`cq_item` column index), set, added to or
    /// compared.
    fn equip_attr(&mut self, g: &mut Canvas, a: &Action, tokens: &[&str], column: i32) -> Emitted {
        let int = |i: usize| tokens.get(i).and_then(|t| t.parse::<i64>().ok());
        let (Some(slot), Some(field), Some(op), Some(value)) =
            (int(0), int(1), tokens.get(2).copied(), int(3))
        else {
            return self.not_converted(g, a, column);
        };
        // `cq_item` columns kept as Conquer's item attributes.
        let name = match field {
            6 => "ident",
            8 => "gem1",
            9 => "gem2",
            12 => "plus",
            13 => "data",
            14 => "bless",
            15 => "enchant",
            20 => "color",
            _ => return self.not_converted(g, a, column),
        };
        match op.to_ascii_lowercase().as_str() {
            "set" => {
                let values = [
                    ("slot", num(slot)),
                    ("name", txt(name)),
                    ("value", num(value)),
                ];
                Self::checked(g, "item.set_worn_attr", &values, "set", column)
            }
            "+=" => {
                let now = g.node(
                    "item.worn_attr",
                    &[("slot", num(slot)), ("name", txt(name))],
                    column,
                );
                let add = g.node("math.add", &[("b", num(value))], column);
                g.data(now, add, "a");
                let set = g.node(
                    "item.set_worn_attr",
                    &[("slot", num(slot)), ("name", txt(name))],
                    column,
                );
                g.data(add, set, "value");
                let branch = g.node("flow.branch", &[], column);
                g.wire((set, "set"), (branch, "condition"));
                g.exec((set, "then"), branch);
                Emitted {
                    entry: Some(set),
                    ok: Some((branch, "true")),
                    fail: Some((branch, "false")),
                }
            }
            op @ ("==" | ">=") => {
                let now = g.node(
                    "item.worn_attr",
                    &[("slot", num(slot)), ("name", txt(name))],
                    column,
                );
                let cmp = self.compare_node(g, now, op, value, column);
                Self::branch(g, cmp, column)
            }
            _ => self.not_converted(g, a, column),
        }
    }

    /// 1080/1081: a statistic's time (flag `ts_<event>_<type>`, seconds): set, moved, compared
    /// with now, or how long ago it was (seconds, or days with a unit).
    fn timestamp(&mut self, g: &mut Canvas, a: &Action, column: i32) -> Emitted {
        let Some((key, rest)) = a.param.split_once(')') else {
            return self.not_converted(g, a, column);
        };
        let key = key
            .trim_start_matches("timestamp(")
            .trim_start_matches("interval(");
        let numbers: Vec<i64> = key
            .split(',')
            .filter_map(|n| n.trim().parse().ok())
            .collect();
        let rest: Vec<&str> = rest.split_whitespace().collect();
        let (Some(&event), Some(&kind), Some(op), Some(n)) = (
            numbers.first(),
            numbers.get(1),
            rest.first().copied(),
            rest.get(1).and_then(|t| t.parse::<i64>().ok()),
        ) else {
            return self.not_converted(g, a, column);
        };
        let name = txt(&format!("ts_{event}_{kind}"));
        let op = op.to_ascii_lowercase();
        if a.kind == 1080 {
            return match op.as_str() {
                "set" if n == 0 => {
                    let clock = g.node("world.clock", &[], column);
                    let set = g.node("player.set_flag", &[("name", name)], column);
                    g.data(clock, set, "value");
                    Emitted {
                        entry: Some(set),
                        ok: Some((set, "then")),
                        fail: None,
                    }
                }
                "set" => Self::plain(
                    g,
                    "player.set_flag",
                    &[("name", name), ("value", num(n))],
                    column,
                ),
                "+=" => {
                    let flag = g.node("player.flag", &[("name", name.clone())], column);
                    let add = g.node("math.add", &[("b", num(n))], column);
                    g.data(flag, add, "a");
                    let set = g.node("player.set_flag", &[("name", name)], column);
                    g.data(add, set, "value");
                    Emitted {
                        entry: Some(set),
                        ok: Some((set, "then")),
                        fail: None,
                    }
                }
                ">=" | ">" | "==" => {
                    let flag = g.node("player.flag", &[("name", name)], column);
                    let reference = match n {
                        0 => g.node("world.clock", &[], column),
                        n => g.node("math.add", &[("a", num(n)), ("b", num(0))], column),
                    };
                    let cmp = self.compare_nodes(g, flag, &op, reference, column);
                    Self::branch(g, cmp, column)
                }
                _ => self.not_converted(g, a, column),
            };
        }
        // 1081: interval(event, type, unit) op n — how long since the time was set.
        let unit = numbers.get(2).copied().unwrap_or(0);
        let per = if unit == 0 { 1 } else { 86_400 };
        let flag = g.node("player.flag", &[("name", name)], column);
        let clock = g.node("world.clock", &[], column);
        let now = g.node("math.divide", &[("b", num(per))], column);
        g.data(clock, now, "a");
        let then = g.node("math.divide", &[("b", num(per))], column);
        g.data(flag, then, "a");
        let due = g.node("math.add", &[("b", num(n))], column);
        g.data(then, due, "a");
        let cmp = self.compare_nodes(g, now, &op, due, column);
        // A time never set fails.
        let unset = self.compare_node(g, flag, "==", 0, column);
        let set = g.node("logic.not", &[], column);
        g.data(unset, set, "a");
        let and = g.node("logic.and", &[], column);
        g.data(set, and, "a");
        g.data(cmp, and, "b");
        Self::branch(g, and, column)
    }

    /// Pure node `a` compared with pure node `b` by `op` (`>=`, `>`, else `==`).
    fn compare_nodes(&mut self, g: &mut Canvas, a: u32, op: &str, b: u32, column: i32) -> u32 {
        let (def, negate) = match op {
            ">=" => ("logic.less", true),
            ">" => ("logic.greater", false),
            _ => ("logic.equal", false),
        };
        let cmp = g.node(def, &[], column);
        g.data(a, cmp, "a");
        g.data(b, cmp, "b");
        if !negate {
            return cmp;
        }
        let not = g.node("logic.not", &[], column);
        g.data(cmp, not, "a");
        not
    }

    /// The item type an action names: its data, else the first type of the name in param.
    fn item_type(&self, a: &Action) -> Option<i64> {
        match a.data {
            0 => self.items.get(&a.param)?.first().map(|&t| t.into()),
            kind => Some(kind),
        }
    }

    /// 122: one of eight chains, picked at random (repeats weigh more), then on.
    fn random_action(&mut self, g: &mut Canvas, a: &Action, column: i32) -> Emitted {
        let ids: Vec<u32> = (a.param.split_whitespace())
            .filter_map(|t| t.parse().ok())
            .collect();
        if ids.len() != 8 {
            return self.not_converted(g, a, column);
        }
        let seq = g.node("flow.sequence", &[], column);
        let pick = g.node("math.random", &[("low", num(0)), ("high", num(7))], column);
        let mut from: Exit = (seq, "then 0");
        for (i, &target) in ids.iter().enumerate() {
            if i == 7 {
                if target != 0 {
                    let call = self.call(g, target, column + 1);
                    g.exec(from, call);
                }
                break;
            }
            let eq = g.node("logic.equal", &[("b", num(i as i64))], column);
            g.data(pick, eq, "a");
            let branch = g.node("flow.branch", &[], column);
            g.data(eq, branch, "condition");
            g.exec(from, branch);
            if target != 0 {
                let call = self.call(g, target, column + 1);
                g.exec((branch, "true"), call);
            }
            from = (branch, "false");
        }
        Emitted {
            entry: Some(seq),
            ok: Some((seq, "then 1")),
            fail: None,
        }
    }

    /// 1001: the player's attributes, read or changed.
    fn user_attr(&mut self, g: &mut Canvas, a: &Action, tokens: &[&str], column: i32) -> Emitted {
        let (Some(attr), Some(op), Some(v)) = (
            tokens.first().map(|t| t.to_ascii_lowercase()),
            tokens.get(1).copied(),
            tokens.get(2).and_then(|t| t.parse::<i64>().ok()),
        ) else {
            return self.not_converted(g, a, column);
        };
        let value_node = match attr.as_str() {
            "money" => Some("player.money"),
            "life" => Some("player.life"),
            "mana" => Some("player.mana"),
            "level" => Some("player.level"),
            "profession" => Some("player.profession"),
            "pk" => Some("player.pk"),
            _ => None,
        };
        // TQ's names of numbers kept as Conquer's attributes.
        let attribute = match attr.as_str() {
            "force" | "strength" => Some("strength"),
            "speed" => Some("agility"),
            "health" => Some("vitality"),
            "soul" => Some("spirit"),
            "addpoint" => Some("attribute_points"),
            "metempsychosis" => Some("rebirths"),
            "virtue" => Some("virtue"),
            "ep" => Some("stamina"),
            "xp" => Some("xp"),
            _ => None,
        };
        if let Some(name) = attribute {
            return match op {
                "+=" => {
                    let values = [("name", txt(name)), ("amount", num(v))];
                    Self::checked(g, "player.add_attr", &values, "added", column)
                }
                "set" => Self::plain(
                    g,
                    "player.set_attr",
                    &[("name", txt(name)), ("value", num(v))],
                    column,
                ),
                "==" | "<" | ">=" | "<=" => {
                    let cmp = self.attr_compare(g, name, op, v, column);
                    Self::branch(g, cmp, column)
                }
                _ => self.not_converted(g, a, column),
            };
        }
        match (attr.as_str(), op) {
            ("money", "+=") if v >= 0 => {
                Self::plain(g, "player.give_money", &[("amount", num(v))], column)
            }
            ("money", "+=") => Self::checked(
                g,
                "player.take_money",
                &[("amount", num(-v))],
                "taken",
                column,
            ),
            ("exp", "+=") => Self::plain(g, "player.give_exp", &[("amount", num(v))], column),
            ("life", "+=") if v >= 0 => {
                Self::plain(g, "player.heal", &[("amount", num(v))], column)
            }
            ("life", "+=") => Self::plain(g, "player.hurt", &[("amount", num(-v))], column),
            ("mana", "+=") => Self::plain(g, "player.add_mana", &[("amount", num(v))], column),
            ("e_money", "+=") if v >= 0 => {
                let values = [
                    ("currency", txt(crate::module::CONQUER_POINTS)),
                    ("amount", num(v)),
                ];
                Self::plain(g, "player.give_currency", &values, column)
            }
            ("e_money", "+=") => {
                let values = [
                    ("currency", txt(crate::module::CONQUER_POINTS)),
                    ("amount", num(-v)),
                ];
                Self::checked(g, "player.take_currency", &values, "taken", column)
            }
            ("e_money", "==" | "<") => {
                let cps = g.node(
                    "player.currency",
                    &[("currency", txt(crate::module::CONQUER_POINTS))],
                    column,
                );
                let cmp = self.compare_node(g, cps, op, v, column);
                Self::branch(g, cmp, column)
            }
            ("iterator", "=") => Self::plain(
                g,
                "player.set_flag",
                &[("name", txt("iterator")), ("value", num(v))],
                column,
            ),
            // TQ's `pk +=` always answers false: the chain goes on through id_nextfail.
            ("pk", "+=") => {
                let id = g.node("player.add_pk", &[("points", num(v))], column);
                Emitted {
                    entry: Some(id),
                    ok: None,
                    fail: Some((id, "then")),
                }
            }
            (_, "==" | "<" | ">=" | "<=") if value_node.is_some() => {
                let cmp = self.compare(g, value_node.unwrap_or_default(), op, v, column);
                Self::branch(g, cmp, column)
            }
            _ => self.not_converted(g, a, column),
        }
    }

    /// 1060/1061/1064: the player's nine scratch variables (`var(i)`), as flags `var<i>`.
    fn variable(&mut self, g: &mut Canvas, a: &Action, tokens: &[&str], column: i32) -> Emitted {
        let index = tokens.first().and_then(|t| {
            t.strip_prefix("var(")?
                .strip_suffix(')')?
                .parse::<u32>()
                .ok()
        });
        let (Some(index), Some(op), Some(v)) = (
            index,
            tokens.get(1).copied(),
            tokens.get(2).and_then(|t| t.parse::<i64>().ok()),
        ) else {
            return self.not_converted(g, a, column);
        };
        let name = txt(&format!("var{index}"));
        match a.kind {
            1060 => {
                let flag = g.node("player.flag", &[("name", name)], column);
                let op = if matches!(op, ">" | ">=") { op } else { "==" };
                let cmp = self.compare_node(g, flag, op, v, column);
                Self::branch(g, cmp, column)
            }
            1061 => Self::plain(
                g,
                "player.set_flag",
                &[("name", name), ("value", num(v))],
                column,
            ),
            _ => {
                let def = match op {
                    "+=" => "math.add",
                    "-=" => "math.subtract",
                    "*=" => "math.multiply",
                    "/=" if v != 0 => "math.divide",
                    "mod=" if v != 0 => "math.remainder",
                    _ => return self.not_converted(g, a, column),
                };
                let flag = g.node("player.flag", &[("name", name.clone())], column);
                let calc = g.node(def, &[("b", num(v))], column);
                g.data(flag, calc, "a");
                let set = g.node("player.set_flag", &[("name", name)], column);
                g.data(calc, set, "value");
                Emitted {
                    entry: Some(set),
                    ok: Some((set, "then")),
                    fail: None,
                }
            }
        }
    }

    /// 1073/1074: the player's statistics (`stc(event, type)`), as flags `stc_<event>_<type>`.
    fn statistic(&mut self, g: &mut Canvas, a: &Action, column: i32) -> Emitted {
        // `stc(2,2) op n [save]`, spaces anywhere inside the brackets.
        let Some((key, rest)) = a.param.split_once(')') else {
            return self.not_converted(g, a, column);
        };
        let numbers: Vec<i64> = (key.trim_start_matches("stc(").split(','))
            .filter_map(|n| n.trim().parse().ok())
            .collect();
        let rest: Vec<&str> = rest.split_whitespace().collect();
        let (&[event, kind], Some(op), Some(v)) = (
            numbers.as_slice(),
            rest.first().copied(),
            rest.get(1).and_then(|t| t.parse::<i64>().ok()),
        ) else {
            return self.not_converted(g, a, column);
        };
        let name = txt(&format!("stc_{event}_{kind}"));
        match (a.kind, op) {
            (1073, op) => {
                let flag = g.node("player.flag", &[("name", name)], column);
                let op = if matches!(op, ">" | ">=") { op } else { "==" };
                let cmp = self.compare_node(g, flag, op, v, column);
                Self::branch(g, cmp, column)
            }
            (_, "=") => Self::plain(
                g,
                "player.set_flag",
                &[("name", name), ("value", num(v))],
                column,
            ),
            (_, "+=") => {
                let flag = g.node("player.flag", &[("name", name.clone())], column);
                let add = g.node("math.add", &[("b", num(v))], column);
                g.data(flag, add, "a");
                let set = g.node("player.set_flag", &[("name", name)], column);
                g.data(add, set, "value");
                Emitted {
                    entry: Some(set),
                    ok: Some((set, "then")),
                    fail: None,
                }
            }
            _ => self.not_converted(g, a, column),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(id: u32, next: u32, fail: u32, kind: u32, data: i64, param: &str) -> Action {
        Action {
            id,
            next,
            fail,
            kind,
            data,
            param: param.into(),
        }
    }

    fn task(id: u32, next: u32) -> Task {
        Task {
            id,
            next,
            fail: 0,
            items: [String::new(), String::new()],
            money: 0,
            profession: 0,
            min_pk: -100_000,
            max_pk: 100_000,
        }
    }

    #[test]
    fn a_talking_npc_becomes_a_blueprint_that_compiles() {
        let mut logic = NpcLogic::default();
        for a in [
            action(10, 11, 0, 101, 0, "Hello~%user_name!"),
            action(11, 12, 0, 102, 0, "Teleport~me 20"),
            action(12, 13, 0, 102, 0, "Bye 0"),
            action(13, 0, 0, 120, 0, ""),
            action(30, 31, 32, 1001, 0, "money += -100"),
            action(31, 0, 0, 1003, 0, "1002 400 400"),
            action(32, 0, 0, 101, 0, "You~are~poor."),
            action(40, 0, 0, 9999, 0, "odd"),
        ] {
            logic.actions.insert(a.id, a);
        }
        logic.tasks.insert(1, task(1, 10));
        logic.tasks.insert(20, task(20, 30));
        let npcs = BTreeMap::from([(7, vec![1, 0]), (8, vec![1])]);
        let converted = convert(&logic, &npcs, &HashMap::new());
        assert_eq!(converted.blueprints.len(), 1);
        assert_eq!(converted.scripts[&7], converted.scripts[&8]);
        let (name, bp) = &converted.blueprints[0];
        let script = c3_script::blueprint::compile(name, bp).expect("compiles");
        assert!(script.contains("Hello {player}!"), "{script}");
        assert!(script.contains("task_20"), "{script}");
        assert!(script.contains("teleport(1002"), "{script}");
    }

    #[test]
    fn converts_the_tq_dump_and_compiles() {
        let dump =
            std::path::Path::new(r"D:\Conquer2\Tools\eosource\tq_re\db\cq_finalized_mariadb.sql");
        let Ok(bytes) = std::fs::read(dump) else {
            return;
        };
        let sql = encoding_rs::GBK.decode(&bytes).0;
        let logic = NpcLogic::from_sql(&sql);
        let population = crate::server_data::ServerPopulation::from_sql(&sql);
        let npcs: BTreeMap<u32, Vec<u32>> = (population.maps.values())
            .flat_map(|p| &p.npcs)
            .map(|n| (n.id, crate::server_data::npc_tasks(n)))
            .collect();
        let converted = convert(&logic, &npcs, &HashMap::new());
        let host = c3_script::ScriptHost::new(std::env::temp_dir(), None);
        let failing: Vec<String> = (converted.blueprints.iter())
            .filter_map(|(name, bp)| match c3_script::blueprint::compile(name, bp) {
                Err(p) => Some(format!("{name}: {:?}", p.first())),
                // The game's script engine reads it too.
                Ok(script) => (host.check(&script, None).into_iter())
                    .next()
                    .map(|d| format!("{name}: {}", d.message)),
            })
            .collect();
        eprintln!("{}", converted.report.join("\n"));
        // Every value set names an input of its node.
        for (name, bp) in &converted.blueprints {
            let graphs = std::iter::once(&bp.events).chain(bp.functions.iter().map(|f| &f.graph));
            for node in graphs.flat_map(|g| &g.nodes) {
                let def = c3_script::blueprint::node_def(&node.def).expect("a library node");
                for key in node.values.keys() {
                    let named = matches!(key.as_str(), "name" | "text");
                    assert!(
                        def.input(key).is_some() || named,
                        "{name}: node {} ({}) has no input {key}",
                        node.id,
                        node.def
                    );
                }
            }
        }
        assert!(
            failing.is_empty(),
            "{} fail, first {:?}",
            failing.len(),
            failing.first()
        );
        assert!(converted.blueprints.len() > 100);
    }
}
