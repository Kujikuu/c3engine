//! The editor shell, laid out like Unity's: a menu bar, a toolbar (tools · Play/Pause/Step ·
//! layout), and dockable panels (Hierarchy, Scene, Game, Inspector, Undo History, Project,
//! Console). The layout is saved between sessions.

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
    Inspector,
    History,
    Project,
    Console,
}

impl Panel {
    pub const ALL: [Panel; 7] = [
        Panel::Scene,
        Panel::Game,
        Panel::Hierarchy,
        Panel::Inspector,
        Panel::Project,
        Panel::Console,
        Panel::History,
    ];

    pub fn icon(self) -> &'static str {
        match self {
            Panel::Hierarchy => icons::TREE_STRUCTURE,
            Panel::Scene => icons::MAP_TRIFOLD,
            Panel::Game => icons::GAME_CONTROLLER,
            Panel::Inspector => icons::SLIDERS,
            Panel::History => icons::CLOCK_COUNTER_CLOCKWISE,
            Panel::Project => icons::FOLDER_OPEN,
            Panel::Console => icons::TERMINAL,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Panel::Hierarchy => "Hierarchy",
            Panel::Scene => "Scene",
            Panel::Game => "Game",
            Panel::Inspector => "Inspector",
            Panel::History => "Undo History",
            Panel::Project => "Project",
            Panel::Console => "Console",
        }
    }

    /// Panels a reopened panel joins (its usual neighbours).
    fn neighbours(self) -> &'static [Panel] {
        match self {
            Panel::Scene | Panel::Game => &[Panel::Scene, Panel::Game],
            Panel::Inspector | Panel::History => &[Panel::Inspector, Panel::History],
            Panel::Project | Panel::Console => &[Panel::Project, Panel::Console],
            Panel::Hierarchy => &[Panel::Hierarchy],
        }
    }
}

/// Unity's default arrangement: Hierarchy left, Scene/Game centre, Inspector right,
/// Project/Console along the bottom.
pub(crate) fn default_layout() -> Dock<Panel> {
    Dock::new(|group| {
        let left = group(vec![Panel::Hierarchy]);
        let centre = group(vec![Panel::Scene, Panel::Game]);
        let bottom = group(vec![Panel::Project, Panel::Console]);
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
            let undo = self
                .doc
                .as_ref()
                .and_then(|d| d.undo_label())
                .map(str::to_owned);
            let redo = self
                .doc
                .as_ref()
                .and_then(|d| d.redo_label())
                .map(str::to_owned);
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
            let width = 3.0 * 30.0;
            ui.add_space((middle - ui.cursor().left() - width / 2.0).max(8.0));
            let playing = self.play.is_some();
            let play_hint = if playing {
                "Stop the game"
            } else {
                "Play this map where the Scene view is looking (opens the game window; \
                 in-editor play comes later)"
            };
            let play_icon = if playing { icons::STOP } else { icons::PLAY };
            ui.add_enabled_ui(self.project.is_some(), |ui| {
                if theme::icon_toggle(ui, playing, play_icon, play_hint).clicked() {
                    self.toggle_play();
                }
            });
            ui.add_enabled_ui(false, |ui| {
                let later = "Comes with in-editor play mode";
                theme::icon_toggle(ui, false, icons::PAUSE, later).on_disabled_hover_text(later);
                theme::icon_toggle(ui, false, icons::SKIP_FORWARD, later)
                    .on_disabled_hover_text(later);
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
    pub(crate) fn toggle_play(&mut self) {
        if let Some(mut child) = self.play.take() {
            // The game may already have been closed by the player.
            let _ = child.kill();
            self.notify("Stopped the game".into());
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
                self.play = Some(child);
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
        if let Some(child) = &mut self.play
            && !matches!(child.try_wait(), Ok(None))
        {
            self.play = None;
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
            Panel::Scene if self.is_dirty() => "Scene*",
            _ => panel.name(),
        };
        theme::icon_text(panel.icon(), name)
    }

    fn ui(&mut self, ui: &mut egui::Ui, panel: Panel) {
        match panel {
            Panel::Hierarchy => self.hierarchy_panel(ui),
            Panel::Scene => self.scene_panel(ui),
            Panel::Game => self.game_panel(ui),
            Panel::Inspector => self.inspector_panel(ui),
            Panel::History => self.history_panel(ui),
            Panel::Project => self.project_panel(ui),
            Panel::Console => self.console_panel(ui),
        }
    }
}
