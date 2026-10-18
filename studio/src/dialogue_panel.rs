//! The Dialogue panel: NPC dialogues drawn as graphs (`dialogues/*.dialogue.ron`) and the
//! project's quests. Saving a dialogue writes the script NPCs run
//! (`scripts/dialogue/<name>.rhai`); a quest-giver dialogue is made from a quest in one click.

use crate::app::{Focus, Pending, Studio};
use crate::graph_canvas::{self, CanvasState};
use crate::icons;
use crate::node_editor::node_editor;
use crate::script_panel::valid_name;
use crate::shell::Panel;
use crate::theme;
use c3_edit::DialogueDoc;
use c3_script::quest::QUEST_TABLE;
use c3_script::{DialogueGraph, GraphProblem, NodeKind, Quest};
use eframe::egui::{self, RichText};
use std::path::PathBuf;
use std::time::SystemTime;

/// A dialogue open in the editor.
pub struct OpenDialogue {
    pub name: String,
    /// The graph as the canvas edits it; committed to `doc` as one undo step at a time.
    pub graph: DialogueGraph,
    /// The dialogue as recorded through c3-edit commands (undo, redo, saved state).
    doc: DialogueDoc,
    canvas: CanvasState,
    problems: Vec<GraphProblem>,
    /// Shows the script the dialogue makes instead of the graph.
    show_script: bool,
}

impl OpenDialogue {
    fn new(name: String, graph: DialogueGraph) -> Self {
        let problems = graph.problems();
        Self {
            name,
            doc: DialogueDoc::new(graph.clone()),
            graph,
            canvas: CanvasState::default(),
            problems,
            show_script: false,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.graph != *self.doc.graph() || self.doc.is_dirty()
    }

    /// Records the edits since the last step as one undo step (the commands that make them).
    fn commit(&mut self) {
        if !self.doc.change_to(&self.graph).is_empty() {
            self.problems = self.graph.problems();
        }
    }

    /// Undoes (`back`) or redoes a step; its name.
    fn step(&mut self, back: bool) -> Option<String> {
        self.commit();
        let label = if back {
            self.doc.undo()
        } else {
            self.doc.redo()
        }?;
        self.graph = self.doc.graph().clone();
        self.problems = self.graph.problems();
        if self
            .canvas
            .selected
            .is_some_and(|s| self.graph.node(s).is_none())
        {
            self.canvas.selected = None;
        }
        Some(label)
    }
}

/// The quest table, read again when its file changes.
#[derive(Default)]
struct QuestCache {
    file: Option<PathBuf>,
    modified: Option<SystemTime>,
    quests: Vec<Quest>,
    /// Whether the table file exists.
    exists: bool,
}

impl QuestCache {
    fn refresh(&mut self, file: PathBuf) {
        let modified = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
        if self.file.as_ref() == Some(&file) && self.modified == modified {
            return;
        }
        self.exists = modified.is_some();
        self.quests = c3_data::load_table(&file)
            .map(|t| Quest::all(&t))
            .unwrap_or_default();
        self.file = Some(file);
        self.modified = modified;
    }
}

/// The "New dialogue" dialog: a name, and the quest it gives (if made for one).
struct NewDialogue {
    name: String,
    quest: Option<i64>,
}

#[derive(Default)]
pub struct DialoguePanel {
    pub open: Option<OpenDialogue>,
    /// The project's dialogue names.
    pub names: Vec<String>,
    search: String,
    new: Option<NewDialogue>,
    quests: QuestCache,
    /// A text field has the keyboard (Undo then belongs to it).
    pub typing: bool,
}

impl DialoguePanel {
    pub fn is_dirty(&self) -> bool {
        self.open.as_ref().is_some_and(OpenDialogue::is_dirty)
    }

    pub fn undo(&mut self) -> Option<String> {
        let open = self.open.as_mut()?;
        let label = open.step(true)?;
        Some(format!("{label} of dialogue {}", open.name))
    }

    pub fn redo(&mut self) -> Option<String> {
        let open = self.open.as_mut()?;
        let label = open.step(false)?;
        Some(format!("{label} of dialogue {}", open.name))
    }

    /// The open dialogue's undo history, for the Undo History panel.
    pub fn history(&self) -> Vec<c3_edit::HistoryEntry> {
        self.open
            .as_ref()
            .map(|o| o.doc.history())
            .unwrap_or_default()
    }

    /// Selects node `id` of the open dialogue.
    pub fn select(&mut self, id: u32) {
        if let Some(open) = &mut self.open {
            open.canvas.selected = Some(id);
        }
    }

    /// Labels for the Edit menu.
    pub fn undo_redo_labels(&self) -> (Option<String>, Option<String>) {
        let Some(open) = &self.open else {
            return (None, None);
        };
        let pending =
            (open.graph != *open.doc.graph()).then(|| format!("Edit of dialogue {}", open.name));
        let undo = pending.or_else(|| open.doc.undo_label().map(str::to_owned));
        (undo, open.doc.redo_label().map(str::to_owned))
    }
}

/// A dialogue name for a quest ("Pheasant hunt" → `pheasant_hunt`).
fn name_for(quest: &Quest) -> String {
    let mut name: String = quest
        .name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    name = name.trim_matches('_').to_owned();
    if name.is_empty() {
        format!("quest_{}", quest.id)
    } else {
        name
    }
}

impl Studio {
    pub(crate) fn refresh_dialogues(&mut self) {
        self.dialogues.names = self
            .project
            .as_ref()
            .map(|p| c3_script::dialogue_names(&p.dialogues_dir()))
            .unwrap_or_default();
    }

    pub(crate) fn dialogue_panel(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            theme::empty_state(
                ui,
                icons::CHATS,
                "No project open",
                "Open a project to write its NPC dialogues and quests.",
            );
            return;
        };
        self.dialogues
            .quests
            .refresh(project.table_file(QUEST_TABLE));
        if ui.rect_contains_pointer(ui.max_rect()) && ui.input(|i| i.pointer.any_pressed()) {
            self.focus = Focus::Dialogue;
        }
        let mut open = None;
        egui::Panel::left("dialogue list")
            .resizable(true)
            .default_size(210.0)
            .size_range(160.0..=360.0)
            .show(ui, |ui| open = self.dialogue_list(ui));
        if self.dialogues.open.is_some() {
            egui::Panel::right("dialogue node")
                .resizable(true)
                .default_size(300.0)
                .size_range(240.0..=480.0)
                .show(ui, |ui| self.dialogue_node(ui));
        }
        egui::CentralPanel::default().show(ui, |ui| {
            if self.dialogues.open.is_some() {
                self.dialogue_bar(ui);
                ui.separator();
                self.dialogue_view(ui);
            } else {
                theme::empty_state(
                    ui,
                    icons::CHATS,
                    "No dialogue open",
                    "Pick a dialogue on the left, or make one with New. A quest in the list \
                     can make its quest-giver's dialogue for you.",
                );
            }
        });
        self.dialogues.typing = ui.ctx().egui_wants_keyboard_input();
        if let Some(open) = &mut self.dialogues.open {
            // Edits in progress (a drag, a focused field) finish before they become a step.
            let busy = ui.input(|i| i.pointer.any_down()) || self.dialogues.typing;
            if !busy {
                open.commit();
            }
        }
        if let Some(name) = open {
            self.open_dialogue(&name);
        }
        self.new_dialogue_dialog(ui.ctx());
    }

    /// The dialogues and quests; returns the dialogue clicked.
    fn dialogue_list(&mut self, ui: &mut egui::Ui) -> Option<String> {
        let mut open = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Dialogues").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(theme::icon_text(icons::PLUS, "New"))
                    .on_hover_text("Make a new dialogue")
                    .clicked()
                {
                    self.dialogues.new = Some(NewDialogue {
                        name: String::new(),
                        quest: None,
                    });
                }
            });
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.dialogues.search)
                .hint_text(theme::icon_text(icons::MAGNIFYING_GLASS, "Search"))
                .desired_width(f32::INFINITY),
        );
        ui.separator();
        let needle = self.dialogues.search.trim().to_lowercase();
        egui::ScrollArea::vertical()
            .id_salt("dialogue names")
            .max_height(ui.available_height() * 0.55)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if self.dialogues.names.is_empty() {
                    ui.label(
                        RichText::new("No dialogues yet. Make one with New.")
                            .color(theme::TEXT_DIM),
                    );
                }
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                    for name in &self.dialogues.names {
                        if !name.to_lowercase().contains(&needle) {
                            continue;
                        }
                        let current = self.dialogues.open.as_ref().filter(|d| &d.name == name);
                        let dirty = current.is_some_and(OpenDialogue::is_dirty);
                        let label = theme::icon_text(
                            icons::CHATS,
                            &format!("{name}{}", if dirty { "*" } else { "" }),
                        );
                        if ui
                            .add(egui::Button::selectable(current.is_some(), label))
                            .on_hover_text(format!(
                                "NPCs run it as script {}",
                                c3_script::generated_script(name)
                            ))
                            .clicked()
                        {
                            open = Some(name.clone());
                        }
                    }
                });
            });
        ui.add_space(8.0);
        self.quest_list(ui);
        open
    }

    /// The quest table's quests, each able to make its giver's dialogue.
    fn quest_list(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(theme::icon_text(icons::SCROLL, "Quests")).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.dialogues.quests.exists {
                    if ui
                        .button(theme::icon_text(icons::DATABASE, "Edit"))
                        .on_hover_text("Edit the quests table in the Data panel")
                        .clicked()
                    {
                        self.open_table(QUEST_TABLE);
                        self.show_panel(Panel::Data);
                    }
                } else if ui
                    .button(theme::icon_text(icons::PLUS, "Create"))
                    .on_hover_text("Make the quests table (tables/quests.table.ron)")
                    .clicked()
                {
                    self.create_quest_table();
                }
            });
        });
        ui.separator();
        let quests = &self.dialogues.quests;
        if !quests.exists {
            ui.label(
                RichText::new("This project has no quests table yet. Create it, then add quests in the Data panel.")
                    .color(theme::TEXT_DIM),
            );
            return;
        }
        if quests.quests.is_empty() {
            ui.label(
                RichText::new("No quests yet: add rows to the quests table.")
                    .color(theme::TEXT_DIM),
            );
        }
        let mut make = None;
        egui::ScrollArea::vertical()
            .id_salt("quest names")
            .auto_shrink(false)
            .show(ui, |ui| {
                for q in &quests.quests {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("#{}", q.id)).color(theme::TEXT_DIM));
                        let name = if q.name.is_empty() {
                            "(no name)"
                        } else {
                            q.name.as_str()
                        };
                        ui.label(name).on_hover_text(if q.description.is_empty() {
                            "No description"
                        } else {
                            q.description.as_str()
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(egui::Button::new(icons::CHATS).frame(false))
                                .on_hover_text("Make this quest's giver dialogue")
                                .clicked()
                            {
                                make = Some(q.clone());
                            }
                        });
                    });
                }
            });
        if let Some(q) = make {
            self.dialogues.new = Some(NewDialogue {
                name: name_for(&q),
                quest: Some(q.id),
            });
        }
    }

    fn create_quest_table(&mut self) {
        let Some(project) = &self.project else { return };
        let file = project.table_file(QUEST_TABLE);
        if let Err(e) = c3_data::save_table(&Quest::new_table(), &file) {
            self.error = Some(format!("Can't make the quests table: {e}"));
            return;
        }
        self.notify("Made the quests table: add quests in the Data panel".into());
        self.refresh_tables();
        self.open_table(QUEST_TABLE);
        self.show_panel(Panel::Data);
    }

    fn dialogue_bar(&mut self, ui: &mut egui::Ui) {
        let Some(open) = &mut self.dialogues.open else {
            return;
        };
        let mut save = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(theme::icon_text(icons::CHATS, &open.name)).strong());
            let (icon, color, text) = match open.problems.len() {
                0 => (
                    icons::CHECK_CIRCLE,
                    theme::SUCCESS,
                    "No problems".to_owned(),
                ),
                1 => (icons::WARNING, theme::WARNING, "1 problem".to_owned()),
                n => (icons::WARNING, theme::WARNING, format!("{n} problems")),
            };
            let resp = ui.add(
                egui::Label::new(RichText::new(theme::icon_text(icon, &text)).color(color))
                    .sense(egui::Sense::click()),
            );
            if !open.problems.is_empty() {
                let list: Vec<String> = open
                    .problems
                    .iter()
                    .map(|p| match p.node {
                        Some(n) => format!("Node #{n}: {}", p.message),
                        None => p.message.clone(),
                    })
                    .collect();
                if resp
                    .on_hover_text(format!("{}\n\nClick to show the next one", list.join("\n")))
                    .clicked()
                {
                    // Cycle through the nodes with problems.
                    let nodes: Vec<u32> = open.problems.iter().filter_map(|p| p.node).collect();
                    let next = nodes
                        .iter()
                        .copied()
                        .find(|&n| Some(n) > open.canvas.selected)
                        .or(nodes.first().copied());
                    if let Some(n) = next {
                        open.canvas.focus(&open.graph, n);
                    }
                }
            }
            if open.is_dirty() {
                ui.label(RichText::new("Unsaved").color(theme::TEXT_DIM));
            }
        });
        ui.horizontal(|ui| {
            // Narrow panels show the tools as icons (their hints still say what they do).
            let narrow = ui.available_width() < 360.0;
            let tool = |icon: &str, text: &str| {
                if narrow {
                    icon.to_owned()
                } else {
                    theme::icon_text(icon, text)
                }
            };
            let add = |kind: NodeKind, open: &mut OpenDialogue| {
                let at = open
                    .canvas
                    .selected
                    .and_then(|s| open.graph.node(s))
                    .map_or([40.0, 40.0], |n| [n.pos[0] + 300.0, n.pos[1]]);
                open.canvas.selected = Some(open.graph.add(kind, at));
            };
            if ui
                .button(tool(icons::CHAT_TEARDROP_TEXT, "Say"))
                .on_hover_text("Add a node where the NPC speaks and the player answers")
                .clicked()
            {
                add(NodeKind::say("…"), open);
            }
            if ui
                .button(tool(icons::GIT_FORK, "Branch"))
                .on_hover_text("Add a node that goes on by conditions (level, items, quests)")
                .clicked()
            {
                add(
                    NodeKind::Branch {
                        cases: Vec::new(),
                        otherwise: None,
                    },
                    open,
                );
            }
            if ui
                .button(tool(icons::CORNERS_OUT, "Fit"))
                .on_hover_text("Fit every node in view")
                .clicked()
            {
                open.canvas.fit = true;
                open.show_script = false;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                save = ui
                    .add_enabled(
                        open.is_dirty(),
                        egui::Button::new(tool(icons::FLOPPY_DISK, "Save")),
                    )
                    .on_hover_text("Ctrl+S: saves the dialogue and writes its script")
                    .clicked();
                if theme::icon_toggle(
                    ui,
                    open.show_script,
                    icons::FILE_CODE,
                    "Show the script this dialogue makes",
                )
                .clicked()
                {
                    open.show_script ^= true;
                }
            });
        });
        if save {
            self.save_dialogue();
        }
    }

    fn dialogue_view(&mut self, ui: &mut egui::Ui) {
        let quests = &self.dialogues.quests.quests;
        let Some(open) = &mut self.dialogues.open else {
            return;
        };
        if open.show_script {
            let script = open.graph.to_script(&open.name);
            ui.label(
                RichText::new(format!(
                    "Read only: saving writes this to scripts/{}.rhai",
                    c3_script::generated_script(&open.name)
                ))
                .color(theme::TEXT_DIM),
            );
            egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut script.as_str())
                        .code_editor()
                        .desired_width(f32::INFINITY),
                );
            });
            return;
        }
        let out = graph_canvas::canvas(
            ui,
            &mut open.graph,
            &mut open.canvas,
            &open.problems,
            quests,
        );
        if let Some(text) = out.notice {
            self.notify(text);
        }
    }

    fn dialogue_node(&mut self, ui: &mut egui::Ui) {
        let quests = &self.dialogues.quests.quests;
        let Some(open) = &mut self.dialogues.open else {
            return;
        };
        match open.canvas.selected {
            Some(id) => node_editor(ui, &mut open.graph, id, quests),
            None => {
                theme::empty_state(
                    ui,
                    icons::CURSOR,
                    "No node selected",
                    "Click a node to edit it. Drag from an answer's dot onto a node to link \
                     them, or onto empty space to make the next node.",
                );
            }
        }
    }

    /// Opens dialogue `name`, asking first if the open one has unsaved changes.
    pub(crate) fn open_dialogue(&mut self, name: &str) {
        if self.dialogues.open.as_ref().is_some_and(|d| d.name == name) {
            self.show_panel(Panel::Dialogue);
            return;
        }
        if self.dialogues.is_dirty() {
            self.pending = Some(Pending::OpenDialogue(name.to_owned()));
            return;
        }
        self.load_dialogue(name);
    }

    pub(crate) fn load_dialogue(&mut self, name: &str) {
        let Some(project) = &self.project else { return };
        let file = project.dialogue_file(name);
        match c3_script::load_dialogue(&file) {
            Ok(graph) => {
                self.dialogues.open = Some(OpenDialogue::new(name.to_owned(), graph));
                self.focus = Focus::Dialogue;
                self.show_panel(Panel::Dialogue);
            }
            Err(e) => self.error = Some(format!("Can't open dialogue {name}: {e}")),
        }
    }

    /// Writes the open dialogue and its script; false if that failed.
    pub(crate) fn save_dialogue(&mut self) -> bool {
        let (Some(project), Some(open)) = (&self.project, &mut self.dialogues.open) else {
            return true;
        };
        open.commit();
        let script_file = project.scripts_dir().join(format!(
            "{}{}",
            c3_script::generated_script(&open.name),
            c3_script::SCRIPT_SUFFIX
        ));
        let written = c3_script::save_dialogue(&open.graph, &project.dialogue_file(&open.name))
            .and_then(|()| {
                if let Some(dir) = script_file.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(&script_file, open.graph.to_script(&open.name))
            });
        match written {
            Ok(()) => {
                open.commit();
                open.doc.mark_saved();
                let msg = match open.problems.len() {
                    0 => format!(
                        "Saved dialogue {} (script {})",
                        open.name,
                        c3_script::generated_script(&open.name)
                    ),
                    n => format!("Saved dialogue {} ({n} problems to look at)", open.name),
                };
                self.notify(msg);
                self.refresh_scripts();
                true
            }
            Err(e) => {
                self.error = Some(format!("Can't save dialogue {}: {e}", open.name));
                false
            }
        }
    }

    /// Makes dialogue `name` from `graph` (unless it exists), writes its script and opens it.
    pub(crate) fn create_dialogue(&mut self, name: &str, graph: DialogueGraph) -> bool {
        let Some(project) = &self.project else {
            return false;
        };
        if !project.dialogue_file(name).exists() {
            if self.dialogues.is_dirty() && !self.save_dialogue() {
                return false;
            }
            self.dialogues.open = Some(OpenDialogue::new(name.to_owned(), graph));
            if !self.save_dialogue() {
                self.dialogues.open = None;
                return false;
            }
            self.notify(format!(
                "Made dialogue {name}: NPCs run it as script {}",
                c3_script::generated_script(name)
            ));
            self.refresh_dialogues();
        }
        self.open_dialogue(name);
        true
    }

    fn new_dialogue_dialog(&mut self, ctx: &egui::Context) {
        let Some(new) = &mut self.dialogues.new else {
            return;
        };
        let quests = &self.dialogues.quests.quests;
        let mut create = false;
        let mut close = false;
        egui::Window::new("New dialogue")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Name (folders with /):");
                let field = ui.add(
                    egui::TextEdit::singleline(&mut new.name)
                        .hint_text("guard")
                        .desired_width(280.0),
                );
                field.request_focus();
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Start from");
                    let shown = new
                        .quest
                        .and_then(|id| quests.iter().find(|q| q.id == id))
                        .map_or("A greeting".to_owned(), |q| {
                            format!("Quest giver: {}", q.name)
                        });
                    egui::ComboBox::from_id_salt("new dialogue template")
                        .selected_text(shown)
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut new.quest, None, "A greeting");
                            for q in quests {
                                ui.selectable_value(
                                    &mut new.quest,
                                    Some(q.id),
                                    format!("Quest giver: {}", q.name),
                                );
                            }
                        });
                });
                let name = new.name.trim().trim_matches('/');
                let ok = valid_name(name);
                let taken = self.dialogues.names.iter().any(|n| n == name);
                let hint = if !ok {
                    (
                        "Letters, digits, _ and - only, folders split by /",
                        theme::TEXT_DIM,
                    )
                } else if taken {
                    (
                        "A dialogue with this name exists; it will be opened",
                        theme::WARNING,
                    )
                } else {
                    (
                        "Give it to an NPC in its Inspector (Script)",
                        theme::TEXT_DIM,
                    )
                };
                ui.label(RichText::new(hint.0).small().color(hint.1));
                ui.horizontal(|ui| {
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    create = ui
                        .add_enabled(
                            ok,
                            egui::Button::new(theme::icon_text(icons::PLUS, "Create")),
                        )
                        .clicked()
                        || (ok && enter);
                    close = ui.button("Cancel").clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Escape));
                });
            });
        if create {
            let name = new.name.trim().trim_matches('/').to_owned();
            let graph = new
                .quest
                .and_then(|id| quests.iter().find(|q| q.id == id))
                .map_or_else(DialogueGraph::default, DialogueGraph::quest_giver);
            self.dialogues.new = None;
            self.create_dialogue(&name, graph);
        } else if close {
            self.dialogues.new = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_steps_and_quest_names() {
        let mut open = OpenDialogue::new("a".into(), DialogueGraph::default());
        open.graph.add(NodeKind::say("one"), [0.0, 0.0]);
        open.commit();
        open.graph.add(NodeKind::say("two"), [0.0, 0.0]);
        assert!(open.step(true).is_some(), "the pending edit is a step too");
        assert_eq!(open.graph.nodes.len(), 2);
        assert!(open.step(true).is_some());
        assert_eq!(open.graph.nodes.len(), 1);
        assert!(open.step(true).is_none());
        assert!(open.step(false).is_some());
        assert_eq!(open.graph.nodes.len(), 2);
        let mut quest = Quest {
            id: 4,
            name: "Pheasant hunt!".into(),
            description: String::new(),
            giver: 0,
            min_level: 0,
            goal: c3_script::Goal::Talk,
            reward_money: 0,
            reward_exp: 0,
            reward_item: 0,
        };
        assert_eq!(name_for(&quest), "pheasant_hunt");
        quest.name = "?".into();
        assert_eq!(name_for(&quest), "quest_4");
    }
}
