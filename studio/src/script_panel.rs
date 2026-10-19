//! The Script panel: the project's Rhai scripts (`scripts/*.rhai`) in a code editor. Problems
//! show as you type (gutter marks and a list that jumps to the line); the reference lists the
//! functions scripts can call and inserts one at the cursor.

use crate::app::{Focus, Pending, Studio};
use crate::code_view;
use crate::icons;
use crate::theme;
use c3_edit::ScriptDoc;
use c3_script::{Diagnostic, ScriptHost, Severity};
use eframe::egui::{self, RichText};

/// A new NPC script's text.
pub fn npc_template(npc_name: &str) -> String {
    format!(
        "// What {npc_name} says. talk() runs when the player clicks the NPC; each option\n\
         // names the function run when it is picked (none closes the dialogue).\n\
         \n\
         fn talk() {{\n    text(\"Hello, \" + player_name() + \".\");\n    option(\"Goodbye\");\n}}\n"
    )
}

/// A script open in the editor.
pub struct OpenScript {
    pub name: String,
    /// The editor's text; each change is recorded in `doc`.
    pub text: String,
    /// The script as recorded through c3-edit commands (undo, redo, saved state).
    doc: ScriptDoc,
    pub problems: Vec<Diagnostic>,
}

impl OpenScript {
    fn new(name: String, text: String) -> Self {
        Self {
            name,
            doc: ScriptDoc::new(text.clone()),
            text,
            problems: Vec::new(),
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.doc.is_dirty()
    }

    /// Records the editor's typing; a run of keystrokes is one undo step.
    fn record_typing(&mut self) {
        self.doc.type_text(self.text.clone());
    }
}

#[derive(Default)]
pub struct ScriptPanel {
    pub open: Option<OpenScript>,
    /// The project's script names.
    pub names: Vec<String>,
    search: String,
    /// The "New script" dialog's name field, while it is open.
    new_name: Option<String>,
    /// The function reference is shown unless hidden.
    hide_reference: bool,
    /// Checks scripts as they are typed (compiles, never runs).
    checker: Option<ScriptHost>,
}

impl ScriptPanel {
    pub fn is_dirty(&self) -> bool {
        self.open.as_ref().is_some_and(OpenScript::is_dirty)
    }

    /// Undoes the last step of the open script (outside the editor, which undoes its own
    /// typing while it has the keyboard).
    pub fn undo(&mut self, ctx: &egui::Context) -> Option<String> {
        self.step(ctx, true)
    }

    pub fn redo(&mut self, ctx: &egui::Context) -> Option<String> {
        self.step(ctx, false)
    }

    fn step(&mut self, ctx: &egui::Context, back: bool) -> Option<String> {
        if ctx.memory(|m| m.has_focus(editor_id())) {
            return None;
        }
        let open = self.open.as_mut()?;
        let label = if back {
            open.doc.undo()
        } else {
            open.doc.redo()
        }?;
        open.text = open.doc.text().to_owned();
        self.check();
        Some(format!("{label} in script {}", self.open.as_ref()?.name))
    }

    /// Labels for the Edit menu.
    pub fn undo_redo_labels(&self) -> (Option<String>, Option<String>) {
        let Some(open) = &self.open else {
            return (None, None);
        };
        (
            open.doc.undo_label().map(str::to_owned),
            open.doc.redo_label().map(str::to_owned),
        )
    }

    /// The open script's undo history, for the Undo History panel.
    pub fn history(&self) -> Vec<c3_edit::HistoryEntry> {
        self.open
            .as_ref()
            .map(|o| o.doc.history())
            .unwrap_or_default()
    }

    /// The open script's document, the editor's typing recorded first (commands from
    /// elsewhere, an agent's, go on from there).
    pub(crate) fn recorded_doc(&mut self) -> Option<&mut ScriptDoc> {
        let open = self.open.as_mut()?;
        open.doc.end_typing();
        Some(&mut open.doc)
    }

    /// The open script's document changed from elsewhere: the editor shows it.
    pub(crate) fn doc_changed(&mut self) {
        if let Some(open) = &mut self.open {
            open.text = open.doc.text().to_owned();
        }
        self.check();
    }

    fn check(&mut self) {
        let Some(open) = &mut self.open else { return };
        let checker = self
            .checker
            .get_or_insert_with(|| ScriptHost::new(std::path::PathBuf::new(), None));
        let entry = open.name.starts_with("npc/").then_some(c3_script::TALK);
        open.problems = checker.check(&open.text, entry);
    }
}

/// Whether `name` can name a script or dialogue file (`npc/guard`).
pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('/').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}

fn editor_id() -> egui::Id {
    egui::Id::unique("script editor")
}

impl Studio {
    pub(crate) fn refresh_scripts(&mut self) {
        self.scripts.names = self
            .project
            .as_ref()
            .map(|p| c3_script::script_names(&p.scripts_dir()))
            .unwrap_or_default();
    }

    pub(crate) fn script_panel(&mut self, ui: &mut egui::Ui) {
        if self.project.is_none() {
            theme::empty_state(
                ui,
                icons::FILE_CODE,
                "No project open",
                "Open a project to write its scripts.",
            );
            return;
        }
        if ui.rect_contains_pointer(ui.max_rect()) && ui.input(|i| i.pointer.any_pressed()) {
            self.focus = Focus::Script;
        }
        let mut open = None;
        egui::Panel::left("script list")
            .resizable(true)
            .default_size(190.0)
            .size_range(140.0..=360.0)
            .show(ui, |ui| open = self.script_list(ui));
        if !self.scripts.hide_reference {
            egui::Panel::right("script reference")
                .resizable(true)
                .default_size(260.0)
                .show(ui, |ui| self.script_reference(ui));
        }
        egui::CentralPanel::default().show(ui, |ui| {
            if self.scripts.open.is_some() {
                self.script_bar(ui);
                ui.separator();
                self.script_problems(ui);
                let open = self.scripts.open.as_mut().expect("checked above");
                let problems = open.problems.clone();
                if code_view::code_editor(ui, editor_id(), &mut open.text, &problems) {
                    open.record_typing();
                    self.scripts.check();
                } else if !ui.ctx().memory(|m| m.has_focus(editor_id())) {
                    // Leaving the editor ends the run of typing.
                    open.doc.end_typing();
                }
            } else {
                theme::empty_state(
                    ui,
                    icons::FILE_CODE,
                    "No script open",
                    "Pick a script on the left, or make one with New. NPCs run the script \
                     named in their Inspector.",
                );
            }
        });
        if let Some(name) = open {
            self.open_script(&name);
        }
        self.new_script_dialog(ui.ctx());
    }

    /// The list of scripts; returns the one clicked.
    fn script_list(&mut self, ui: &mut egui::Ui) -> Option<String> {
        let mut open = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Scripts").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(theme::icon_text(icons::PLUS, "New"))
                    .on_hover_text("Make a new script")
                    .clicked()
                {
                    self.scripts.new_name = Some("npc/".into());
                }
            });
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.scripts.search)
                .hint_text(theme::icon_text(icons::MAGNIFYING_GLASS, "Search"))
                .desired_width(f32::INFINITY),
        );
        ui.separator();
        if self.scripts.names.is_empty() {
            ui.label(RichText::new("No scripts yet. Make one with New.").color(theme::TEXT_DIM));
        }
        let needle = self.scripts.search.trim().to_lowercase();
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                    for name in &self.scripts.names {
                        if !name.to_lowercase().contains(&needle) {
                            continue;
                        }
                        let current = self.scripts.open.as_ref().filter(|s| &s.name == name);
                        let dirty = current.is_some_and(OpenScript::is_dirty);
                        let label = theme::icon_text(
                            icons::FILE_CODE,
                            &format!("{name}{}", if dirty { "*" } else { "" }),
                        );
                        if ui
                            .add(egui::Button::selectable(current.is_some(), label))
                            .clicked()
                        {
                            open = Some(name.clone());
                        }
                    }
                });
            });
        open
    }

    fn script_bar(&mut self, ui: &mut egui::Ui) {
        let Some(open) = &self.scripts.open else {
            return;
        };
        let mut save = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(theme::icon_text(icons::FILE_CODE, &open.name)).strong());
            let errors = open
                .problems
                .iter()
                .filter(|p| p.severity == Severity::Error)
                .count();
            let (icon, color, text) = match (errors, open.problems.len()) {
                (0, 0) => (
                    icons::CHECK_CIRCLE,
                    theme::SUCCESS,
                    "No problems".to_owned(),
                ),
                (0, n) => (icons::WARNING, theme::WARNING, plural(n, "warning")),
                (n, _) => (icons::X_CIRCLE, theme::DANGER, plural(n, "error")),
            };
            ui.label(RichText::new(theme::icon_text(icon, &text)).color(color));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                save = ui
                    .add_enabled(
                        open.is_dirty(),
                        egui::Button::new(theme::icon_text(icons::FLOPPY_DISK, "Save")),
                    )
                    .on_hover_text("Ctrl+S")
                    .clicked();
                if theme::icon_toggle(
                    ui,
                    !self.scripts.hide_reference,
                    icons::BOOK_OPEN,
                    "Functions scripts can call",
                )
                .clicked()
                {
                    self.scripts.hide_reference ^= true;
                }
            });
        });
        if save {
            self.save_script();
        }
    }

    /// The open script's problems; clicking one moves the cursor to its line.
    fn script_problems(&mut self, ui: &mut egui::Ui) {
        let Some(open) = &self.scripts.open else {
            return;
        };
        if open.problems.is_empty() {
            return;
        }
        let mut go = None;
        for p in &open.problems {
            let (icon, color) = match p.severity {
                Severity::Error => (icons::X_CIRCLE, theme::DANGER),
                Severity::Warning => (icons::WARNING, theme::WARNING),
            };
            let place = if p.line > 0 {
                format!("Line {}: ", p.line)
            } else {
                String::new()
            };
            let resp = ui.add(
                egui::Label::new(
                    RichText::new(theme::icon_text(icon, &format!("{place}{}", p.message)))
                        .color(color),
                )
                .sense(egui::Sense::click()),
            );
            if p.line > 0 && resp.on_hover_text("Go to the line").clicked() {
                go = Some(p.line);
            }
        }
        ui.separator();
        if let Some(line) = go {
            code_view::go_to_line(ui.ctx(), editor_id(), &open.text, line);
        }
    }

    /// The functions scripts can call; clicking one inserts it at the cursor.
    fn script_reference(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(theme::icon_text(icons::BOOK_OPEN, "Script functions")).strong());
        ui.label(RichText::new("Click one to insert it at the cursor.").color(theme::TEXT_DIM));
        ui.separator();
        let mut insert = None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for (signature, what) in c3_script::API {
                    let resp = ui.add(
                        egui::Label::new(RichText::new(*signature).monospace().color(theme::TEXT))
                            .sense(egui::Sense::click()),
                    );
                    if resp.on_hover_text("Insert at the cursor").clicked() {
                        insert = Some(*signature);
                    }
                    ui.label(RichText::new(*what).small().color(theme::TEXT_DIM));
                    ui.add_space(4.0);
                }
            });
        if let (Some(signature), Some(open)) = (insert, &mut self.scripts.open) {
            // `give_item(item[, n])` → `give_item(item)`: optional parts are left out.
            let call = match signature.find('[') {
                Some(i) => format!("{})", &signature[..i]),
                None => signature.to_owned(),
            };
            code_view::insert_at_cursor(ui.ctx(), editor_id(), &mut open.text, &call);
            open.doc
                .set_text(open.text.clone(), &format!("Insert {call}"));
            self.scripts.check();
        }
    }

    /// Opens script `name`, asking first if the open one has unsaved changes.
    pub(crate) fn open_script(&mut self, name: &str) {
        if self.scripts.open.as_ref().is_some_and(|s| s.name == name) {
            self.show_panel(crate::shell::Panel::Script);
            return;
        }
        if self.scripts.is_dirty() {
            self.pending = Some(Pending::OpenScript(name.to_owned()));
            return;
        }
        self.load_script(name);
    }

    pub(crate) fn load_script(&mut self, name: &str) {
        let Some(project) = &self.project else { return };
        let file = project
            .scripts_dir()
            .join(format!("{name}{}", c3_script::SCRIPT_SUFFIX));
        match std::fs::read_to_string(&file) {
            Ok(text) => {
                self.scripts.open = Some(OpenScript::new(name.to_owned(), text));
                self.scripts.check();
                self.focus = Focus::Script;
                self.show_panel(crate::shell::Panel::Script);
            }
            Err(e) => self.error = Some(format!("Can't open script {name}: {e}")),
        }
    }

    /// Writes the open script; false if that failed.
    pub(crate) fn save_script(&mut self) -> bool {
        let (Some(project), Some(open)) = (&self.project, &mut self.scripts.open) else {
            return true;
        };
        let file = project
            .scripts_dir()
            .join(format!("{}{}", open.name, c3_script::SCRIPT_SUFFIX));
        match std::fs::write(&file, &open.text) {
            Ok(()) => {
                open.doc.mark_saved();
                let name = open.name.clone();
                let errors = open.problems.iter().any(|p| p.severity == Severity::Error);
                self.notify(if errors {
                    format!("Saved script {name} (it has errors)")
                } else {
                    format!("Saved script {name}")
                });
                true
            }
            Err(e) => {
                self.error = Some(format!("Can't save script {}: {e}", open.name));
                false
            }
        }
    }

    /// Makes script `name` with `text` (unless it exists) and opens it.
    pub(crate) fn create_script(&mut self, name: &str, text: &str) -> bool {
        let Some(project) = &self.project else {
            return false;
        };
        let file = project
            .scripts_dir()
            .join(format!("{name}{}", c3_script::SCRIPT_SUFFIX));
        if !file.exists() {
            let made = file
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&file, text));
            if let Err(e) = made {
                self.error = Some(format!("Can't make script {name}: {e}"));
                return false;
            }
            self.notify(format!("Made script {name}"));
            self.refresh_scripts();
        }
        self.open_script(name);
        true
    }

    fn new_script_dialog(&mut self, ctx: &egui::Context) {
        let Some(name) = &mut self.scripts.new_name else {
            return;
        };
        let mut create = false;
        let mut close = false;
        egui::Window::new("New script")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Name (folders with /):");
                let field = ui.add(
                    egui::TextEdit::singleline(name)
                        .hint_text("npc/blacksmith")
                        .desired_width(260.0),
                );
                field.request_focus();
                let name = name.trim().trim_matches('/');
                let ok = valid_name(name);
                let taken = self.scripts.names.iter().any(|n| n == name);
                let hint = if !ok {
                    (
                        "Letters, digits, _ and - only, folders split by /",
                        theme::TEXT_DIM,
                    )
                } else if taken {
                    (
                        "A script with this name exists; it will be opened",
                        theme::WARNING,
                    )
                } else {
                    (
                        "Scripts under npc/ are NPC dialogues (they need fn talk())",
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
            let name = name.trim().trim_matches('/').to_owned();
            self.scripts.new_name = None;
            let title = name.rsplit('/').next().unwrap_or(&name).to_owned();
            self.create_script(&name, &npc_template(&title));
        } else if close {
            self.scripts.new_name = None;
        }
    }
}

fn plural(n: usize, what: &str) -> String {
    if n == 1 {
        format!("1 {what}")
    } else {
        format!("{n} {what}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_names_and_template() {
        assert!(valid_name("npc/black-smith_2"));
        assert!(!valid_name("npc//a") && !valid_name("../a") && !valid_name("a b"));
        let host = ScriptHost::new(std::path::PathBuf::new(), None);
        assert!(
            host.check(&npc_template("Guard"), Some(c3_script::TALK))
                .is_empty()
        );
    }
}
