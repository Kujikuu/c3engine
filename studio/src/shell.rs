//! The editor shell, laid out like Unity's: a menu bar, a toolbar (tools · Play/Pause/Step ·
//! layout), and dockable panels (Hierarchy, Scene, Game, Preview, Inspector, Undo History,
//! Project, Console, Data, Script). The layout is saved between sessions.

use crate::app::{Studio, Workspace};
use crate::dock::{Dock, Node, TabViewer};
use crate::editor::Tool;
use crate::icons;
use crate::maps::MapRef;
use crate::theme;
use eframe::egui::{self, RichText};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A dockable panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Panel {
    Hierarchy,
    Scene,
    Game,
    /// Characters, actions and effects on a stage.
    Preview,
    Inspector,
    History,
    Project,
    Console,
    /// Game data tables.
    Data,
    /// The script editor.
    Script,
    /// Dialogue graphs and quests.
    Dialogue,
}

impl Panel {
    pub const ALL: [Panel; 11] = [
        Panel::Scene,
        Panel::Game,
        Panel::Preview,
        Panel::Script,
        Panel::Dialogue,
        Panel::Hierarchy,
        Panel::Inspector,
        Panel::Project,
        Panel::Data,
        Panel::Console,
        Panel::History,
    ];

    pub fn icon(self) -> &'static str {
        match self {
            Panel::Hierarchy => icons::TREE_STRUCTURE,
            Panel::Scene => icons::MAP_TRIFOLD,
            Panel::Game => icons::GAME_CONTROLLER,
            Panel::Preview => icons::PERSON_SIMPLE,
            Panel::Inspector => icons::SLIDERS,
            Panel::History => icons::CLOCK_COUNTER_CLOCKWISE,
            Panel::Project => icons::FOLDER_OPEN,
            Panel::Console => icons::TERMINAL,
            Panel::Data => icons::DATABASE,
            Panel::Script => icons::FILE_CODE,
            Panel::Dialogue => icons::CHATS,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Panel::Hierarchy => "Hierarchy",
            Panel::Scene => "Scene",
            Panel::Game => "Game",
            Panel::Preview => "Preview",
            Panel::Inspector => "Inspector",
            Panel::History => "Undo History",
            Panel::Project => "Project",
            Panel::Console => "Console",
            Panel::Data => "Data",
            Panel::Script => "Script",
            Panel::Dialogue => "Dialogue",
        }
    }

    /// Panels a reopened panel joins (its usual neighbours).
    fn neighbours(self) -> &'static [Panel] {
        match self {
            Panel::Scene | Panel::Game | Panel::Preview | Panel::Script | Panel::Dialogue => &[
                Panel::Scene,
                Panel::Game,
                Panel::Preview,
                Panel::Script,
                Panel::Dialogue,
            ],
            Panel::Inspector | Panel::History => &[Panel::Inspector, Panel::History],
            Panel::Project | Panel::Console | Panel::Data => {
                &[Panel::Project, Panel::Data, Panel::Console]
            }
            Panel::Hierarchy => &[Panel::Hierarchy],
        }
    }
}

/// Unity's default arrangement: Hierarchy left, Scene/Game centre, Inspector right,
/// Project/Console along the bottom.
pub(crate) fn default_layout() -> Dock<Panel> {
    Dock::new(|group| {
        let left = group(vec![Panel::Hierarchy]);
        let centre = group(vec![
            Panel::Scene,
            Panel::Game,
            Panel::Preview,
            Panel::Script,
            Panel::Dialogue,
        ]);
        let bottom = group(vec![Panel::Project, Panel::Data, Panel::Console]);
        let right = group(vec![Panel::Inspector, Panel::History]);
        let top = Node::split(false, 0.2, left, centre);
        let main = Node::split(true, 0.7, top, bottom);
        Node::split(false, 0.76, main, right)
    })
}

fn layout_file() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("C3Engine").join("layout.ron"))
}

/// The saved layout, or the default one.
pub(crate) fn load_layout() -> Dock<Panel> {
    layout_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|text| ron::from_str(&text).ok())
        .unwrap_or_else(default_layout)
}

/// Severity of a console message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    Info,
    Error,
}

pub(crate) struct LogEntry {
    pub level: Level,
    pub text: String,
    /// Seconds since the Studio started.
    pub at: f32,
}

/// Most console messages kept.
const MAX_LOG: usize = 500;

impl Studio {
    // ---- menu bar ----

    pub(crate) fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.label(RichText::new("C3 Studio").strong().color(theme::BRAND));
            ui.add_space(6.0);
            self.file_menu(ui);
            self.edit_menu(ui);
            self.window_menu(ui);
            ui.menu_button("Help", |ui| {
                ui.label(RichText::new("C3 Studio 0.1").strong());
                ui.label(
                    RichText::new("TQ's C3 engine with a Unity-style editor.")
                        .color(theme::TEXT_DIM),
                );
            });
        });
    }

    fn edit_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Edit", |ui| {
            let (undo, redo) = self.undo_redo_labels();
            let item = |what: &str, step: &Option<String>| match step {
                Some(label) => format!("{what} {label}"),
                None => what.to_owned(),
            };
            let undo_button = egui::Button::new(item("Undo", &undo)).shortcut_text("Ctrl+Z");
            if ui.add_enabled(undo.is_some(), undo_button).clicked() {
                self.undo();
            }
            let redo_button = egui::Button::new(item("Redo", &redo)).shortcut_text("Ctrl+Y");
            if ui.add_enabled(redo.is_some(), redo_button).clicked() {
                self.redo();
            }
            ui.separator();
            let selected = self.editor.selected.is_some() && self.doc.is_some();
            let duplicate = egui::Button::new(theme::icon_text(icons::COPY, "Duplicate"))
                .shortcut_text("Ctrl+D");
            if ui.add_enabled(selected, duplicate).clicked() {
                self.duplicate_selection();
            }
            let delete =
                egui::Button::new(theme::icon_text(icons::TRASH, "Delete")).shortcut_text("Del");
            if ui.add_enabled(selected, delete).clicked() {
                self.delete_selection();
            }
            ui.separator();
            if ui
                .button(theme::icon_text(
                    icons::CLOCK_COUNTER_CLOCKWISE,
                    "Undo History",
                ))
                .clicked()
            {
                self.show_panel(Panel::History);
            }
        });
    }

    fn window_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Window", |ui| {
            for panel in Panel::ALL {
                let open = self.dock.contains(panel);
                let label = theme::icon_text(panel.icon(), panel.name());
                if ui.add(egui::Button::selectable(open, label)).clicked() {
                    self.show_panel(panel);
                }
            }
            ui.separator();
            if ui
                .button(theme::icon_text(icons::LAYOUT, "Reset Layout"))
                .clicked()
            {
                self.dock = default_layout();
                self.dock.changed = true;
            }
        });
    }

    pub(crate) fn show_panel(&mut self, panel: Panel) {
        self.dock.show_tab(panel, panel.neighbours());
    }

    // ---- toolbar ----

    pub(crate) fn main_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            // Scene tools, like Unity's transform tools (keys 1–4).
            let map_open = self.doc.is_some() && self.workspace == Workspace::Maps;
            ui.add_enabled_ui(map_open, |ui| {
                for tool in Tool::ALL {
                    let hint = format!("{} ({})\n{}", tool.label(), tool.key(), tool.hint());
                    if theme::icon_toggle(ui, self.editor.tool == tool, tool.icon(), &hint)
                        .clicked()
                    {
                        self.editor.tool = tool;
                    }
                }
            });
            if let Some(project) = &self.project {
                ui.separator();
                ui.label(RichText::new(project.name()).color(theme::TEXT_DIM))
                    .on_hover_text(project.root.display().to_string());
            }

            // Play controls in the middle.
            let middle = ui.available_rect_before_wrap().center().x;
            let width = 4.0 * 30.0;
            ui.add_space((middle - ui.cursor().left() - width / 2.0).max(8.0));
            let playing = self.playing.is_some();
            let play_hint = if playing {
                "Stop playing (Ctrl+P)"
            } else {
                "Play this map in the Game view, the hero where the Scene view looks (Ctrl+P)"
            };
            let play_icon = if playing { icons::STOP } else { icons::PLAY };
            ui.add_enabled_ui(self.doc.is_some() && self.game.is_some(), |ui| {
                if theme::icon_toggle(ui, playing, play_icon, play_hint).clicked() {
                    self.toggle_play();
                }
            });
            let paused = self.playing.as_ref().is_some_and(|p| p.paused);
            let online = self.playing.as_ref().is_some_and(|p| p.server_addr().is_some());
            ui.add_enabled_ui(playing && !online, |ui| {
                let pause = theme::icon_toggle(ui, paused, icons::PAUSE, "Pause (Ctrl+Shift+P)")
                    .on_disabled_hover_text("A server keeps its own time: online play can't pause");
                if pause.clicked()
                    && let Some(session) = &mut self.playing
                {
                    session.paused ^= true;
                }
                let step = theme::icon_toggle(ui, false, icons::SKIP_FORWARD, "Step one frame")
                    .on_disabled_hover_text("A server keeps its own time: online play can't step");
                if step.clicked()
                    && let Some(session) = &mut self.playing
                {
                    session.paused = true;
                    session.step();
                }
            });
            ui.add_enabled_ui(!playing, |ui| {
                let hint = if self.play_online {
                    "Online: Play runs a local game server and joins it, as players will (click for offline)"
                } else {
                    "Offline: Play simulates the game in the Studio (click to play through a local server)"
                };
                if theme::icon_toggle(ui, self.play_online, icons::HARD_DRIVES, hint)
                    .on_disabled_hover_text("Stop playing to switch between offline and online")
                    .clicked()
                {
                    self.play_online ^= true;
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button(theme::icon_text(icons::LAYOUT, "Layout"), |ui| {
                    if ui.button("Default").clicked() {
                        self.dock = default_layout();
                        self.dock.changed = true;
                    }
                });
            });
        });
    }

    // ---- play ----

    /// Starts the game client on the open map at the Scene view's centre, or stops it.
    /// Starts playing the open map in the Game view, or stops.
    pub(crate) fn toggle_play(&mut self) {
        if self.playing.take().is_some() {
            self.show_panel(Panel::Scene);
            self.notify("Stopped playing".into());
            return;
        }
        let (Some(game), Some(doc), Some(state)) = (&self.game, &self.doc, &self.render_state)
        else {
            return;
        };
        let start = doc.world_to_cell(self.cam.center);
        let setup = crate::play::PlaySetup {
            scripts: self.project.as_ref().map(|p| p.scripts_dir()),
            tables: self.project.as_ref().map(|p| p.tables_dir()),
            map_ids: game.map_ids(doc.name()),
            online: self.play_online,
        };
        match crate::play::PlaySession::start(std::rc::Rc::clone(game), doc, start, state, setup) {
            Ok(session) => {
                self.playing = Some(session);
                if self.scripts.is_dirty() {
                    self.log_info(
                        "Playing with the saved script: save it (Ctrl+S) to try your edits".into(),
                    );
                }
                self.show_panel(Panel::Game);
                self.notify("Playing: Stop (Ctrl+P) returns to editing".into());
            }
            Err(e) => self.error = Some(format!("Can't play this map: {e}")),
        }
    }

    /// Plays the saved project in the game client's own window, or closes it.
    pub(crate) fn toggle_game_window(&mut self) {
        if let Some(mut child) = self.game_window.take() {
            // The game may already have been closed by the player.
            let _ = child.kill();
            self.notify("Closed the game window".into());
            return;
        }
        let Some(project) = &self.project else { return };
        let exe = std::env::current_exe()
            .map(|e| e.with_file_name("c3-client.exe"))
            .unwrap_or_else(|_| PathBuf::from("c3-client.exe"));
        let mut command = std::process::Command::new(&exe);
        command.arg("--project").arg(&project.root);
        let mut unsaved = false;
        if let Some(doc) = &self.doc {
            command.arg("--map").arg(match &doc.map {
                MapRef::Client(path) => path.as_str(),
                MapRef::Project(name) => name.as_str(),
            });
            let (x, y) = doc.world_to_cell(self.cam.center);
            command.arg("--cell").arg(format!("{x},{y}"));
            unsaved = doc.is_dirty();
        }
        match command.spawn() {
            Ok(child) => {
                self.game_window = Some(child);
                self.notify(if unsaved {
                    "Playing the last saved version; save to include your changes".into()
                } else {
                    "Playing in the game window".into()
                });
            }
            Err(e) => self.error = Some(format!("Can't start the game ({}): {e}", exe.display())),
        }
    }

    /// Notices when the game window was closed.
    pub(crate) fn poll_play(&mut self) {
        if let Some(child) = &mut self.game_window
            && !matches!(child.try_wait(), Ok(None))
        {
            self.game_window = None;
        }
    }

    // ---- dock ----

    pub(crate) fn dock_ui(&mut self, ui: &mut egui::Ui) {
        let mut dock = std::mem::replace(&mut self.dock, Dock::new(|group| group(Vec::new())));
        dock.show(ui, &theme::dock_style(), self);
        self.dock = dock;
        if self.dock.changed {
            self.dock.changed = false;
            self.save_layout();
        }
    }

    fn save_layout(&mut self) {
        let Some(file) = layout_file() else { return };
        let saved = ron::to_string(&self.dock)
            .map_err(|e| e.to_string())
            .and_then(|text| {
                if let Some(dir) = file.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                std::fs::write(&file, text).map_err(|e| e.to_string())
            });
        if let Err(e) = saved {
            self.error = Some(format!("Can't save the window layout: {e}"));
        }
    }

    // ---- console ----

    /// Copies new status messages into the console log.
    pub(crate) fn sync_console(&mut self) {
        let at = self.started.elapsed().as_secs_f32();
        if self.error != self.logged_error {
            self.logged_error = self.error.clone();
            if let Some(e) = &self.error {
                self.push_log(Level::Error, e.clone(), at);
            }
        }
        let notice = self
            .notice
            .as_ref()
            .map(|(text, when)| (text.clone(), *when));
        if let Some((text, when)) = notice
            && self.logged_notice != Some(when)
        {
            self.logged_notice = Some(when);
            self.push_log(Level::Info, text, at);
        }
    }

    /// Logs a line straight to the console (without the status bar).
    pub(crate) fn log_info(&mut self, text: String) {
        let at = self.started.elapsed().as_secs_f32();
        self.push_log(Level::Info, text, at);
    }

    /// Logs a problem straight to the console (without the status bar).
    pub(crate) fn log_error(&mut self, text: String) {
        let at = self.started.elapsed().as_secs_f32();
        self.push_log(Level::Error, text, at);
    }

    fn push_log(&mut self, level: Level, text: String, at: f32) {
        self.log.push(LogEntry { level, text, at });
        if self.log.len() > MAX_LOG {
            self.log.remove(0);
        }
    }
}

impl TabViewer<Panel> for Studio {
    fn title(&self, panel: Panel) -> String {
        let name = match panel {
            // Unity marks a scene with unsaved changes with an asterisk.
            Panel::Scene if self.map_dirty() => "Scene*",
            Panel::Data if self.data.is_dirty() => "Data*",
            Panel::Script if self.scripts.is_dirty() => "Script*",
            Panel::Dialogue if self.dialogues.is_dirty() => "Dialogue*",
            _ => panel.name(),
        };
        theme::icon_text(panel.icon(), name)
    }

    fn ui(&mut self, ui: &mut egui::Ui, panel: Panel) {
        match panel {
            Panel::Hierarchy => self.hierarchy_panel(ui),
            Panel::Scene => self.scene_panel(ui),
            Panel::Game => self.game_panel(ui),
            Panel::Preview => self.preview_panel(ui),
            Panel::Inspector => self.inspector_panel(ui),
            Panel::History => self.history_panel(ui),
            Panel::Project => self.project_panel(ui),
            Panel::Console => self.console_panel(ui),
            Panel::Data => self.data_panel(ui),
            Panel::Script => self.script_panel(ui),
            Panel::Dialogue => self.dialogue_panel(ui),
        }
    }
}
