//! The Studio's dockable panels (Hierarchy, Scene, Game, Inspector, Undo History, Project,
//! Console), the status bar and the unsaved-changes prompt.

use crate::app::{Pending, Studio, Workspace};
use crate::editor::{MAX_BRUSH, Tool};
use crate::icons;
use crate::map_view::{self, ViewMode, colors};
use crate::shell::Level;
use crate::theme;
use c3_scene::EntityKind;
use eframe::egui::{self, Color32, Key, RichText};

/// Where the camera should jump to, chosen in a list this frame.
type Jump = Option<egui::Pos2>;

impl Studio {
    // ---- Hierarchy ----

    pub(crate) fn hierarchy_panel(&mut self, ui: &mut egui::Ui) {
        if self.workspace == Workspace::Models {
            self.model_hierarchy(ui);
            return;
        }
        let Some(doc) = &self.doc else {
            theme::empty_state(
                ui,
                icons::TREE_STRUCTURE,
                "No map open",
                "Open a map from the Project panel.",
            );
            return;
        };
        let scene = doc.scene();
        let mut jump: Jump = None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let name = if doc.is_dirty() {
                    format!("{}*", doc.name())
                } else {
                    doc.name().to_owned()
                };
                ui.label(RichText::new(theme::icon_text(icons::MAP_TRIFOLD, &name)).strong());
                ui.indent("hierarchy", |ui| {
                    let portals: Vec<(String, egui::Pos2)> = scene
                        .portals
                        .iter()
                        .map(|p| {
                            let label = format!("Portal {}   {}, {}", p.id, p.cell.0, p.cell.1);
                            (label, doc.cell_to_world(p.cell))
                        })
                        .collect();
                    tree_group(ui, icons::DOOR_OPEN, "Portals", &portals, &mut jump);
                    for (icon, group, keep) in [
                        (icons::SPARKLE, "Effects", 0),
                        (icons::TREE, "Objects", 1),
                        (icons::SPEAKER_HIGH, "Sounds", 2),
                    ] {
                        let items: Vec<(String, egui::Pos2)> = scene
                            .entities
                            .iter()
                            .filter_map(|e| {
                                let (kind, name) = match &e.kind {
                                    EntityKind::Effect { name, .. } => (0, name.as_str()),
                                    EntityKind::Sprite { animation, .. } => (1, animation.as_str()),
                                    EntityKind::Prefab { asset } => (1, file_stem(asset)),
                                    EntityKind::Sound { asset, .. } => (2, file_stem(asset)),
                                };
                                let (x, y) = scene.entity_world(e);
                                (kind == keep).then(|| (name.to_owned(), egui::pos2(x, y)))
                            })
                            .collect();
                        tree_group(ui, icon, group, &items, &mut jump);
                    }
                    let layers: Vec<(String, egui::Pos2)> = scene
                        .layers
                        .iter()
                        .map(|l| {
                            let label = format!("Layer {} · scroll {}%", l.order, l.scroll_rate.0);
                            (label, egui::Pos2::ZERO)
                        })
                        .collect();
                    ui.add_enabled_ui(false, |ui| {
                        tree_group(ui, icons::STACK, "Layers", &layers, &mut None)
                    });
                });
            });
        if let Some(p) = jump {
            self.cam.center = p;
            self.cam.zoom = self.cam.zoom.max(0.75);
        }
    }

    // ---- Scene ----

    pub(crate) fn scene_panel(&mut self, ui: &mut egui::Ui) {
        if self.workspace == Workspace::Models {
            self.model_viewport(ui);
            return;
        }
        if self.doc.is_some() {
            self.scene_bar(ui);
            ui.add_space(2.0);
        }
        self.map_viewport(ui);
    }

    /// The Scene view's own toolbar: overlays, brush and zoom.
    fn scene_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let o = &mut self.overlays;
            theme::toggle_chip(ui, &mut o.grid, "Grid", Color32::from_gray(200), "G");
            theme::toggle_chip(ui, &mut o.blocked, "Blocked", colors::BLOCKED, "B");
            theme::toggle_chip(ui, &mut o.portals, "Portals", colors::PORTAL, "P");
            ui.separator();
            self.layers_menu(ui);
            if theme::icon_toggle(
                ui,
                self.animate,
                icons::FILM_STRIP,
                "Animate objects and effects",
            )
            .clicked()
            {
                self.animate ^= true;
            }
            if self.editor.tool.uses_brush() {
                ui.separator();
                ui.label(RichText::new("Brush").color(theme::TEXT_DIM));
                let mut size = self.editor.brush * 2 + 1;
                let slider = egui::Slider::new(&mut size, 1..=MAX_BRUSH * 2 + 1)
                    .step_by(2.0)
                    .suffix(" cells");
                if ui
                    .add(slider)
                    .on_hover_text("[ and ] change the brush")
                    .changed()
                {
                    self.editor.brush = (size - 1) / 2;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::icon_toggle(ui, false, icons::CORNERS_OUT, "Fit map to view (F)")
                    .clicked()
                {
                    self.fit_pending = true;
                }
                if theme::icon_toggle(
                    ui,
                    false,
                    icons::MAGNIFYING_GLASS_PLUS,
                    "Zoom in (+ or scroll)",
                )
                .clicked()
                {
                    self.cam.zoom_by(1.25);
                }
                let zoom = RichText::new(format!("{:.0}%", self.cam.zoom * 100.0)).monospace();
                ui.add_sized([46.0, 20.0], egui::Label::new(zoom));
                if theme::icon_toggle(
                    ui,
                    false,
                    icons::MAGNIFYING_GLASS_MINUS,
                    "Zoom out (- or scroll)",
                )
                .clicked()
                {
                    self.cam.zoom_by(0.8);
                }
            });
        });
    }

    /// What the Scene view draws: the scene's layers and the anchor markers.
    fn layers_menu(&mut self, ui: &mut egui::Ui) {
        let (l, o) = (&mut self.layers, &mut self.overlays);
        let hidden = [l.sky, l.ground, l.objects, l.effects, !o.markers]
            .iter()
            .filter(|on| !**on)
            .count();
        let title = match hidden {
            0 => theme::icon_text(icons::STACK, "Layers"),
            n => theme::icon_text(icons::STACK, &format!("Layers ({n} hidden)")),
        };
        ui.menu_button(title, |ui| {
            ui.label(RichText::new("Scene").color(theme::TEXT_DIM).small());
            for (on, label, key) in [
                (&mut l.sky, "Sky and water", ""),
                (&mut l.ground, "Ground", ""),
                (&mut l.objects, "Objects", "O"),
                (&mut l.effects, "Effects", "E"),
            ] {
                if ui
                    .add(egui::Button::selectable(*on, label).shortcut_text(key))
                    .clicked()
                {
                    *on ^= true;
                }
            }
            ui.separator();
            ui.label(RichText::new("Editor").color(theme::TEXT_DIM).small());
            let markers = egui::Button::selectable(o.markers, "Anchor markers").shortcut_text("M");
            if ui
                .add(markers)
                .on_hover_text("Dots where objects, sounds and effects are anchored")
                .clicked()
            {
                o.markers ^= true;
            }
        });
    }

    fn map_viewport(&mut self, ui: &mut egui::Ui) {
        let Some(gpu) = &mut self.gpu else {
            ui.colored_label(
                theme::DANGER,
                "GPU renderer unavailable (wgpu backend required).",
            );
            return;
        };
        let (Some(client), Some(doc), Some(scene_view)) =
            (&self.client, &mut self.doc, &mut self.scene_view)
        else {
            if self.maps.is_empty() {
                theme::empty_state(
                    ui,
                    icons::MAP_TRIFOLD,
                    "No maps in this project yet",
                    "Projects imported from a Conquer client list its maps in the Project panel.",
                );
            } else {
                theme::empty_state(
                    ui,
                    icons::MAP_TRIFOLD,
                    "No map open",
                    "Pick a map in the Project panel, or press PgDn.",
                );
            }
            return;
        };
        if self.fit_pending && self.cam.viewport.x > 1.0 {
            self.cam.fit(doc.bounds());
            self.fit_pending = false;
        }
        let tool = self.editor.tool;
        let mode = ViewMode {
            pan_with_primary: tool == Tool::Inspect,
            brush: tool.uses_brush().then(|| {
                let color = if tool == Tool::Block {
                    colors::BRUSH_BLOCK
                } else {
                    colors::BRUSH_CLEAR
                };
                (self.editor.brush, color)
            }),
            dragging_portal: self.editor.dragging_portal(),
            time_ms: self.scene_ms as u64,
        };
        scene_view.layers = self.layers;
        let r = map_view::show(
            ui,
            client,
            gpu,
            scene_view,
            doc,
            &mut self.cam,
            self.overlays,
            mode,
        );
        self.hover = r.pointer.cell;
        self.streaming = r.streaming;
        if self.animate {
            // Capped so a long pause doesn't jump animations ahead.
            self.scene_ms += f64::from(ui.input(|i| i.stable_dt).min(0.1)) * 1000.0;
            ui.ctx().request_repaint();
        }
        let errors = scene_view.library.take_errors();
        if let Some(msg) = self.editor.handle(doc, &r.pointer) {
            self.notify(msg);
        }
        for error in errors {
            self.log_error(error);
        }
        if self.fit_pending {
            ui.ctx().request_repaint();
        }
    }

    // ---- Game ----

    pub(crate) fn game_panel(&mut self, ui: &mut egui::Ui) {
        let playing = self.play.is_some();
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.3).max(8.0));
            ui.label(
                RichText::new(icons::GAME_CONTROLLER)
                    .size(40.0)
                    .color(Color32::from_gray(96)),
            );
            ui.add_space(4.0);
            if playing {
                ui.label(RichText::new("The game is running in its own window").size(15.0));
                ui.label(
                    RichText::new("Press Stop in the toolbar to end it.").color(theme::TEXT_DIM),
                );
            } else {
                ui.label(RichText::new("Press Play to try your map").size(15.0));
                ui.label(
                    RichText::new(
                        "The hero starts where the Scene view is looking. For now the game opens \
                         in its own window; playing inside this panel comes later.",
                    )
                    .color(theme::TEXT_DIM),
                );
            }
            ui.add_space(10.0);
            let (icon, label) = if playing {
                (icons::STOP, "Stop")
            } else {
                (icons::PLAY, "Play")
            };
            let button = egui::Button::new(
                RichText::new(theme::icon_text(icon, label)).color(Color32::WHITE),
            )
            .fill(theme::ACCENT)
            .min_size(egui::vec2(120.0, 30.0));
            if ui.add_enabled(self.project.is_some(), button).clicked() {
                self.toggle_play();
            }
        });
    }

    // ---- Inspector ----

    pub(crate) fn inspector_panel(&mut self, ui: &mut egui::Ui) {
        if self.inspect_asset && self.browser.selected.is_some() {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| self.asset_inspector(ui));
            return;
        }
        if self.workspace == Workspace::Models && self.model.is_some() {
            self.model_inspector(ui);
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                match &self.doc {
                    Some(_) => self.map_inspector(ui),
                    None => theme::empty_state(
                        ui,
                        icons::SLIDERS,
                        "Nothing selected",
                        "Open a map or a model to see its properties.",
                    ),
                }
                ui.add_space(6.0);
                self.project_section(ui);
            });
    }

    fn map_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(doc) = &self.doc else { return };
        let scene = doc.scene();
        theme::section(ui, "Map", |ui| {
            theme::kv(ui, "Name", doc.name());
            theme::kv(
                ui,
                "Cells",
                &format!("{} × {}", scene.grid.width, scene.grid.height),
            );
            if let Some(g) = &scene.ground {
                theme::kv(ui, "Ground", &format!("{} × {} tiles", g.width, g.height));
                theme::kv(ui, "Ground file", &g.source);
                theme::kv(ui, "Tile set", &g.tileset.source);
            }
            let pct = scene.grid.walkable_ratio() * 100.0;
            theme::kv(ui, "Walkable", &format!("{pct:.1}%"));
            theme::kv(ui, "Objects", &scene.entities.len().to_string());
            theme::kv(ui, "Portals", &scene.portals.len().to_string());
        });
        if !doc.warnings.is_empty() {
            let title = theme::icon_text(
                icons::WARNING,
                &format!("Warnings ({})", doc.warnings.len()),
            );
            theme::section(ui, &title, |ui| {
                for w in &doc.warnings {
                    ui.colored_label(theme::WARNING, w);
                }
            });
        }
        theme::section(ui, "Hovered cell", |ui| {
            match self.hover.and_then(|c| doc.cell(c).map(|i| (c, i))) {
                Some(((x, y), c)) => {
                    theme::kv(ui, "Position", &format!("{x}, {y}"));
                    let (text, color) = if c.is_blocked() {
                        ("Blocked", theme::DANGER)
                    } else {
                        ("Walkable", theme::SUCCESS)
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Access").color(theme::TEXT_DIM));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.colored_label(color, text)
                        });
                    });
                    theme::kv(ui, "Surface", &c.surface.to_string());
                    theme::kv(ui, "Height", &c.height.to_string());
                }
                None => {
                    ui.label(
                        RichText::new("Hover the map to inspect a cell.").color(theme::TEXT_DIM),
                    );
                }
            }
        });
        theme::section(ui, "Legend", |ui| {
            theme::legend(ui, colors::BLOCKED, "Blocked cell");
            theme::legend(ui, colors::PORTAL, "Portal");
            if self.overlays.markers {
                theme::legend(ui, colors::COVER, "Cover object (trees, buildings)");
                theme::legend(ui, colors::TERRAIN, "Terrain object");
                theme::legend(ui, colors::SOUND, "Sound source");
                theme::legend(ui, colors::EFFECT, "Effect");
            } else {
                ui.label(
                    RichText::new("Anchor markers (M) colour objects, sounds and effects.")
                        .color(theme::TEXT_DIM)
                        .small(),
                );
            }
        });
    }

    // ---- Undo History ----

    pub(crate) fn history_panel(&mut self, ui: &mut egui::Ui) {
        let Some(doc) = &self.doc else {
            theme::empty_state(
                ui,
                icons::CLOCK_COUNTER_CLOCKWISE,
                "No map open",
                "Edits you make to a map are listed here.",
            );
            return;
        };
        let entries = doc.history();
        if entries.is_empty() {
            theme::empty_state(
                ui,
                icons::CLOCK_COUNTER_CLOCKWISE,
                "No edits yet",
                "Every change appears here; click one to go back to it.",
            );
            return;
        }
        let done = entries.iter().filter(|e| e.done).count();
        let mut steps: i32 = 0; // > 0 undo that many, < 0 redo
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                    for (i, e) in entries.iter().enumerate() {
                        let text = if e.done {
                            RichText::new(&e.label)
                        } else {
                            RichText::new(&e.label)
                                .color(theme::TEXT_DIM)
                                .strikethrough()
                        };
                        let r = ui
                            .add(egui::Button::selectable(i + 1 == done, text))
                            .on_hover_text(if e.done {
                                "Click to undo back to this step"
                            } else {
                                "Undone · click to redo up to this step"
                            });
                        if r.clicked() {
                            steps = done as i32 - (i as i32 + 1);
                        }
                    }
                });
            });
        for _ in 0..steps.max(0) {
            self.undo();
        }
        for _ in 0..(-steps).max(0) {
            self.redo();
        }
    }

    // ---- Console ----

    pub(crate) fn console_panel(&mut self, ui: &mut egui::Ui) {
        let errors = self.log.iter().filter(|e| e.level == Level::Error).count();
        let infos = self.log.len() - errors;
        ui.horizontal(|ui| {
            if ui.button(theme::icon_text(icons::TRASH, "Clear")).clicked() {
                self.log.clear();
            }
            ui.separator();
            let info = RichText::new(theme::icon_text(icons::INFO, &infos.to_string()));
            ui.toggle_value(&mut self.console_infos, info)
                .on_hover_text("Show messages");
            let error = RichText::new(theme::icon_text(icons::X_CIRCLE, &errors.to_string()))
                .color(if errors > 0 {
                    theme::DANGER
                } else {
                    theme::TEXT_DIM
                });
            ui.toggle_value(&mut self.console_errors, error)
                .on_hover_text("Show errors");
        });
        ui.separator();
        if self.log.is_empty() {
            theme::empty_state(
                ui,
                icons::TERMINAL,
                "No messages",
                "What the Studio does is logged here.",
            );
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for entry in &self.log {
                    let shown = match entry.level {
                        Level::Info => self.console_infos,
                        Level::Error => self.console_errors,
                    };
                    if !shown {
                        continue;
                    }
                    let (icon, color) = match entry.level {
                        Level::Info => (icons::INFO, theme::TEXT_DIM),
                        Level::Error => (icons::X_CIRCLE, theme::DANGER),
                    };
                    let secs = entry.at as u32;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icon).color(color));
                        ui.label(
                            RichText::new(format!("[{:02}:{:02}]", secs / 60, secs % 60))
                                .monospace()
                                .color(theme::TEXT_DIM),
                        );
                        ui.label(&entry.text);
                    });
                }
            });
    }

    // ---- status bar ----

    pub(crate) fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            if let Some(e) = &self.error {
                ui.colored_label(theme::DANGER, theme::icon_text(icons::X_CIRCLE, e));
            } else if let Some(n) = self.active_notice() {
                ui.colored_label(theme::SUCCESS, theme::icon_text(icons::CHECK_CIRCLE, n));
            } else if self.project.is_none() {
                ui.label(RichText::new("No project open").color(theme::TEXT_DIM));
                return;
            } else if self.workspace == Workspace::Models {
                if let Some(m) = &self.model {
                    ui.label(&m.entry.path);
                }
            } else if let Some(doc) = &self.doc {
                ui.label(self.save_target().unwrap_or_else(|| doc.name().to_owned()));
                ui.separator();
                let tiles = self.scene_view.as_ref().map_or(0, |v| v.loaded_tiles());
                if self.streaming {
                    ui.add(egui::Spinner::new().size(12.0));
                    ui.label(format!("Loading art · tiles {tiles}/{}", doc.used_tiles()));
                } else {
                    ui.label(RichText::new(format!("{tiles} tiles loaded")).color(theme::TEXT_DIM));
                }
                if let Some(view) = &self.scene_view
                    && view.effect_count() > 0
                {
                    ui.separator();
                    ui.label(
                        RichText::new(format!("{} effects", view.effect_count()))
                            .color(theme::TEXT_DIM),
                    );
                }
                if let Some((x, y)) = self.hover {
                    ui.separator();
                    ui.label(RichText::new(format!("Cell {x}, {y}")).monospace());
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let hint = if self.workspace == Workspace::Models {
                    "Drag to orbit · Scroll zooms · Space play/pause · ← → step · F reset"
                        .to_owned()
                } else {
                    format!(
                        "{}  ·  Right-drag pans · Scroll zooms",
                        self.editor.tool.hint()
                    )
                };
                ui.label(RichText::new(hint).color(theme::TEXT_DIM).small());
            });
        });
    }

    /// Modal asking what to do with unsaved changes before switching maps or quitting.
    pub(crate) fn unsaved_prompt(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending.clone() else {
            return;
        };
        let name = self.doc.as_ref().map_or("", |d| d.name()).to_owned();
        let action = match pending {
            Pending::OpenMap(_) => "opening another map",
            Pending::OpenProject(_) => "opening another project",
            Pending::CloseProject => "closing the project",
            Pending::Quit => "quitting",
        };
        let mut choice: Option<Option<bool>> = None; // Some(Some(save)) or Some(None) = cancel
        let modal = egui::Modal::new(egui::Id::unique("unsaved_prompt")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading("Unsaved changes");
            ui.add_space(4.0);
            ui.label(format!("Save changes to {name} before {action}?"));
            ui.label(
                RichText::new(format!(
                    "Saving writes {}.",
                    self.save_target().unwrap_or_default()
                ))
                .color(theme::TEXT_DIM)
                .small(),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let save = egui::Button::new(RichText::new("Save").color(Color32::WHITE))
                    .fill(theme::ACCENT);
                if ui.add(save).clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                    choice = Some(Some(true));
                }
                if ui.button("Don't save").clicked() {
                    choice = Some(Some(false));
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(None);
                }
            });
        });
        if modal.should_close() && choice.is_none() {
            choice = Some(None);
        }
        match choice {
            Some(Some(save)) => self.resolve_pending(save, ctx),
            Some(None) => self.pending = None,
            None => {}
        }
    }
}

/// A collapsible Hierarchy group whose rows jump the camera when clicked.
fn tree_group(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    items: &[(String, egui::Pos2)],
    jump: &mut Jump,
) {
    let header = theme::icon_text(icon, &format!("{title} ({})", items.len()));
    egui::CollapsingHeader::new(header)
        .id_salt(title)
        .default_open(items.len() <= 12)
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                for (label, pos) in items {
                    if ui
                        .add(egui::Button::new(label).frame(false))
                        .on_hover_text("Click to show it in the Scene")
                        .clicked()
                    {
                        *jump = Some(*pos);
                    }
                }
            });
        });
}

/// `ani/tree.ani` → `tree`.
fn file_stem(path: &str) -> &str {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    file.rsplit_once('.').map_or(file, |(stem, _)| stem)
}
