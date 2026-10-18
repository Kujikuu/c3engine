//! The dialogue graph canvas: nodes as cards, links as curves. Drag a card to move it, drag
//! empty space to pan, scroll to zoom. Drag from an answer's dot onto a card to link them,
//! or onto empty space to make the next node there. Right-click for more.

use crate::icons;
use crate::theme;
use c3_script::{Action, Condition, DialogueGraph, GraphProblem, Node, NodeKind, Quest};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use egui::epaint::CubicBezierShape;

/// Card width and row heights, in graph units.
const WIDTH: f32 = 240.0;
const HEADER: f32 = 26.0;
const TEXT_ROW: f32 = 17.0;
const PORT_ROW: f32 = 22.0;
const PAD: f32 = 6.0;
/// Lines of what the NPC says shown on a card.
const SHOWN_LINES: usize = 3;
const MIN_ZOOM: f32 = 0.3;
/// Fitting never zooms out further than this (cards stay readable); what doesn't fit is
/// panned to.
const MIN_FIT_ZOOM: f32 = 0.7;
const MAX_ZOOM: f32 = 2.0;

const SAY: Color32 = Color32::from_rgb(70, 110, 160);
const BRANCH: Color32 = Color32::from_rgb(150, 100, 170);
const CHOICE_LINK: Color32 = Color32::from_rgb(222, 178, 92);
const CASE_LINK: Color32 = Color32::from_rgb(170, 130, 210);
const OTHERWISE_LINK: Color32 = Color32::from_gray(150);
const CARD: Color32 = Color32::from_rgb(34, 36, 42);

/// How the canvas is looked at and what is going on in it.
pub struct CanvasState {
    /// Screen offset of graph point (0, 0) from the canvas corner.
    pub offset: Vec2,
    pub zoom: f32,
    pub selected: Option<u32>,
    /// Fit every node in view on the next frame.
    pub fit: bool,
    /// An answer being linked: (node, port).
    linking: Option<(u32, usize)>,
    /// Graph point the background menu was opened at.
    menu_at: Option<[f32; 2]>,
    /// The canvas size last drawn.
    size: Vec2,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            offset: vec2(20.0, 20.0),
            zoom: 1.0,
            selected: None,
            fit: true,
            linking: None,
            menu_at: None,
            size: vec2(800.0, 500.0),
        }
    }
}

/// An outgoing link of a card: its label and where it leads.
struct Port {
    label: String,
    color: Color32,
    next: Option<u32>,
}

/// A card's contents, laid out in graph units.
struct Card {
    lines: Vec<String>,
    ports: Vec<Port>,
}

impl Card {
    fn of(node: &Node, quests: &[Quest]) -> Self {
        match &node.kind {
            NodeKind::Say {
                actions,
                lines,
                choices,
            } => {
                let mut shown: Vec<String> = actions
                    .iter()
                    .map(|a| format!("{}  {}", icons::SPARKLE, action_text(a, quests)))
                    .collect();
                shown.extend(
                    lines
                        .iter()
                        .filter(|l| !l.trim().is_empty())
                        .take(SHOWN_LINES)
                        .map(|l| format!("\u{201C}{}\u{201D}", l.trim())),
                );
                let hidden = lines.len().saturating_sub(SHOWN_LINES);
                if hidden > 0 {
                    shown.push(format!("… {hidden} more"));
                }
                let ports = choices
                    .iter()
                    .map(|c| Port {
                        label: match &c.condition {
                            Some(cond) => {
                                format!("{}  [{}]", c.label, condition_text(cond, quests))
                            }
                            None => c.label.clone(),
                        },
                        color: CHOICE_LINK,
                        next: c.next,
                    })
                    .collect();
                Card {
                    lines: shown,
                    ports,
                }
            }
            NodeKind::Branch { cases, otherwise } => {
                let mut ports: Vec<Port> = cases
                    .iter()
                    .map(|c| Port {
                        label: format!("If {}", condition_text(&c.condition, quests)),
                        color: CASE_LINK,
                        next: c.next,
                    })
                    .collect();
                ports.push(Port {
                    label: "Otherwise".into(),
                    color: OTHERWISE_LINK,
                    next: *otherwise,
                });
                Card {
                    lines: Vec::new(),
                    ports,
                }
            }
        }
    }

    fn height(&self) -> f32 {
        HEADER + PAD * 2.0 + self.lines.len() as f32 * TEXT_ROW + self.ports.len() as f32 * PORT_ROW
    }

    /// Top of port `i`'s row, below the card's top.
    fn port_top(&self, i: usize) -> f32 {
        HEADER + PAD + self.lines.len() as f32 * TEXT_ROW + i as f32 * PORT_ROW
    }
}

fn quest_name(id: i64, quests: &[Quest]) -> String {
    quests.iter().find(|q| q.id == id).map_or_else(
        || format!("quest {id}"),
        |q| format!("\u{201C}{}\u{201D}", q.name),
    )
}

/// A condition in words.
pub fn condition_text(c: &Condition, quests: &[Quest]) -> String {
    match c {
        Condition::Level(n) => format!("level ≥ {n}"),
        Condition::Money(n) => format!("money ≥ {n}"),
        Condition::Item { item, count } => format!("carries {count} × item {item}"),
        Condition::Flag { name, value } => format!("{name} = {value}"),
        Condition::QuestNotTaken(q) => format!("{} not taken", quest_name(*q, quests)),
        Condition::QuestTaken(q) => format!("{} taken", quest_name(*q, quests)),
        Condition::QuestReady(q) => format!("{} ready", quest_name(*q, quests)),
        Condition::QuestDone(q) => format!("{} done", quest_name(*q, quests)),
    }
}

/// An action in words.
pub fn action_text(a: &Action, quests: &[Quest]) -> String {
    match a {
        Action::GiveMoney(n) => format!("Give {n} money"),
        Action::TakeMoney(n) => format!("Take {n} money"),
        Action::GiveItem { item, count } => format!("Give {count} × item {item}"),
        Action::TakeItem { item, count } => format!("Take {count} × item {item}"),
        Action::SetFlag { name, value } => format!("Set {name} = {value}"),
        Action::Message(text) => format!("Message: {text}"),
        Action::Teleport { map, x, y } => format!("Teleport to {map} ({x}, {y})"),
        Action::OpenShop => "Open the shop".into(),
        Action::StartQuest(q) => format!("Start {}", quest_name(*q, quests)),
        Action::CompleteQuest(q) => format!("Complete {}", quest_name(*q, quests)),
    }
}

/// `text` cut to `max` characters.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        let mut s: String = text.chars().take(max.saturating_sub(1)).collect();
        s.push('…');
        s
    }
}

/// A node's name for lists: `#3 Offer` or its first line.
pub fn node_label(node: &Node) -> String {
    let name = if !node.title.is_empty() {
        node.title.clone()
    } else {
        match &node.kind {
            NodeKind::Say { lines, .. } => lines.first().cloned().unwrap_or_default(),
            NodeKind::Branch { .. } => "Branch".into(),
        }
    };
    clip(&format!("#{}  {}", node.id, name), 40)
}

/// What the canvas asks of the panel.
#[derive(Default)]
pub struct CanvasOutput {
    /// A short message for the status bar.
    pub notice: Option<String>,
}

impl CanvasState {
    fn to_screen(&self, origin: Pos2, p: [f32; 2]) -> Pos2 {
        origin + self.offset + Vec2::from(p) * self.zoom
    }

    fn to_graph(&self, origin: Pos2, p: Pos2) -> [f32; 2] {
        ((p - origin - self.offset) / self.zoom).into()
    }

    /// Fits every node of `graph` in a canvas of `size`.
    fn fit_to(&mut self, graph: &DialogueGraph, quests: &[Quest], size: Vec2) {
        let mut bounds = Rect::NOTHING;
        for n in &graph.nodes {
            let card = Card::of(n, quests);
            bounds = bounds.union(Rect::from_min_size(
                Pos2::from(n.pos),
                vec2(WIDTH, card.height()),
            ));
        }
        if !bounds.is_positive() {
            *self = Self {
                fit: false,
                ..Self::default()
            };
            return;
        }
        let margin = 30.0;
        let zoom = ((size.x - margin * 2.0) / bounds.width())
            .min((size.y - margin * 2.0) / bounds.height())
            .clamp(MIN_FIT_ZOOM, 1.0);
        self.zoom = zoom;
        // Centred along each axis that fits, else from its start.
        let room = size - bounds.size() * zoom;
        let place = |room: f32| if room > 0.0 { room / 2.0 } else { margin };
        self.offset = vec2(place(room.x), place(room.y)) - bounds.min.to_vec2() * zoom;
    }

    /// Puts node `id` in the middle of the canvas and selects it.
    pub fn focus(&mut self, graph: &DialogueGraph, id: u32) {
        if let Some(n) = graph.node(id) {
            self.offset =
                self.size / 2.0 - (Vec2::from(n.pos) + vec2(WIDTH / 2.0, HEADER)) * self.zoom;
            self.selected = Some(id);
        }
    }
}

/// Draws `graph` and handles editing it.
pub fn canvas(
    ui: &mut egui::Ui,
    graph: &mut DialogueGraph,
    state: &mut CanvasState,
    problems: &[GraphProblem],
    quests: &[Quest],
) -> CanvasOutput {
    let mut out = CanvasOutput::default();
    let (rect, background) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let origin = rect.min;
    if state.fit {
        state.fit_to(graph, quests, rect.size());
        // Panels settle over the first frames: fit again until the size holds.
        state.fit = rect.size() != state.size;
    }
    state.size = rect.size();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_gray(22));
    grid(&painter, rect, state);

    // Pan and zoom.
    if background.dragged() {
        state.offset += background.drag_delta();
    }
    if let Some(pointer) = ui.ctx().pointer_hover_pos().filter(|p| rect.contains(*p)) {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let before = state.to_graph(origin, pointer);
            state.zoom = (state.zoom * (scroll * 0.002).exp()).clamp(MIN_ZOOM, MAX_ZOOM);
            state.offset = pointer - origin - Vec2::from(before) * state.zoom;
        }
    }
    if background.clicked() {
        state.selected = None;
    }
    if background.secondary_clicked() {
        state.menu_at = background
            .interact_pointer_pos()
            .map(|p| state.to_graph(origin, p));
    }
    background.context_menu(|ui| {
        let at = state.menu_at.unwrap_or([0.0, 0.0]);
        if ui
            .button(theme::icon_text(icons::CHAT_TEARDROP_TEXT, "Add Say node"))
            .clicked()
        {
            state.selected = Some(graph.add(NodeKind::say("…"), at));
            ui.close();
        }
        if ui
            .button(theme::icon_text(icons::GIT_FORK, "Add Branch node"))
            .clicked()
        {
            let branch = NodeKind::Branch {
                cases: Vec::new(),
                otherwise: None,
            };
            state.selected = Some(graph.add(branch, at));
            ui.close();
        }
        ui.separator();
        if ui
            .button(theme::icon_text(icons::CORNERS_OUT, "Fit all in view"))
            .clicked()
        {
            state.fit = true;
            ui.close();
        }
    });

    let cards: Vec<(u32, Card)> = graph
        .nodes
        .iter()
        .map(|n| (n.id, Card::of(n, quests)))
        .collect();
    let card_rect = |state: &CanvasState, n: &Node, card: &Card| {
        Rect::from_min_size(
            state.to_screen(origin, n.pos),
            vec2(WIDTH, card.height()) * state.zoom,
        )
    };
    let input_at = |state: &CanvasState, n: &Node| {
        state.to_screen(origin, [n.pos[0], n.pos[1] + HEADER / 2.0])
    };
    let port_at = |state: &CanvasState, n: &Node, card: &Card, i: usize| {
        state.to_screen(
            origin,
            [
                n.pos[0] + WIDTH,
                n.pos[1] + card.port_top(i) + PORT_ROW / 2.0,
            ],
        )
    };

    // Links under the cards.
    for (n, (_, card)) in graph.nodes.iter().zip(&cards) {
        for (i, port) in card.ports.iter().enumerate() {
            let Some(target) = port.next.and_then(|t| graph.node(t)) else {
                continue;
            };
            let from = port_at(state, n, card, i);
            link(
                &painter,
                from,
                input_at(state, target),
                port.color,
                state.zoom,
            );
        }
    }

    // Cards, then their ports (on top, so they take the pointer first).
    let id = egui::Id::unique("dialogue canvas");
    let mut drop_target = None;
    let pointer = ui.ctx().pointer_latest_pos();
    let mut remove = None;
    let mut disconnect = None;
    let mut make_start = None;
    let mut duplicate = None;
    for (index, (node_id, card)) in cards.iter().enumerate() {
        let n = &graph.nodes[index];
        let r = card_rect(state, n, card);
        if !r.intersects(rect) {
            continue;
        }
        let resp = ui.interact(r, id.with(("card", node_id)), Sense::click_and_drag());
        if pointer.is_some_and(|p| r.contains(p)) {
            drop_target = Some(*node_id);
        }
        let selected = state.selected == Some(*node_id);
        let problem = problems.iter().find(|p| p.node == Some(*node_id));
        draw_card(
            &painter,
            r,
            n,
            card,
            state.zoom,
            selected,
            *node_id == graph.start,
            problem,
        );
        if resp.clicked() || resp.drag_started() {
            state.selected = Some(*node_id);
        }
        if let Some(p) = problem {
            resp.clone().on_hover_text(
                egui::RichText::new(theme::icon_text(icons::WARNING, &p.message))
                    .color(theme::WARNING),
            );
        }
        if resp.dragged() {
            let delta = resp.drag_delta() / state.zoom;
            let node = &mut graph.nodes[index];
            node.pos = [node.pos[0] + delta.x, node.pos[1] + delta.y];
        }
        resp.context_menu(|ui| {
            if ui
                .add_enabled(
                    *node_id != graph.start,
                    egui::Button::new(theme::icon_text(icons::FLAG, "Start here")),
                )
                .on_disabled_hover_text("The dialogue already starts here")
                .clicked()
            {
                make_start = Some(*node_id);
                ui.close();
            }
            if ui
                .button(theme::icon_text(icons::COPY, "Duplicate"))
                .clicked()
            {
                duplicate = Some(*node_id);
                ui.close();
            }
            if ui
                .add_enabled(
                    *node_id != graph.start,
                    egui::Button::new(theme::icon_text(icons::TRASH, "Delete")),
                )
                .on_disabled_hover_text("Make another node the start first")
                .clicked()
            {
                remove = Some(*node_id);
                ui.close();
            }
        });
        let n = &graph.nodes[index];
        for (i, port) in card.ports.iter().enumerate() {
            let at = port_at(state, n, card, i);
            let radius = 5.0 * state.zoom.max(0.6);
            let hit = Rect::from_center_size(at, Vec2::splat(radius * 3.0));
            let resp = ui.interact(hit, id.with(("port", node_id, i)), Sense::click_and_drag());
            let hot = resp.hovered() || resp.dragged();
            painter.circle_filled(
                at,
                radius,
                if port.next.is_some() || hot {
                    port.color
                } else {
                    CARD
                },
            );
            painter.circle_stroke(at, radius, Stroke::new(1.5, port.color));
            if resp.drag_started() {
                state.linking = Some((*node_id, i));
            }
            let resp =
                resp.on_hover_text("Drag onto a node to link it; drop on empty space to make one");
            resp.context_menu(|ui| {
                if ui
                    .add_enabled(
                        port.next.is_some(),
                        egui::Button::new(theme::icon_text(icons::LINK_BREAK, "Disconnect")),
                    )
                    .clicked()
                {
                    disconnect = Some((*node_id, i));
                    ui.close();
                }
            });
        }
    }

    // A link being drawn follows the pointer, and lands on release.
    if let Some((from, port)) = state.linking {
        let start = graph
            .node(from)
            .zip(cards.iter().find(|(id, _)| *id == from).map(|(_, c)| c))
            .map(|(n, card)| port_at(state, n, card, port));
        if let (Some(start), Some(end)) = (start, pointer) {
            link(&painter, start, end, CHOICE_LINK, state.zoom);
        }
        if ui.input(|i| i.pointer.any_released()) {
            state.linking = None;
            match (drop_target, pointer) {
                (Some(target), _) if target != from => set_link(graph, from, port, Some(target)),
                (Some(_), _) => out.notice = Some("A node can't lead to itself".into()),
                (None, Some(end)) if rect.contains(end) => {
                    let mut at = state.to_graph(origin, end);
                    at[1] -= HEADER / 2.0;
                    let made = graph.add(NodeKind::say("…"), at);
                    set_link(graph, from, port, Some(made));
                    state.selected = Some(made);
                }
                _ => {}
            }
        }
    }

    if let Some((node, port)) = disconnect {
        set_link(graph, node, port, None);
    }
    if let Some(start) = make_start {
        graph.start = start;
    }
    if let Some(source) = duplicate.and_then(|d| graph.node(d).cloned()) {
        let at = [source.pos[0] + 30.0, source.pos[1] + 30.0];
        let copy = graph.add(source.kind, at);
        if let Some(n) = graph.node_mut(copy) {
            n.title = source.title;
        }
        state.selected = Some(copy);
    }
    let delete_key = ui.input(|i| i.key_pressed(egui::Key::Delete))
        && !ui.ctx().egui_wants_keyboard_input()
        && ui
            .ctx()
            .pointer_hover_pos()
            .is_some_and(|p| rect.contains(p));
    if delete_key && let Some(selected) = state.selected {
        if selected == graph.start {
            out.notice =
                Some("The start node can't be deleted: make another node the start first".into());
        } else {
            remove = Some(selected);
        }
    }
    if let Some(r) = remove {
        graph.remove(r);
        if state.selected == Some(r) {
            state.selected = None;
        }
    }
    if graph.nodes.len() <= 1 {
        hint(&painter, rect);
    }
    out
}

/// Sets where port `port` of node `node` leads.
fn set_link(graph: &mut DialogueGraph, node: u32, port: usize, to: Option<u32>) {
    if let Some(n) = graph.node_mut(node)
        && let Some(link) = n.kind.links_mut().into_iter().nth(port)
    {
        *link = to;
    }
}

fn grid(painter: &egui::Painter, rect: Rect, state: &CanvasState) {
    let step = 40.0 * state.zoom;
    if step < 12.0 {
        return;
    }
    let start = rect.min
        + vec2(
            state.offset.x.rem_euclid(step),
            state.offset.y.rem_euclid(step),
        );
    let color = Color32::from_gray(38);
    let mut y = start.y;
    while y < rect.bottom() {
        let mut x = start.x;
        while x < rect.right() {
            painter.circle_filled(pos2(x, y), 1.2, color);
            x += step;
        }
        y += step;
    }
}

fn link(painter: &egui::Painter, from: Pos2, to: Pos2, color: Color32, zoom: f32) {
    let bend = ((to.x - from.x).abs() * 0.5).max(60.0 * zoom);
    let points = [from, from + vec2(bend, 0.0), to - vec2(bend, 0.0), to];
    painter.add(CubicBezierShape::from_points_stroke(
        points,
        false,
        Color32::TRANSPARENT,
        Stroke::new(2.0 * zoom.max(0.5), color.gamma_multiply(0.85)),
    ));
    // An arrowhead into the target.
    let size = 6.0 * zoom.max(0.6);
    painter.add(egui::Shape::convex_polygon(
        vec![
            to,
            to - vec2(size, size * 0.6),
            to - vec2(size, -size * 0.6),
        ],
        color,
        Stroke::NONE,
    ));
}

#[allow(clippy::too_many_arguments)]
fn draw_card(
    painter: &egui::Painter,
    r: Rect,
    n: &Node,
    card: &Card,
    zoom: f32,
    selected: bool,
    start: bool,
    problem: Option<&GraphProblem>,
) {
    let (kind_icon, kind_name, color) = match n.kind {
        NodeKind::Say { .. } => (icons::CHAT_TEARDROP_TEXT, "Say", SAY),
        NodeKind::Branch { .. } => (icons::GIT_FORK, "Branch", BRANCH),
    };
    let radius = 6.0 * zoom;
    painter.rect_filled(
        r.translate(vec2(0.0, 3.0 * zoom)),
        radius,
        Color32::from_black_alpha(90),
    );
    painter.rect_filled(r, radius, CARD);
    let header = Rect::from_min_size(r.min, vec2(r.width(), HEADER * zoom));
    painter.rect_filled(
        header,
        egui::CornerRadius {
            nw: radius as u8,
            ne: radius as u8,
            sw: 0,
            se: 0,
        },
        color.gamma_multiply(0.55),
    );
    let outline = if selected {
        Stroke::new(2.0, theme::ACCENT.gamma_multiply(1.4))
    } else if problem.is_some() {
        Stroke::new(1.5, theme::WARNING)
    } else {
        Stroke::new(1.0, Color32::from_gray(60))
    };
    painter.rect_stroke(r, radius, outline, egui::StrokeKind::Outside);
    let font = |size: f32| FontId::proportional(size * zoom);
    let x = r.left() + 8.0 * zoom;
    let title = if n.title.is_empty() {
        kind_name.to_owned()
    } else {
        n.title.clone()
    };
    let mut head = format!("{kind_icon}  {}", clip(&title, 26));
    if start {
        head = format!("{}  {head}", icons::FLAG);
    }
    painter.text(
        pos2(x, header.center().y),
        egui::Align2::LEFT_CENTER,
        head,
        font(12.5),
        Color32::WHITE,
    );
    let mut badge = format!("#{}", n.id);
    if problem.is_some() {
        badge = format!("{}  {badge}", icons::WARNING);
    }
    painter.text(
        pos2(r.right() - 8.0 * zoom, header.center().y),
        egui::Align2::RIGHT_CENTER,
        badge,
        font(11.0),
        if problem.is_some() {
            theme::WARNING
        } else {
            Color32::from_gray(200)
        },
    );
    // Input port.
    let input = pos2(r.left(), header.center().y);
    painter.circle_filled(input, 5.0 * zoom.max(0.6), color);
    let mut y = r.top() + (HEADER + PAD) * zoom;
    for line in &card.lines {
        painter.text(
            pos2(x, y + TEXT_ROW * zoom / 2.0),
            egui::Align2::LEFT_CENTER,
            clip(line, 36),
            font(11.5),
            theme::TEXT,
        );
        y += TEXT_ROW * zoom;
    }
    for (i, port) in card.ports.iter().enumerate() {
        let top = r.top() + card.port_top(i) * zoom;
        let row = Rect::from_min_size(
            pos2(r.left() + 4.0 * zoom, top + 1.0),
            vec2(r.width() - 8.0 * zoom, PORT_ROW * zoom - 2.0),
        );
        painter.rect_filled(row, 3.0 * zoom, Color32::from_gray(44));
        let text = if port.next.is_none() {
            format!("{}  (closes)", clip(&port.label, 26))
        } else {
            clip(&port.label, 34)
        };
        painter.text(
            pos2(x, row.center().y),
            egui::Align2::LEFT_CENTER,
            text,
            font(11.5),
            port.color,
        );
    }
}

/// How to use the canvas, while it is nearly empty.
fn hint(painter: &egui::Painter, rect: Rect) {
    painter.text(
        rect.center_bottom() - vec2(0.0, 18.0),
        egui::Align2::CENTER_BOTTOM,
        "Drag from an answer's dot to make the next node  ·  right-click for more  ·  scroll to zoom",
        FontId::proportional(12.0),
        theme::TEXT_DIM,
    );
}
