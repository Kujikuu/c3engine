//! Blueprint → Rhai. Each event node becomes the script function the game calls, following
//! its exec wires: actions are statements, branches `if`, loops `for`; inputs are the pure
//! nodes wired into them, worked out in place, or the outputs of nodes that already ran. A
//! wait starts a timer and the rest of the chain becomes its own function, run from
//! `on_timer`. Whatever doesn't fit is a [`Problem`] naming its node.

use super::library::{DefKind, NodeDef, node_def};
use super::model::{Blueprint, Graph, GraphNode, Literal, PinType, VariableScope};
use std::collections::BTreeSet;
use std::fmt::Write;

/// Which graph of a blueprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphRef {
    Events,
    Function(String),
}

/// Something wrong in a graph, at a node when it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub graph: GraphRef,
    pub node: Option<u32>,
    pub message: String,
}

/// The timer name a wait of blueprint `name` resumes by.
fn wait_timer(name: &str, node: u32) -> String {
    format!("bp:{name}:{node}")
}

/// `name` as part of a Rhai identifier.
fn ident(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn literal(value: &Literal) -> String {
    match value {
        Literal::Number(n) if n.fract() == 0.0 && n.abs() < 9.0e15 => format!("{}", *n as i64),
        Literal::Number(n) => format!("{n}"),
        Literal::Text(t) => quote(t),
        Literal::Bool(b) => b.to_string(),
    }
}

fn default_of(ty: PinType) -> &'static str {
    match ty {
        PinType::Text => "\"\"",
        PinType::Bool => "false",
        PinType::List => "[]",
        _ => "0",
    }
}

/// The Rhai of blueprint `name`, or what is wrong with it.
pub fn compile(name: &str, blueprint: &Blueprint) -> Result<String, Vec<Problem>> {
    let mut c = Compiler {
        name,
        blueprint,
        problems: Vec::new(),
        waits: Vec::new(),
    };
    let mut out =
        format!("// Made from blueprint \"{name}\"; edit the blueprint, not this file.\n");
    c.events(&mut out);
    c.functions(&mut out);
    if c.problems.is_empty() {
        Ok(out)
    } else {
        Err(c.problems)
    }
}

struct Compiler<'a> {
    name: &'a str,
    blueprint: &'a Blueprint,
    problems: Vec<Problem>,
    /// Waits met so far: their timer names and the functions going on after them.
    waits: Vec<(String, String)>,
}

/// One function body being written.
struct Body<'a> {
    graph: &'a Graph,
    which: GraphRef,
    code: String,
    depth: usize,
    /// Outputs of nodes that have run, readable from here on.
    known: BTreeSet<(u32, String)>,
    /// Nodes on the exec path being written (a loop back is a problem).
    path: Vec<u32>,
    /// Bodies split off at waits: (function name, the wait node, its graph).
    rest: Vec<(String, u32)>,
}

impl<'a> Body<'a> {
    fn new(graph: &'a Graph, which: GraphRef, depth: usize) -> Self {
        Self {
            graph,
            which,
            code: String::new(),
            depth,
            known: BTreeSet::new(),
            path: Vec::new(),
            rest: Vec::new(),
        }
    }

    fn line(&mut self, text: &str) {
        let _ = writeln!(self.code, "{}{text}", "    ".repeat(self.depth));
    }
}

impl<'a> Compiler<'a> {
    fn problem(&mut self, which: &GraphRef, node: Option<u32>, message: impl Into<String>) {
        self.problems.push(Problem {
            graph: which.clone(),
            node,
            message: message.into(),
        });
    }

    /// Declarations of the blueprint's local variables, at the top of each function.
    fn locals(&self, body: &mut Body) {
        for v in &self.blueprint.variables {
            if v.scope == VariableScope::Local {
                body.line(&format!(
                    "let v_{} = {};",
                    ident(&v.name),
                    literal(&v.default)
                ));
            }
        }
    }

    fn events(&mut self, out: &mut String) {
        let graph = &self.blueprint.events;
        let mut seen: Vec<String> = Vec::new();
        let mut timer_chain = None;
        for node in &graph.nodes {
            let Some(def) = self.def(&GraphRef::Events, node) else {
                continue;
            };
            let DefKind::Event { function } = def.kind else {
                if def.kind == DefKind::FunctionEntry || def.kind == DefKind::Return {
                    self.problem(
                        &GraphRef::Events,
                        Some(node.id),
                        "Only a function's graph has this node",
                    );
                }
                continue;
            };
            let function = match def.id {
                "event.custom" => match node.values.get("name") {
                    Some(Literal::Text(n)) if !n.trim().is_empty() => ident(n.trim()),
                    _ => {
                        self.problem(&GraphRef::Events, Some(node.id), "Name the event");
                        continue;
                    }
                },
                _ => function.to_owned(),
            };
            if seen.contains(&function) {
                self.problem(
                    &GraphRef::Events,
                    Some(node.id),
                    format!("A second \"{}\" event", def.title),
                );
                continue;
            }
            seen.push(function.clone());
            if function == "on_timer" {
                timer_chain = Some(node.id);
                continue;
            }
            let params: Vec<String> = def
                .outputs
                .iter()
                .map(|(p, _)| format!("p_{}", ident(p)))
                .collect();
            let _ = writeln!(out, "\nfn {function}({}) {{", params.join(", "));
            let mut body = Body::new(graph, GraphRef::Events, 1);
            self.locals(&mut body);
            self.chain(&mut body, node.id, "then");
            out.push_str(&body.code);
            out.push_str("}\n");
            self.rest_of(out, body.rest, graph, GraphRef::Events);
        }
        // `on_timer` runs the event's own graph and resumes the waits (all known once the
        // timer's own chain is written too).
        let mut timer_body = String::new();
        let mut timer_rest = String::new();
        if let Some(id) = timer_chain {
            let mut body = Body::new(graph, GraphRef::Events, 1);
            self.locals(&mut body);
            self.chain(&mut body, id, "then");
            timer_body = body.code;
            self.rest_of(&mut timer_rest, body.rest, graph, GraphRef::Events);
        }
        if timer_chain.is_some() || !self.waits.is_empty() {
            out.push_str("\nfn on_timer(p_name) {\n");
            for (timer, function) in &self.waits {
                let _ = writeln!(
                    out,
                    "    if p_name == {} {{ {function}(); return; }}",
                    quote(timer)
                );
            }
            out.push_str(&timer_body);
            out.push_str("}\n");
            out.push_str(&timer_rest);
        }
    }

    /// The functions going on after waits (and after waits in those, in turn).
    fn rest_of(
        &mut self,
        out: &mut String,
        rest: Vec<(String, u32)>,
        graph: &Graph,
        which: GraphRef,
    ) {
        let mut todo = rest;
        while let Some((function, wait)) = todo.pop() {
            let _ = writeln!(out, "\nfn {function}() {{");
            let mut body = Body::new(graph, which.clone(), 1);
            self.locals(&mut body);
            self.chain(&mut body, wait, "then");
            out.push_str(&body.code);
            out.push_str("}\n");
            todo.extend(body.rest);
        }
    }

    fn functions(&mut self, out: &mut String) {
        let blueprint = self.blueprint;
        for f in &blueprint.functions {
            let which = GraphRef::Function(f.name.clone());
            let entries: Vec<&GraphNode> = (f.graph.nodes.iter())
                .filter(|n| n.def == "flow.entry")
                .collect();
            let entry = match entries.as_slice() {
                [one] => one.id,
                [] => {
                    self.problem(&which, None, "The function has no start node");
                    continue;
                }
                [_, second, ..] => {
                    self.problem(&which, Some(second.id), "A function has one start node");
                    continue;
                }
            };
            let params: Vec<String> = f
                .inputs
                .iter()
                .map(|(p, _)| format!("p_{}", ident(p)))
                .collect();
            let _ = writeln!(out, "\nfn f_{}({}) {{", ident(&f.name), params.join(", "));
            let mut body = Body::new(&f.graph, which.clone(), 1);
            self.locals(&mut body);
            self.chain(&mut body, entry, "then");
            out.push_str(&body.code);
            out.push_str("}\n");
            if !body.rest.is_empty() {
                self.problem(&which, None, "Functions can't wait (only events can)");
            }
        }
    }

    fn def(&mut self, which: &GraphRef, node: &GraphNode) -> Option<&'static NodeDef> {
        let def = node_def(&node.def);
        if def.is_none() {
            self.problem(
                which,
                Some(node.id),
                format!("No such node \"{}\"", node.def),
            );
        }
        def
    }

    /// Writes what runs after exec out `pin` of `node`.
    fn chain(&mut self, body: &mut Body, node: u32, pin: &str) {
        let next: Vec<u32> = body
            .graph
            .wires_from(node, pin)
            .map(|w| w.to.node)
            .collect();
        match next.as_slice() {
            [] => {}
            [one] => self.run(body, *one),
            [_, second, ..] => {
                let which = body.which.clone();
                self.problem(
                    &which,
                    Some(*second),
                    "An exec pin leads to one node; use a Sequence",
                );
            }
        }
    }

    /// Writes `id` (a node that runs) and what follows it.
    fn run(&mut self, body: &mut Body, id: u32) {
        let which = body.which.clone();
        if body.path.contains(&id) {
            self.problem(&which, Some(id), "The exec wires loop back here");
            return;
        }
        let Some(node) = body.graph.node(id) else {
            self.problem(&which, None, format!("A wire leads to a missing node {id}"));
            return;
        };
        let Some(def) = self.def(&which, node) else {
            return;
        };
        if !def.runs() {
            self.problem(
                &which,
                Some(id),
                format!("\"{}\" doesn't run in exec order", def.title),
            );
            return;
        }
        body.path.push(id);
        match def.kind {
            DefKind::Action => {
                let code = self.fill(body, node, def);
                match def.outputs.first() {
                    Some((out, _)) => {
                        body.line(&format!("let n{id}_{} = {code};", ident(out)));
                        body.known.insert((id, (*out).to_owned()));
                    }
                    None => body.line(&code),
                }
                self.chain(body, id, "then");
            }
            DefKind::Branch => {
                let cond = self.input(body, node, "condition", PinType::Bool);
                body.line(&format!("if {cond} {{"));
                body.depth += 1;
                self.chain(body, id, "true");
                body.depth -= 1;
                body.line("} else {");
                body.depth += 1;
                self.chain(body, id, "false");
                body.depth -= 1;
                body.line("}");
            }
            DefKind::Sequence => {
                for pin in def.exec_outs() {
                    self.chain(body, id, pin);
                }
            }
            DefKind::ForEach => {
                let list = self.input(body, node, "list", PinType::List);
                body.line(&format!("for n{id}_item in {list} {{"));
                body.known.insert((id, "item".to_owned()));
                body.depth += 1;
                self.chain(body, id, "body");
                body.depth -= 1;
                body.line("}");
                self.chain(body, id, "done");
            }
            DefKind::Wait => {
                let seconds = self.input(body, node, "seconds", PinType::Number);
                let timer = wait_timer(self.name, id);
                let function = format!("wait_{id}");
                body.line(&format!("start_timer({}, {seconds});", quote(&timer)));
                self.waits.push((timer, function.clone()));
                body.rest.push((function, id));
            }
            DefKind::SetVariable => {
                if let Some(v) = self.variable(body, node) {
                    let value = self.input(body, node, "value", v.ty);
                    match v.scope {
                        VariableScope::Local => {
                            body.line(&format!("v_{} = {value};", ident(&v.name)))
                        }
                        VariableScope::Player => {
                            body.line(&format!("set_flag({}, {value});", quote(&v.name)));
                        }
                    }
                }
                self.chain(body, id, "then");
            }
            DefKind::Call => {
                if let Some(f) = self.function(body, node) {
                    let args: Vec<String> = (f.inputs.iter())
                        .map(|(pin, ty)| self.input(body, node, pin, *ty))
                        .collect();
                    let call = format!("f_{}({})", ident(&f.name), args.join(", "));
                    match f.output {
                        Some(_) => {
                            body.line(&format!("let n{id}_value = {call};"));
                            body.known.insert((id, "value".to_owned()));
                        }
                        None => body.line(&format!("{call};")),
                    }
                }
                self.chain(body, id, "then");
            }
            DefKind::Return => {
                let output = match &which {
                    GraphRef::Function(name) => (self.blueprint.functions.iter())
                        .find(|f| &f.name == name)
                        .and_then(|f| f.output),
                    GraphRef::Events => None,
                };
                match output {
                    Some(ty) => {
                        let value = self.input(body, node, "value", ty);
                        body.line(&format!("return {value};"));
                    }
                    None => body.line("return;"),
                }
            }
            DefKind::Event { .. }
            | DefKind::Pure
            | DefKind::GetVariable
            | DefKind::FunctionEntry => {}
        }
        body.path.pop();
    }

    /// `def`'s code with its inputs filled in.
    fn fill(&mut self, body: &mut Body, node: &GraphNode, def: &NodeDef) -> String {
        let mut code = def.code.to_owned();
        for (pin, ty) in def.inputs {
            let value = self.input(body, node, pin, *ty);
            code = code.replace(&format!("{{{pin}}}"), &value);
        }
        code
    }

    /// What flows into input `pin` (of type `ty`) of `node`: a wired node's value, else the
    /// typed-in value, else the type's default.
    fn input(&mut self, body: &mut Body, node: &GraphNode, pin: &str, ty: PinType) -> String {
        let which = body.which.clone();
        let Some(wire) = body.graph.wire_into(node.id, pin) else {
            return node
                .values
                .get(pin)
                .map_or_else(|| default_of(ty).to_owned(), literal);
        };
        let (from, out) = (wire.from.node, wire.from.pin.clone());
        let Some(source) = body.graph.node(from) else {
            self.problem(
                &which,
                Some(node.id),
                format!("Input \"{pin}\" is wired to a missing node"),
            );
            return default_of(ty).to_owned();
        };
        let Some(def) = self.def(&which, source) else {
            return default_of(ty).to_owned();
        };
        if let Some(given) = self.output_type(body, source, def, &out)
            && !given.fits(ty)
        {
            self.problem(
                &which,
                Some(node.id),
                format!("Input \"{pin}\" takes {}, not {}", ty.name(), given.name()),
            );
        }
        match def.kind {
            DefKind::Pure => {
                if body.path.contains(&from) {
                    self.problem(&which, Some(from), "The data wires loop back here");
                    return default_of(ty).to_owned();
                }
                body.path.push(from);
                let code = self.fill(body, source, def);
                body.path.pop();
                code
            }
            DefKind::GetVariable => match self.variable(body, source) {
                Some(v) => match v.scope {
                    VariableScope::Local => format!("v_{}", ident(&v.name)),
                    VariableScope::Player => format!("flag({})", quote(&v.name)),
                },
                None => default_of(ty).to_owned(),
            },
            DefKind::Event { .. } | DefKind::FunctionEntry => format!("p_{}", ident(&out)),
            _ => {
                if body.known.contains(&(from, out.clone())) {
                    format!("n{from}_{}", ident(&out))
                } else {
                    self.problem(
                        &which,
                        Some(node.id),
                        format!(
                            "Input \"{pin}\" reads \"{}\", which hasn't run yet here",
                            def.title
                        ),
                    );
                    default_of(ty).to_owned()
                }
            }
        }
    }

    /// The type of output `pin` of `node` (variables and functions say their own).
    fn output_type(
        &mut self,
        body: &Body,
        node: &GraphNode,
        def: &NodeDef,
        pin: &str,
    ) -> Option<PinType> {
        match def.kind {
            DefKind::GetVariable => {
                let name = text_value(node, "name")?;
                (self.blueprint.variables.iter())
                    .find(|v| v.name == name)
                    .map(|v| v.ty)
            }
            DefKind::FunctionEntry => match &body.which {
                GraphRef::Function(f) => (self.blueprint.functions.iter())
                    .find(|x| &x.name == f)?
                    .inputs
                    .iter()
                    .find(|(p, _)| p == pin)
                    .map(|(_, t)| *t),
                GraphRef::Events => None,
            },
            DefKind::Call => {
                let name = text_value(node, "name")?;
                (self.blueprint.functions.iter())
                    .find(|f| f.name == name)?
                    .output
            }
            _ => def.output(pin),
        }
    }

    fn variable(&mut self, body: &Body, node: &GraphNode) -> Option<&'a super::model::Variable> {
        let blueprint = self.blueprint;
        let found =
            text_value(node, "name").and_then(|n| blueprint.variables.iter().find(|v| v.name == n));
        if found.is_none() {
            self.problem(&body.which, Some(node.id), "Pick a variable");
        }
        found
    }

    fn function(&mut self, body: &Body, node: &GraphNode) -> Option<&'a super::model::Function> {
        let blueprint = self.blueprint;
        let found =
            text_value(node, "name").and_then(|n| blueprint.functions.iter().find(|f| f.name == n));
        if found.is_none() {
            self.problem(&body.which, Some(node.id), "Pick a function");
        }
        found
    }
}

fn text_value<'n>(node: &'n GraphNode, key: &str) -> Option<&'n str> {
    match node.values.get(key) {
        Some(Literal::Text(t)) => Some(t.as_str()),
        _ => None,
    }
}
