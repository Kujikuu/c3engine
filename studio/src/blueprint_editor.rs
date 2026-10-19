//! The Blueprints panel: a project's blueprints (game rules and behaviours as node graphs)
//! edited on an Unreal-style canvas (egui-snarl). Right-click the canvas for the node palette,
//! drag from pin to pin to wire (types must fit), type values into unwired inputs. Variables
//! and functions are on the left; problems name their node and outline it. Each change is
//! one undo step, saved to the `.c3bp` and compiled to its script at once.

use crate::app::{Focus, Studio};
use crate::icons;
use crate::theme;
use crate::value_form::Touch;
use c3_edit::ValueDoc;
use c3_script::blueprint::{
    self, Blueprint, DefKind, EXEC_IN, Function, Graph, GraphNode, GraphRef, Literal, NODES,
    NodeDef, PinRef, PinType, Problem, Variable, VariableScope, Wire, node_def, pins,
};
use eframe::egui::{self, Color32, DragValue, RichText, Stroke};
use egui_snarl::ui::{PinInfo, SnarlStyle, SnarlViewer, SnarlWidget};
use egui_snarl::{InPin, NodeId, OutPin, Snarl};
use std::collections::HashMap;

/// The blueprint open for editing.
pub(crate) struct BlueprintEditor {
    pub name: String,
    doc: ValueDoc<Blueprint>,
    /// Which graph is on the canvas.
    graph: GraphRef,
    /// The canvas: one snarl node per graph node (its id), rebuilt when the blueprint changes.
    snarl: Snarl<u32>,
    /// The change count the canvas was built at (`None`: to build).
    built: Option<u64>,
    revision: u64,
    problems: Vec<Problem>,
    /// Nodes being dragged: the move is one undo step.
    moving: bool,
    /// What the palette is searching for.
    search: String,
    /// Nodes Play stops after (not part of the blueprint: no undo).
    breakpoints: std::collections::BTreeSet<u32>,
    /// The last "Find uses" search: what was looked for and where it is.
    references: Option<(String, Vec<blueprint::NodeAt>)>,
    /// A node to point out (a reference followed), outlined.
    focus: Option<u32>,
}

/// What the canvas asked for, carried out after it is drawn.
enum Edit {
    Add {
        def: &'static str,
        pos: [f32; 2],
    },
    Remove(u32),
    Connect(Wire),
    Disconnect(Wire),
    Value {
        node: u32,
        key: String,
        value: Option<Literal>,
    },
    /// Stop (or no longer stop) Play after this node runs.
    Breakpoint(u32),
    /// Move the selected nodes (none selected: this one) into a new function.
    Collapse(u32),
    /// Look for the nodes like this one (the same library node and name).
    FindUses(u32),
}

/// What Play reported about this blueprint: how recently each node ran (0 just now, 1 long
/// ago) and the values outputs took.
#[derive(Default)]
struct Live {
    ran: HashMap<u32, f32>,
    watches: HashMap<(u32, String), String>,
}

/// Traces back that count as "just ran" (the lit path fades over them).
const LIT_TRACES: f32 = 48.0;

impl BlueprintEditor {
    fn new(name: String, blueprint: Blueprint) -> Self {
        Self {
            name,
            doc: ValueDoc::new(blueprint),
            graph: GraphRef::Events,
            snarl: Snarl::new(),
            built: None,
            revision: 0,
            problems: Vec::new(),
            moving: false,
            search: String::new(),
            breakpoints: Default::default(),
            references: None,
            focus: None,
        }
    }

    fn graph_of<'b>(blueprint: &'b Blueprint, which: &GraphRef) -> Option<&'b Graph> {
        match which {
            GraphRef::Events => Some(&blueprint.events),
            GraphRef::Function(name) => (blueprint.functions.iter())
                .find(|f| &f.name == name)
                .map(|f| &f.graph),
        }
    }

    fn graph_mut<'b>(blueprint: &'b mut Blueprint, which: &GraphRef) -> Option<&'b mut Graph> {
        match which {
            GraphRef::Events => Some(&mut blueprint.events),
            GraphRef::Function(name) => (blueprint.functions.iter_mut())
                .find(|f| &f.name == name)
                .map(|f| &mut f.graph),
        }
    }

    /// The canvas as the blueprint now is; snarl node ids by graph node id.
    fn rebuild(&mut self) -> HashMap<u32, NodeId> {
        let mut snarl = Snarl::new();
        let mut ids = HashMap::new();
        let blueprint = self.doc.value();
        let Some(graph) = Self::graph_of(blueprint, &self.graph) else {
            self.snarl = snarl;
            return ids;
        };
        for node in &graph.nodes {
            let id = snarl.insert_node(egui::pos2(node.pos[0], node.pos[1]), node.id);
            ids.insert(node.id, id);
        }
        for wire in &graph.wires {
            let (Some(&from), Some(&to)) = (ids.get(&wire.from.node), ids.get(&wire.to.node))
            else {
                continue;
            };
            let (Some(out), Some(input)) = (
                pin_index(blueprint, &self.graph, graph, &wire.from, false),
                pin_index(blueprint, &self.graph, graph, &wire.to, true),
            ) else {
                continue;
            };
            snarl.connect(
                egui_snarl::OutPinId {
                    node: from,
                    output: out,
                },
                egui_snarl::InPinId { node: to, input },
            );
        }
        self.snarl = snarl;
        self.built = Some(self.revision);
        ids
    }
}

/// The index of pin `pin` among its node's inputs (or outputs).
fn pin_index(
    blueprint: &Blueprint,
    which: &GraphRef,
    graph: &Graph,
    pin: &PinRef,
    input: bool,
) -> Option<usize> {
    let node = graph.node(pin.node)?;
    let def = node_def(&node.def)?;
    let pins = pins(blueprint, which, node, def);
    let list = if input { &pins.inputs } else { &pins.outputs };
    list.iter().position(|(n, _)| n == &pin.pin)
}

/// The colour a pin type is drawn in, as Unreal colours them.
fn pin_color(ty: PinType) -> Color32 {
    match ty {
        PinType::Exec => Color32::WHITE,
        PinType::Number => Color32::from_rgb(110, 210, 140),
        PinType::Text => Color32::from_rgb(240, 100, 200),
        PinType::Bool => Color32::from_rgb(200, 60, 60),
        PinType::Entity => Color32::from_rgb(70, 160, 240),
        PinType::Item => Color32::from_rgb(240, 180, 70),
        PinType::Map => Color32::from_rgb(160, 120, 230),
        PinType::List => Color32::from_rgb(120, 200, 220),
        PinType::Record => Color32::from_rgb(90, 110, 220),
    }
}

/// The header colour of a palette group.
fn category_color(category: &str) -> Color32 {
    match category {
        "Events" => Color32::from_rgb(150, 40, 40),
        "Formulas" => Color32::from_rgb(130, 60, 140),
        "Flow" => Color32::from_rgb(70, 70, 70),
        "Math" | "Logic" | "Text" => Color32::from_rgb(40, 110, 90),
        _ => Color32::from_rgb(40, 80, 140),
    }
}

/// Draws graph nodes and turns what the user does into [`Edit`]s.
struct Viewer<'a> {
    blueprint: &'a Blueprint,
    which: GraphRef,
    graph: &'a Graph,
    problems: HashMap<u32, String>,
    edits: Vec<Edit>,
    touch: &'a mut Touch,
    search: &'a mut String,
    live: Live,
    breakpoints: &'a std::collections::BTreeSet<u32>,
    focus: Option<u32>,
}

impl Viewer<'_> {
    fn node(&self, snarl: &Snarl<u32>, id: NodeId) -> Option<(&GraphNode, &'static NodeDef)> {
        let node = self.graph.node(*snarl.get_node(id)?)?;
        Some((node, node_def(&node.def)?))
    }

    fn pin_info(ty: PinType, wired: bool) -> PinInfo {
        let color = pin_color(ty);
        let info = if ty == PinType::Exec {
            PinInfo::triangle()
        } else {
            PinInfo::circle()
        };
        let fill = if wired {
            color
        } else {
            color.gamma_multiply(0.35)
        };
        info.with_fill(fill)
            .with_stroke(Stroke::new(1.0, color))
            .with_wire_color(color)
    }

    /// The input pin `pin` names, as a graph pin.
    fn graph_pin(
        &self,
        snarl: &Snarl<u32>,
        node: NodeId,
        index: usize,
        input: bool,
    ) -> Option<(PinRef, PinType)> {
        let (n, def) = self.node(snarl, node)?;
        let pins = pins(self.blueprint, &self.which, n, def);
        let (name, ty) = if input {
            pins.inputs.get(index)?
        } else {
            pins.outputs.get(index)?
        };
        Some((
            PinRef {
                node: n.id,
                pin: name.clone(),
            },
            *ty,
        ))
    }

    /// An editor for an unwired input's typed-in value.
    fn value_field(&mut self, ui: &mut egui::Ui, node: &GraphNode, pin: &str, ty: PinType) {
        let current = node.values.get(pin);
        let mut set = None;
        match ty {
            PinType::Number | PinType::Entity | PinType::Item | PinType::Map => {
                let mut n = match current {
                    Some(Literal::Number(n)) => *n,
                    _ => 0.0,
                };
                let r = self.touch.track(ui.add(DragValue::new(&mut n).speed(0.5)));
                if r.changed() {
                    set = Some(Literal::Number(n));
                }
            }
            PinType::Text => {
                let mut t = match current {
                    Some(Literal::Text(t)) => t.clone(),
                    _ => String::new(),
                };
                let r = self
                    .touch
                    .track(ui.add(egui::TextEdit::singleline(&mut t).desired_width(90.0)));
                if r.changed() {
                    set = Some(Literal::Text(t));
                }
            }
            PinType::Bool => {
                let mut b = matches!(current, Some(Literal::Bool(true)));
                if ui.checkbox(&mut b, "").changed() {
                    self.touch.click();
                    set = Some(Literal::Bool(b));
                }
            }
            PinType::Exec | PinType::List | PinType::Record => {}
        }
        if let Some(value) = set {
            self.edits.push(Edit::Value {
                node: node.id,
                key: pin.to_owned(),
                value: Some(value),
            });
        }
    }

    /// A picker for what a variable, call or custom event names.
    fn name_field(&mut self, ui: &mut egui::Ui, node: &GraphNode, def: &NodeDef) {
        let current = match node.values.get("name") {
            Some(Literal::Text(t)) => t.clone(),
            _ => String::new(),
        };
        let options: Vec<String> = match def.kind {
            DefKind::GetVariable | DefKind::SetVariable => self
                .blueprint
                .variables
                .iter()
                .map(|v| v.name.clone())
                .collect(),
            DefKind::Call => self
                .blueprint
                .functions
                .iter()
                .map(|f| f.name.clone())
                .collect(),
            _ => {
                // A custom event: its name is typed.
                let mut name = current.clone();
                let r = self.touch.track(
                    ui.add(
                        egui::TextEdit::singleline(&mut name)
                            .hint_text("name")
                            .desired_width(110.0),
                    ),
                );
                if r.changed() {
                    self.edits.push(Edit::Value {
                        node: node.id,
                        key: "name".into(),
                        value: Some(Literal::Text(name)),
                    });
                }
                return;
            }
        };
        let shown = if current.is_empty() {
            "pick…"
        } else {
            &current
        };
        egui::ComboBox::from_id_salt(("bp name", node.id))
            .selected_text(shown)
            .width(110.0)
            .show_ui(ui, |ui| {
                for option in options {
                    if ui.selectable_label(option == current, &option).clicked() {
                        self.touch.click();
                        self.edits.push(Edit::Value {
                            node: node.id,
                            key: "name".into(),
                            value: Some(Literal::Text(option)),
                        });
                    }
                }
            });
    }

    /// The palette: library nodes matching the search, by group.
    fn palette(&mut self, ui: &mut egui::Ui, pos: egui::Pos2) {
        ui.set_min_width(220.0);
        let search = ui.add(
            egui::TextEdit::singleline(self.search)
                .hint_text(theme::icon_text(icons::MAGNIFYING_GLASS, "Search nodes")),
        );
        search.request_focus();
        let needle = self.search.to_lowercase();
        let in_function = matches!(self.which, GraphRef::Function(_));
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                let mut category = "";
                for def in NODES {
                    let fits = match def.kind {
                        DefKind::FunctionEntry => in_function,
                        DefKind::Event { .. } => !in_function,
                        _ => true,
                    };
                    let text = format!("{} {}", def.title, def.category).to_lowercase();
                    if !fits || !text.contains(&needle) {
                        continue;
                    }
                    if def.category != category {
                        category = def.category;
                        ui.label(RichText::new(category).small().color(theme::TEXT_DIM));
                    }
                    if ui.button(def.title).clicked() {
                        self.touch.click();
                        self.edits.push(Edit::Add {
                            def: def.id,
                            pos: [pos.x, pos.y],
                        });
                        self.search.clear();
                        ui.close();
                    }
                }
            });
    }
}

impl Viewer<'_> {
    fn plain_title(&self, node: u32) -> String {
        let Some(n) = self.graph.node(node) else {
            return "?".into();
        };
        let Some(def) = node_def(&n.def) else {
            return n.def.clone();
        };
        match n.values.get("name") {
            Some(Literal::Text(name))
                if !name.is_empty() && !matches!(def.kind, DefKind::Action | DefKind::Pure) =>
            {
                format!("{}: {name}", def.title)
            }
            _ => def.title.to_owned(),
        }
    }

    /// A node's header: its group's colour, outlined when it has a problem or just ran.
    fn frame_of(&self, default: egui::Frame, node: NodeId, snarl: &Snarl<u32>) -> egui::Frame {
        let Some((n, def)) = self.node(snarl, node) else {
            return default;
        };
        let frame = default.fill(category_color(def.category));
        if self.focus == Some(n.id) {
            return frame.stroke(Stroke::new(3.0, theme::ACCENT));
        }
        if self.problems.contains_key(&n.id) {
            frame.stroke(Stroke::new(2.0, theme::DANGER))
        } else if let Some(age) = self.live.ran.get(&n.id) {
            // The path that ran glows, fading as other nodes run after it.
            let glow = theme::BRAND.gamma_multiply(1.0 - age * 0.8);
            frame.stroke(Stroke::new(3.0, glow))
        } else {
            frame
        }
    }
}

impl SnarlViewer<u32> for Viewer<'_> {
    fn title(&mut self, node: &u32) -> String {
        let title = self.plain_title(*node);
        if self.breakpoints.contains(node) {
            // A red dot marks where Play stops.
            format!("\u{25CF} {title}")
        } else {
            title
        }
    }

    fn header_frame(
        &mut self,
        default: egui::Frame,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        snarl: &Snarl<u32>,
    ) -> egui::Frame {
        self.frame_of(default, node, snarl)
    }

    fn inputs(&mut self, node: &u32) -> usize {
        self.graph
            .node(*node)
            .and_then(|n| {
                Some(
                    pins(self.blueprint, &self.which, n, node_def(&n.def)?)
                        .inputs
                        .len(),
                )
            })
            .unwrap_or(0)
    }

    fn outputs(&mut self, node: &u32) -> usize {
        self.graph
            .node(*node)
            .and_then(|n| {
                Some(
                    pins(self.blueprint, &self.which, n, node_def(&n.def)?)
                        .outputs
                        .len(),
                )
            })
            .unwrap_or(0)
    }

    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut egui::Ui,
        snarl: &mut Snarl<u32>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        let Some((n, def)) = self.node(snarl, pin.id.node) else {
            return Self::pin_info(PinType::Exec, false);
        };
        let ps = pins(self.blueprint, &self.which, n, def);
        let Some((name, ty)) = ps.inputs.get(pin.id.input).cloned() else {
            return Self::pin_info(PinType::Exec, false);
        };
        let wired = !pin.remotes.is_empty();
        if name != EXEC_IN {
            ui.label(RichText::new(&name).small());
        }
        if !wired && ty != PinType::Exec {
            let node = n.clone();
            self.value_field(ui, &node, &name, ty);
        }
        Self::pin_info(ty, wired)
    }

    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut egui::Ui,
        snarl: &mut Snarl<u32>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        let Some((n, def)) = self.node(snarl, pin.id.node) else {
            return Self::pin_info(PinType::Exec, false);
        };
        let ps = pins(self.blueprint, &self.which, n, def);
        let Some((name, ty)) = ps.outputs.get(pin.id.output).cloned() else {
            return Self::pin_info(PinType::Exec, false);
        };
        if name != "then" && name != "value" {
            ui.label(RichText::new(&name).small());
        }
        if let Some(value) = self.live.watches.get(&(n.id, name.clone())) {
            ui.label(
                RichText::new(format!("= {value}"))
                    .small()
                    .color(theme::BRAND),
            );
        }
        Self::pin_info(ty, !pin.remotes.is_empty())
    }

    fn has_body(&mut self, node: &u32) -> bool {
        let Some(def) = self.graph.node(*node).and_then(|n| node_def(&n.def)) else {
            return false;
        };
        matches!(
            def.kind,
            DefKind::GetVariable | DefKind::SetVariable | DefKind::Call | DefKind::Comment
        ) || def.id == "event.custom"
    }

    fn show_body(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        snarl: &mut Snarl<u32>,
    ) {
        let Some((n, def)) = self.node(snarl, node) else {
            return;
        };
        let n = n.clone();
        if def.kind == DefKind::Comment {
            let mut text = match n.values.get("text") {
                Some(Literal::Text(t)) => t.clone(),
                _ => String::new(),
            };
            let r = self.touch.track(
                ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .hint_text("A note")
                        .desired_width(180.0)
                        .desired_rows(3),
                ),
            );
            if r.changed() {
                self.edits.push(Edit::Value {
                    node: n.id,
                    key: "text".into(),
                    value: Some(Literal::Text(text)),
                });
            }
        } else {
            self.name_field(ui, &n, def);
        }
    }

    fn has_on_hover_popup(&mut self, node: &u32) -> bool {
        self.problems.contains_key(node)
    }

    fn show_on_hover_popup(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        snarl: &mut Snarl<u32>,
    ) {
        if let Some(problem) = snarl.get_node(node).and_then(|id| self.problems.get(id)) {
            ui.label(RichText::new(theme::icon_text(icons::WARNING, problem)).color(theme::DANGER));
        }
    }

    fn has_graph_menu(&mut self, _pos: egui::Pos2, _snarl: &mut Snarl<u32>) -> bool {
        true
    }

    fn show_graph_menu(&mut self, pos: egui::Pos2, ui: &mut egui::Ui, _snarl: &mut Snarl<u32>) {
        self.palette(ui, pos);
    }

    fn has_node_menu(&mut self, _node: &u32) -> bool {
        true
    }

    fn show_node_menu(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        snarl: &mut Snarl<u32>,
    ) {
        let id = snarl.get_node(node).copied();
        if let Some(id) = id {
            let on = self.breakpoints.contains(&id);
            let text = if on {
                "Remove breakpoint"
            } else {
                "Breakpoint: pause Play here"
            };
            if ui.button(theme::icon_text(icons::CIRCLE, text)).clicked() {
                self.edits.push(Edit::Breakpoint(id));
                ui.close();
            }
        }
        if let Some(id) = id {
            if ui
                .button(theme::icon_text(icons::MAGNIFYING_GLASS, "Find uses"))
                .on_hover_text("Every node like this one, in every blueprint")
                .clicked()
            {
                self.edits.push(Edit::FindUses(id));
                ui.close();
            }
            if ui
                .button(theme::icon_text(
                    icons::GIT_FORK,
                    "Collapse selected to function",
                ))
                .on_hover_text(
                    "The selected nodes (Shift+click or Shift+drag to select several; none: this one) become a function called here",
                )
                .clicked()
            {
                self.touch.click();
                self.edits.push(Edit::Collapse(id));
                ui.close();
            }
        }
        if ui
            .button(theme::icon_text(icons::TRASH, "Delete"))
            .clicked()
        {
            if let Some(&id) = snarl.get_node(node) {
                self.touch.click();
                self.edits.push(Edit::Remove(id));
            }
            ui.close();
        }
    }

    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<u32>) {
        let (Some((out, out_ty)), Some((input, in_ty))) = (
            self.graph_pin(snarl, from.id.node, from.id.output, false),
            self.graph_pin(snarl, to.id.node, to.id.input, true),
        ) else {
            return;
        };
        let exec = out_ty == PinType::Exec;
        if exec != (in_ty == PinType::Exec)
            || (!exec && !out_ty.fits(in_ty))
            || out.node == input.node
        {
            return;
        }
        self.touch.click();
        // An input takes one wire, an exec output leads to one node.
        for wire in &self.graph.wires {
            let into_input = wire.to == input;
            let from_exec = exec && wire.from == out;
            if into_input || from_exec {
                self.edits.push(Edit::Disconnect(wire.clone()));
            }
        }
        let reroute = (self.graph.node(input.node))
            .and_then(|n| node_def(&n.def))
            .is_some_and(|d| d.kind == DefKind::Reroute);
        if reroute {
            self.edits.push(Edit::Value {
                node: input.node,
                key: "type".into(),
                value: Some(Literal::Text(out_ty.name().to_owned())),
            });
        }
        self.edits.push(Edit::Connect(Wire {
            from: out,
            to: input,
        }));
    }

    fn disconnect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<u32>) {
        if let (Some((out, _)), Some((input, _))) = (
            self.graph_pin(snarl, from.id.node, from.id.output, false),
            self.graph_pin(snarl, to.id.node, to.id.input, true),
        ) {
            self.touch.click();
            self.edits.push(Edit::Disconnect(Wire {
                from: out,
                to: input,
            }));
        }
    }

    fn drop_outputs(&mut self, pin: &OutPin, snarl: &mut Snarl<u32>) {
        if let Some((out, _)) = self.graph_pin(snarl, pin.id.node, pin.id.output, false) {
            self.touch.click();
            for wire in self.graph.wires.iter().filter(|w| w.from == out) {
                self.edits.push(Edit::Disconnect(wire.clone()));
            }
        }
    }

    fn drop_inputs(&mut self, pin: &InPin, snarl: &mut Snarl<u32>) {
        if let Some((input, _)) = self.graph_pin(snarl, pin.id.node, pin.id.input, true) {
            self.touch.click();
            for wire in self.graph.wires.iter().filter(|w| w.to == input) {
                self.edits.push(Edit::Disconnect(wire.clone()));
            }
        }
    }
}

impl Studio {
    /// Opens blueprint `name` of the project.
    pub(crate) fn open_blueprint(&mut self, name: &str) {
        let Some(project) = &self.project else { return };
        let file = blueprint_file(&project.scripts_dir(), name);
        let blueprint = if file.is_file() {
            match blueprint::load_blueprint(&file) {
                Ok(b) => b,
                Err(e) => {
                    self.error = Some(format!("Can't open blueprint {name}: {e}"));
                    return;
                }
            }
        } else {
            Blueprint::default()
        };
        let mut editor = BlueprintEditor::new(name.to_owned(), blueprint);
        editor.problems = blueprint::compile(name, editor.doc.value())
            .err()
            .unwrap_or_default();
        self.blueprint_editor = Some(editor);
    }

    pub(crate) fn blueprint_panel(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            theme::empty_state(
                ui,
                icons::GIT_FORK,
                "No project",
                "Open a project to make its blueprints.",
            );
            return;
        };
        let scripts = project.scripts_dir();
        let names = blueprint::blueprint_names(&scripts);
        if self.blueprint_editor.is_none()
            && let Some(first) = names.first().cloned()
        {
            self.open_blueprint(&first);
        }
        let mut open = None;
        let mut make = false;
        ui.horizontal(|ui| {
            let current = self
                .blueprint_editor
                .as_ref()
                .map_or("none".to_owned(), |e| e.name.clone());
            egui::ComboBox::from_id_salt("bp file")
                .selected_text(theme::icon_text(icons::GIT_FORK, &current))
                .width(170.0)
                .show_ui(ui, |ui| {
                    for name in &names {
                        if ui.selectable_label(*name == current, name).clicked() {
                            open = Some(name.clone());
                        }
                    }
                });
            if ui
                .button(theme::icon_text(icons::PLUS, "Blueprint"))
                .on_hover_text("A new blueprint")
                .clicked()
            {
                make = true;
            }
        });
        if make {
            let name = (1..)
                .map(|i| format!("blueprint{i}"))
                .find(|n| !names.contains(n))
                .unwrap_or_default();
            let blueprint = Blueprint::default();
            if let Err(e) = blueprint::save_blueprint(&blueprint, &blueprint_file(&scripts, &name))
            {
                self.error = Some(format!("Can't make blueprint {name}: {e}"));
            }
            open = Some(name);
        }
        if let Some(name) = open {
            self.open_blueprint(&name);
        }
        let Some(mut editor) = self.blueprint_editor.take() else {
            theme::empty_state(
                ui,
                icons::GIT_FORK,
                "No blueprints yet",
                "Make one: game rules and behaviours as node graphs, no code.",
            );
            return;
        };
        let live = self.follow_play(&editor);
        let mut blueprint = editor.doc.value().clone();
        let mut touch = Touch::default();
        let mut label = "Edit the blueprint";
        egui::Panel::left("bp side")
            .resizable(true)
            .default_size(230.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        side_panel(ui, &mut editor, &mut blueprint, &mut touch, &mut label);
                    });
            });
        let mut follow = None;
        egui::Panel::bottom("bp problems")
            .resizable(true)
            .default_size(80.0)
            .show(ui, |ui| {
                if let Some(at) = references_list(ui, &editor) {
                    follow = Some(at);
                }
                problems_list(ui, &editor);
            });
        egui::CentralPanel::default().show(ui, |ui| {
            canvas(
                ui,
                &mut editor,
                &mut blueprint,
                &mut touch,
                &mut label,
                live,
                &scripts,
            );
        });
        self.blueprint_editor = Some(editor);
        self.apply_blueprint_edit(blueprint, &touch, label);
        if let Some(at) = follow {
            self.go_to_node(at);
        }
    }

    /// Opens the blueprint and graph of `at` with the node pointed out.
    fn go_to_node(&mut self, at: blueprint::NodeAt) {
        let open = self.blueprint_editor.as_ref().map(|e| e.name.clone());
        let references = self
            .blueprint_editor
            .as_mut()
            .and_then(|e| e.references.take());
        if open.as_deref() != Some(at.blueprint.as_str()) {
            self.open_blueprint(&at.blueprint);
        }
        if let Some(editor) = &mut self.blueprint_editor {
            editor.references = references;
            editor.graph = at.graph;
            editor.focus = Some(at.node);
            editor.built = None;
        }
    }

    /// What Play reports about the open blueprint (the path that ran, watched values), with
    /// its breakpoints handed to Play; a breakpoint reached pauses Play.
    fn follow_play(&mut self, editor: &BlueprintEditor) -> Live {
        let Some(log) = self.playing.as_ref().and_then(|p| p.script_debug()) else {
            return Live::default();
        };
        let name = &editor.name;
        let mut log = log.borrow_mut();
        log.breakpoints.retain(|(script, _)| script != name);
        (log.breakpoints).extend(editor.breakpoints.iter().map(|&n| (name.clone(), n)));
        let now = log.now();
        let mut live = Live::default();
        for ((script, node), at) in &log.ran {
            let age = (now - at) as f32 / LIT_TRACES;
            if script == name && age < 1.0 {
                live.ran.insert(*node, age);
            }
        }
        for ((script, node, pin), value) in &log.watches {
            if script == name {
                live.watches.insert((*node, pin.clone()), value.clone());
            }
        }
        let hit = log.hit.take();
        drop(log);
        if let Some((_, node)) = hit
            && let Some(playing) = &mut self.playing
        {
            playing.paused = true;
            self.notify(format!(
                "Breakpoint: blueprint {name}, node {node}; Play paused"
            ));
        }
        live
    }

    /// Records `blueprint` (one step per drag or field visit), saves and compiles it.
    fn apply_blueprint_edit(&mut self, blueprint: Blueprint, touch: &Touch, label: &str) {
        let Some(editor) = &mut self.blueprint_editor else {
            return;
        };
        if touch.started {
            editor.doc.begin_group(label);
            self.focus = Focus::Blueprint;
        }
        let changed = touch.changed && editor.doc.set(blueprint, label).changed;
        if touch.finished {
            editor.doc.end_group();
        }
        if changed {
            editor.revision += 1;
            self.save_blueprint();
        }
    }

    /// Writes the blueprint and its script (none while it has problems).
    fn save_blueprint(&mut self) {
        let (Some(editor), Some(project)) = (&mut self.blueprint_editor, &self.project) else {
            return;
        };
        let scripts = project.scripts_dir();
        let saved =
            blueprint::save_blueprint(editor.doc.value(), &blueprint_file(&scripts, &editor.name))
                .and_then(|()| blueprint::build_blueprint(&scripts, &editor.name));
        match saved {
            Ok(problems) => editor.problems = problems,
            Err(e) => self.error = Some(format!("Can't save blueprint {}: {e}", editor.name)),
        }
    }

    pub(crate) fn blueprint_step(&mut self, redo: bool) -> Option<String> {
        let editor = self.blueprint_editor.as_mut()?;
        let label = if redo {
            editor.doc.redo()
        } else {
            editor.doc.undo()
        };
        if label.is_some() {
            editor.revision += 1;
            self.save_blueprint();
        }
        label
    }

    pub(crate) fn blueprint_history(&self) -> Option<(String, Vec<c3_edit::HistoryEntry>)> {
        let editor = self.blueprint_editor.as_ref()?;
        Some((format!("Blueprint {}", editor.name), editor.doc.history()))
    }

    pub(crate) fn blueprint_labels(&self) -> (Option<String>, Option<String>) {
        let doc = self.blueprint_editor.as_ref().map(|e| &e.doc);
        (
            doc.and_then(|d| d.undo_label()).map(str::to_owned),
            doc.and_then(|d| d.redo_label()).map(str::to_owned),
        )
    }
}

fn blueprint_file(scripts: &std::path::Path, name: &str) -> std::path::PathBuf {
    scripts
        .join(blueprint::BLUEPRINT_DIR)
        .join(format!("{name}{}", blueprint::BLUEPRINT_SUFFIX))
}

/// The graph picker, variables and functions.
fn side_panel(
    ui: &mut egui::Ui,
    editor: &mut BlueprintEditor,
    blueprint: &mut Blueprint,
    touch: &mut Touch,
    label: &mut &'static str,
) {
    theme::section(ui, &theme::icon_text(icons::GIT_FORK, "Graphs"), |ui| {
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
            if ui
                .selectable_label(editor.graph == GraphRef::Events, "Event graph")
                .clicked()
            {
                editor.graph = GraphRef::Events;
                editor.built = None;
            }
            for f in &blueprint.functions {
                let which = GraphRef::Function(f.name.clone());
                if ui
                    .selectable_label(editor.graph == which, format!("ƒ  {}", f.name))
                    .clicked()
                {
                    editor.graph = which;
                    editor.built = None;
                }
            }
        });
    });
    theme::section(ui, "Variables", |ui| {
        let mut remove = None;
        for (i, v) in blueprint.variables.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                touch.track(ui.add(egui::TextEdit::singleline(&mut v.name).desired_width(80.0)));
                if type_pick(ui, ("var type", i), &mut v.ty) {
                    touch.click();
                }
                let scope = match v.scope {
                    VariableScope::Local => "local",
                    VariableScope::Player => "player",
                    VariableScope::World => "world",
                };
                egui::ComboBox::from_id_salt(("var scope", i))
                    .selected_text(scope)
                    .width(60.0)
                    .show_ui(ui, |ui| {
                        for (s, name) in [
                            (VariableScope::Local, "local"),
                            (VariableScope::Player, "player"),
                            (VariableScope::World, "world"),
                        ] {
                            if ui.selectable_label(v.scope == s, name).clicked() && v.scope != s {
                                v.scope = s;
                                touch.click();
                            }
                        }
                    });
                if ui.small_button(icons::X).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            blueprint.variables.remove(i);
            touch.click();
            *label = "Remove a variable";
        }
        if ui
            .button(theme::icon_text(icons::PLUS, "Variable"))
            .clicked()
        {
            let name = (1..)
                .map(|i| format!("var{i}"))
                .find(|n| blueprint.variables.iter().all(|v| &v.name != n))
                .unwrap_or_default();
            blueprint.variables.push(Variable {
                name,
                ty: PinType::Number,
                scope: VariableScope::Local,
                default: Literal::Number(0.0),
            });
            touch.click();
            *label = "Add a variable";
        }
    });
    theme::section(ui, "Functions", |ui| {
        let mut remove = None;
        for (i, f) in blueprint.functions.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let before = f.name.clone();
                let r = touch
                    .track(ui.add(egui::TextEdit::singleline(&mut f.name).desired_width(110.0)));
                if r.changed() && editor.graph == GraphRef::Function(before) {
                    editor.graph = GraphRef::Function(f.name.clone());
                }
                if ui.small_button(icons::X).clicked() {
                    remove = Some(i);
                }
            });
            ui.indent(("fn pins", i), |ui| {
                let mut drop = None;
                for (j, (name, ty)) in f.inputs.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        touch.track(ui.add(egui::TextEdit::singleline(name).desired_width(70.0)));
                        if type_pick(ui, ("fn in", i, j), ty) {
                            touch.click();
                        }
                        if ui.small_button(icons::X).clicked() {
                            drop = Some(j);
                        }
                    });
                }
                if let Some(j) = drop {
                    f.inputs.remove(j);
                    touch.click();
                }
                ui.horizontal(|ui| {
                    if ui
                        .small_button(theme::icon_text(icons::PLUS, "input"))
                        .clicked()
                    {
                        f.inputs
                            .push((format!("in{}", f.inputs.len() + 1), PinType::Number));
                        touch.click();
                    }
                    let mut answers = f.output.is_some();
                    if ui.checkbox(&mut answers, "answers").changed() {
                        f.output = answers.then_some(PinType::Number);
                        touch.click();
                    }
                    if let Some(ty) = &mut f.output
                        && type_pick(ui, ("fn out", i), ty)
                    {
                        touch.click();
                    }
                });
            });
        }
        if let Some(i) = remove {
            let gone = blueprint.functions.remove(i);
            if editor.graph == GraphRef::Function(gone.name) {
                editor.graph = GraphRef::Events;
                editor.built = None;
            }
            touch.click();
            *label = "Remove a function";
        }
        if ui
            .button(theme::icon_text(icons::PLUS, "Function"))
            .clicked()
        {
            let name = (1..)
                .map(|i| format!("function{i}"))
                .find(|n| blueprint.functions.iter().all(|f| &f.name != n))
                .unwrap_or_default();
            let entry = GraphNode {
                id: 1,
                def: "flow.entry".into(),
                pos: [0.0, 0.0],
                values: Default::default(),
            };
            blueprint.functions.push(Function {
                name: name.clone(),
                inputs: Vec::new(),
                output: None,
                graph: Graph {
                    nodes: vec![entry],
                    wires: Vec::new(),
                },
            });
            editor.graph = GraphRef::Function(name);
            editor.built = None;
            touch.click();
            *label = "Add a function";
        }
    });
}

/// A pin type picker; returns whether it changed.
fn type_pick(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    ty: &mut PinType,
) -> bool {
    const TYPES: [PinType; 8] = [
        PinType::Number,
        PinType::Text,
        PinType::Bool,
        PinType::Entity,
        PinType::Item,
        PinType::Map,
        PinType::List,
        PinType::Record,
    ];
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(RichText::new(ty.name()).color(pin_color(*ty)))
        .width(64.0)
        .show_ui(ui, |ui| {
            for t in TYPES {
                if ui
                    .selectable_label(*ty == t, RichText::new(t.name()).color(pin_color(t)))
                    .clicked()
                    && *ty != t
                {
                    *ty = t;
                    changed = true;
                }
            }
        });
    changed
}

/// The last "Find uses" results; returns the one clicked.
fn references_list(ui: &mut egui::Ui, editor: &BlueprintEditor) -> Option<blueprint::NodeAt> {
    let (what, found) = editor.references.as_ref()?;
    let mut picked = None;
    ui.label(RichText::new(format!("Uses of {what}: {}", found.len())).strong());
    ui.horizontal_wrapped(|ui| {
        for at in found {
            let graph = match &at.graph {
                GraphRef::Events => "events".to_owned(),
                GraphRef::Function(f) => f.clone(),
            };
            if ui
                .link(format!("{} › {graph} › {}", at.blueprint, at.node))
                .clicked()
            {
                picked = Some(at.clone());
            }
        }
    });
    picked
}

fn problems_list(ui: &mut egui::Ui, editor: &BlueprintEditor) {
    if editor.problems.is_empty() {
        ui.label(
            RichText::new(theme::icon_text(
                icons::CHECK_CIRCLE,
                "Compiles: the game runs it",
            ))
            .color(theme::SUCCESS),
        );
        return;
    }
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            for p in &editor.problems {
                let place = match (&p.graph, p.node) {
                    (GraphRef::Events, Some(n)) => format!("Event graph, node {n}: "),
                    (GraphRef::Function(f), Some(n)) => format!("{f}, node {n}: "),
                    (GraphRef::Events, None) => "Event graph: ".into(),
                    (GraphRef::Function(f), None) => format!("{f}: "),
                };
                ui.label(
                    RichText::new(theme::icon_text(
                        icons::WARNING,
                        &format!("{place}{}", p.message),
                    ))
                    .color(theme::DANGER),
                );
            }
        });
}

/// The graph on the canvas; what the user does there changes `blueprint`.
fn canvas(
    ui: &mut egui::Ui,
    editor: &mut BlueprintEditor,
    blueprint: &mut Blueprint,
    touch: &mut Touch,
    label: &mut &'static str,
    live: Live,
    scripts: &std::path::Path,
) {
    if editor.built != Some(editor.revision) {
        editor.rebuild();
    }
    let problems = (editor.problems.iter())
        .filter(|p| p.graph == editor.graph)
        .filter_map(|p| Some((p.node?, p.message.clone())))
        .collect();
    let which = editor.graph.clone();
    let source = editor.doc.value().clone();
    let Some(graph) = BlueprintEditor::graph_of(&source, &which) else {
        return;
    };
    let mut viewer = Viewer {
        blueprint: &source,
        which: which.clone(),
        graph,
        problems,
        edits: Vec::new(),
        touch,
        search: &mut editor.search,
        live,
        breakpoints: &editor.breakpoints,
        focus: editor.focus,
    };
    // Near 1:1 even when a graph has few nodes (fitting them would zoom far in).
    let style = SnarlStyle {
        min_scale: Some(0.2),
        max_scale: Some(1.25),
        ..SnarlStyle::new()
    };
    let widget = SnarlWidget::new()
        .id_salt(("bp canvas", &editor.name, format!("{which:?}")))
        .style(style);
    widget.show(&mut editor.snarl, &mut viewer, ui);
    let edits = std::mem::take(&mut viewer.edits);
    let selected: Vec<u32> = (widget.get_selected_nodes(ui).into_iter())
        .filter_map(|id| editor.snarl.get_node(id).copied())
        .collect();
    let Some(graph) = BlueprintEditor::graph_mut(blueprint, &which) else {
        return;
    };
    // Moved nodes: one step for the whole drag.
    let mut moved = false;
    for (_, pos, &id) in editor.snarl.nodes_pos_ids() {
        if let Some(n) = graph.nodes.iter_mut().find(|n| n.id == id)
            && ((n.pos[0] - pos.x).abs() > 0.5 || (n.pos[1] - pos.y).abs() > 0.5)
        {
            n.pos = [pos.x, pos.y];
            moved = true;
        }
    }
    let dragging = ui.input(|i| i.pointer.primary_down());
    if moved {
        touch.changed = true;
        *label = "Move nodes";
        if !editor.moving {
            editor.moving = true;
            touch.started = true;
        }
    }
    if editor.moving && !dragging {
        editor.moving = false;
        touch.finished = true;
    }
    for edit in edits {
        match edit {
            Edit::Add { def, pos } => {
                let id = graph.next_id();
                graph.nodes.push(GraphNode {
                    id,
                    def: def.to_owned(),
                    pos,
                    values: Default::default(),
                });
                *label = "Add a node";
            }
            Edit::Remove(id) => {
                graph.nodes.retain(|n| n.id != id);
                graph.wires.retain(|w| w.from.node != id && w.to.node != id);
                *label = "Delete a node";
            }
            Edit::Connect(wire) => {
                if !graph.wires.contains(&wire) {
                    graph.wires.push(wire);
                }
                *label = "Wire pins";
            }
            Edit::Disconnect(wire) => {
                graph.wires.retain(|w| w != &wire);
                *label = "Unwire pins";
            }
            Edit::Value { node, key, value } => {
                if let Some(n) = graph.nodes.iter_mut().find(|n| n.id == node) {
                    match value {
                        Some(v) => n.values.insert(key, v),
                        None => n.values.remove(&key),
                    };
                }
                *label = "Set a value";
            }
            Edit::Collapse(menu_node) => {
                let chosen = if selected.is_empty() {
                    vec![menu_node]
                } else {
                    selected.clone()
                };
                let name = (1..)
                    .map(|i| format!("function{i}"))
                    .find(|n| source.functions.iter().all(|f| &f.name != n))
                    .unwrap_or_default();
                match blueprint::collapse(blueprint, &which, &chosen, &name) {
                    Ok(()) => {
                        *label = "Collapse to a function";
                        touch.changed = true;
                    }
                    Err(why) => editor.problems.push(Problem {
                        graph: which.clone(),
                        node: None,
                        message: format!("Can't collapse: {why}"),
                    }),
                }
                return;
            }
            Edit::FindUses(node) => {
                let Some(n) = graph.nodes.iter().find(|n| n.id == node) else {
                    continue;
                };
                let name = match n.values.get("name") {
                    Some(Literal::Text(t)) => Some(t.clone()),
                    _ => None,
                };
                let title = node_def(&n.def).map_or(n.def.clone(), |d| d.title.to_owned());
                let title = name
                    .as_ref()
                    .map_or(title.clone(), |nm| format!("{title}: {nm}"));
                let found = blueprint::references(scripts, &n.def, name.as_deref());
                editor.references = Some((title, found));
                continue;
            }
            Edit::Breakpoint(node) => {
                if !editor.breakpoints.remove(&node) {
                    editor.breakpoints.insert(node);
                }
                continue;
            }
        }
        touch.changed = true;
    }
}
