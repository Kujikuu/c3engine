//! Editing one dialogue node: its title, what it does and says, the answers it offers (each
//! perhaps shown only when a condition holds) and where they lead; for a branch, its cases.

use crate::graph_canvas::node_label;
use crate::icons;
use crate::theme;
use c3_script::{Action, Case, Choice, Condition, DialogueGraph, NodeKind, PLACEHOLDERS, Quest};
use eframe::egui::{self, RichText};

/// A widget id part (combo boxes of one node differ by it).
trait Salt: std::hash::Hash + std::fmt::Debug + Copy {}
impl<T: std::hash::Hash + std::fmt::Debug + Copy> Salt for T {}

/// What a list row asked for.
enum RowEdit {
    Up,
    Down,
    Remove,
}

/// Up, down and remove buttons for row `i` of `len`.
fn row_buttons(ui: &mut egui::Ui, i: usize, len: usize) -> Option<RowEdit> {
    let mut edit = None;
    if ui
        .add(egui::Button::new(icons::X).frame(false))
        .on_hover_text("Remove")
        .clicked()
    {
        edit = Some(RowEdit::Remove);
    }
    if ui
        .add_enabled(
            i + 1 < len,
            egui::Button::new(icons::ARROW_DOWN).frame(false),
        )
        .on_hover_text("Move down")
        .clicked()
    {
        edit = Some(RowEdit::Down);
    }
    if ui
        .add_enabled(i > 0, egui::Button::new(icons::ARROW_UP).frame(false))
        .on_hover_text("Move up")
        .clicked()
    {
        edit = Some(RowEdit::Up);
    }
    edit
}

fn apply<T>(list: &mut Vec<T>, i: usize, edit: Option<RowEdit>) {
    match edit {
        Some(RowEdit::Up) => list.swap(i, i - 1),
        Some(RowEdit::Down) => list.swap(i, i + 1),
        Some(RowEdit::Remove) => {
            list.remove(i);
        }
        None => {}
    }
}

/// Edits the rows of `list` with `row`, keeping one move/remove per frame.
fn rows<T>(
    ui: &mut egui::Ui,
    list: &mut Vec<T>,
    mut row: impl FnMut(&mut egui::Ui, usize, &mut T),
) {
    let mut edit = None;
    let len = list.len();
    for (i, item) in list.iter_mut().enumerate() {
        egui::Frame::group(ui.style())
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}", i + 1)).color(theme::TEXT_DIM));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(e) = row_buttons(ui, i, len) {
                            edit = Some((i, e));
                        }
                    });
                });
                row(ui, i, item);
            });
    }
    if let Some((i, e)) = edit {
        apply(list, i, Some(e));
    }
}

fn number(ui: &mut egui::Ui, label: &str, value: &mut i64) {
    ui.label(RichText::new(label).color(theme::TEXT_DIM));
    ui.add(egui::DragValue::new(value).speed(1.0));
}

/// Picks a quest by name from the quest table.
fn quest_picker(ui: &mut egui::Ui, salt: impl Salt, id: &mut i64, quests: &[Quest]) {
    let shown = quests.iter().find(|q| q.id == *id).map_or_else(
        || format!("Quest {id} (not in table)"),
        |q| format!("{}  #{}", q.name, q.id),
    );
    egui::ComboBox::from_id_salt(salt)
        .selected_text(shown)
        .width(ui.available_width().min(200.0))
        .show_ui(ui, |ui| {
            if quests.is_empty() {
                ui.label(RichText::new("The quests table is empty").color(theme::TEXT_DIM));
            }
            for q in quests {
                ui.selectable_value(id, q.id, format!("{}  #{}", q.name, q.id));
            }
        });
}

/// Edits a condition's values (its kind is picked separately).
fn condition_values(ui: &mut egui::Ui, salt: impl Salt, c: &mut Condition, quests: &[Quest]) {
    ui.horizontal_wrapped(|ui| match c {
        Condition::Level(n) | Condition::Money(n) => number(ui, "at least", n),
        Condition::Item { item, count } => {
            number(ui, "item type", item);
            number(ui, "count", count);
        }
        Condition::Flag { name, value } => {
            ui.add(
                egui::TextEdit::singleline(name)
                    .desired_width(90.0)
                    .hint_text("variable"),
            );
            number(ui, "=", value);
        }
        Condition::QuestNotTaken(q)
        | Condition::QuestTaken(q)
        | Condition::QuestReady(q)
        | Condition::QuestDone(q) => quest_picker(ui, (salt, "quest"), q, quests),
    });
}

/// Picks a condition's kind.
fn condition_kind(ui: &mut egui::Ui, salt: impl Salt, c: &mut Condition) {
    let mut kind = c.kind();
    egui::ComboBox::from_id_salt(salt)
        .selected_text(Condition::KINDS[kind])
        .show_ui(ui, |ui| {
            for (i, name) in Condition::KINDS.iter().enumerate() {
                ui.selectable_value(&mut kind, i, *name);
            }
        });
    if kind != c.kind() {
        *c = Condition::of_kind(kind);
    }
}

/// Edits an action: its kind and values.
fn action_row(ui: &mut egui::Ui, salt: impl Salt, a: &mut Action, quests: &[Quest]) {
    let mut kind = a.kind();
    egui::ComboBox::from_id_salt((salt, "kind"))
        .selected_text(Action::KINDS[kind])
        .show_ui(ui, |ui| {
            for (i, name) in Action::KINDS.iter().enumerate() {
                ui.selectable_value(&mut kind, i, *name);
            }
        });
    if kind != a.kind() {
        *a = Action::of_kind(kind);
    }
    ui.horizontal_wrapped(|ui| match a {
        Action::GiveMoney(n) | Action::TakeMoney(n) => number(ui, "amount", n),
        Action::GiveItem { item, count } | Action::TakeItem { item, count } => {
            number(ui, "item type", item);
            number(ui, "count", count);
        }
        Action::SetFlag { name, value } => {
            ui.add(
                egui::TextEdit::singleline(name)
                    .desired_width(90.0)
                    .hint_text("variable"),
            );
            number(ui, "=", value);
        }
        Action::Message(text) => {
            ui.add(
                egui::TextEdit::singleline(text)
                    .desired_width(f32::INFINITY)
                    .hint_text("Shown in the chat"),
            );
        }
        Action::Teleport { map, x, y } => {
            number(ui, "map", map);
            number(ui, "x", x);
            number(ui, "y", y);
        }
        Action::OpenShop => {
            ui.label(RichText::new("The NPC's goods (shop table)").color(theme::TEXT_DIM));
        }
        Action::StartQuest(q) | Action::CompleteQuest(q) => {
            quest_picker(ui, (salt, "quest"), q, quests)
        }
    });
}

/// Picks where a link leads: another node, or closing the dialogue.
fn target_picker(
    ui: &mut egui::Ui,
    salt: impl Salt,
    next: &mut Option<u32>,
    choices: &[(u32, String)],
) {
    let shown = match next {
        Some(id) => choices
            .iter()
            .find(|(c, _)| c == id)
            .map_or_else(|| format!("#{id} (missing)"), |(_, l)| l.clone()),
        None => "Closes the dialogue".to_owned(),
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new("Leads to").color(theme::TEXT_DIM));
        egui::ComboBox::from_id_salt(salt)
            .selected_text(shown)
            .width(ui.available_width().min(200.0))
            .show_ui(ui, |ui| {
                ui.selectable_value(next, None, "Closes the dialogue");
                for (id, label) in choices {
                    ui.selectable_value(next, Some(*id), label);
                }
            });
    });
}

/// Edits node `id` of `graph`.
pub fn node_editor(ui: &mut egui::Ui, graph: &mut DialogueGraph, id: u32, quests: &[Quest]) {
    let targets: Vec<(u32, String)> = graph
        .nodes
        .iter()
        .filter(|n| n.id != id)
        .map(|n| (n.id, node_label(n)))
        .collect();
    let start = graph.start;
    let mut make_start = false;
    let Some(node) = graph.node_mut(id) else {
        return;
    };
    let (icon, kind) = match node.kind {
        NodeKind::Say { .. } => (icons::CHAT_TEARDROP_TEXT, "Say"),
        NodeKind::Branch { .. } => (icons::GIT_FORK, "Branch"),
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(theme::icon_text(icon, &format!("{kind} node #{id}"))).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if start == id {
                ui.label(
                    RichText::new(theme::icon_text(icons::FLAG, "Start")).color(theme::SUCCESS),
                );
            } else {
                make_start = ui
                    .button(theme::icon_text(icons::FLAG, "Start here"))
                    .on_hover_text("talk() begins at this node")
                    .clicked();
            }
        });
    });
    ui.add(
        egui::TextEdit::singleline(&mut node.title)
            .hint_text("Note (what this node is for)")
            .desired_width(f32::INFINITY),
    );
    ui.separator();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| match &mut node.kind {
            NodeKind::Say {
                actions,
                lines,
                choices,
            } => say_editor(ui, actions, lines, choices, &targets, quests),
            NodeKind::Branch { cases, otherwise } => {
                branch_editor(ui, cases, otherwise, &targets, quests)
            }
        });
    if make_start {
        graph.start = id;
    }
}

fn say_editor(
    ui: &mut egui::Ui,
    actions: &mut Vec<Action>,
    lines: &mut Vec<String>,
    choices: &mut Vec<Choice>,
    targets: &[(u32, String)],
    quests: &[Quest],
) {
    theme::section(ui, "The NPC says", |ui| {
        let placeholders: Vec<&str> = PLACEHOLDERS.iter().map(|p| p.0).collect();
        ui.label(
            RichText::new(format!("Fill-ins: {}", placeholders.join("  ")))
                .small()
                .color(theme::TEXT_DIM),
        );
        let mut remove = None;
        for (i, line) in lines.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::multiline(line)
                        .desired_rows(1)
                        .desired_width(ui.available_width() - 24.0),
                );
                if ui
                    .add(egui::Button::new(icons::X).frame(false))
                    .on_hover_text("Remove line")
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            lines.remove(i);
        }
        if ui
            .button(theme::icon_text(icons::PLUS, "Add line"))
            .clicked()
        {
            lines.push(String::new());
        }
    });
    theme::section(ui, "Answers", |ui| {
        if choices.is_empty() {
            ui.label(
                RichText::new("No answers: the page shows with a Close button.")
                    .color(theme::TEXT_DIM),
            );
        }
        rows(ui, choices, |ui, i, c| {
            ui.add(
                egui::TextEdit::singleline(&mut c.label)
                    .hint_text("What the player says")
                    .desired_width(f32::INFINITY),
            );
            target_picker(ui, ("choice next", i), &mut c.next, targets);
            let mut conditional = c.condition.is_some();
            if ui
                .checkbox(&mut conditional, "Only when…")
                .on_hover_text("Offer this answer only when a condition holds")
                .changed()
            {
                c.condition = conditional.then_some(Condition::Level(1));
            }
            if let Some(cond) = &mut c.condition {
                condition_kind(ui, ("choice cond", i), cond);
                condition_values(ui, ("choice values", i), cond, quests);
            }
        });
        if ui
            .button(theme::icon_text(icons::PLUS, "Add answer"))
            .clicked()
        {
            choices.push(Choice {
                label: "Goodbye".into(),
                condition: None,
                next: None,
            });
        }
    });
    theme::section(ui, "First, do", |ui| {
        if actions.is_empty() {
            ui.label(
                RichText::new("Nothing: actions run before the NPC speaks.").color(theme::TEXT_DIM),
            );
        }
        rows(ui, actions, |ui, i, a| {
            action_row(ui, ("action", i), a, quests)
        });
        ui.menu_button(theme::icon_text(icons::PLUS, "Add action"), |ui| {
            for (i, name) in Action::KINDS.iter().enumerate() {
                if ui.button(*name).clicked() {
                    actions.push(Action::of_kind(i));
                    ui.close();
                }
            }
        });
    });
}

fn branch_editor(
    ui: &mut egui::Ui,
    cases: &mut Vec<Case>,
    otherwise: &mut Option<u32>,
    targets: &[(u32, String)],
    quests: &[Quest],
) {
    ui.label(
        RichText::new("Goes on to the first case that holds, top to bottom.")
            .color(theme::TEXT_DIM),
    );
    theme::section(ui, "Cases", |ui| {
        rows(ui, cases, |ui, i, c| {
            condition_kind(ui, ("case cond", i), &mut c.condition);
            condition_values(ui, ("case values", i), &mut c.condition, quests);
            target_picker(ui, ("case next", i), &mut c.next, targets);
        });
        if ui
            .button(theme::icon_text(icons::PLUS, "Add case"))
            .clicked()
        {
            cases.push(Case {
                condition: Condition::Level(1),
                next: None,
            });
        }
    });
    theme::section(ui, "Otherwise", |ui| {
        target_picker(ui, "otherwise", otherwise, targets);
    });
}
