//! Studio panels: toolbar, tool palette, map list, inspector, status bar, and prompts.

use crate::app::{Pending, Studio};
use crate::editor::{MAX_BRUSH, Tool};
use crate::map_view::colors;
use crate::theme;
use eframe::egui::{self, Color32, Key, RichText};

impl Studio {
    pub(crate) fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.label(
                RichText::new("C3 Studio")
                    .strong()
                    .size(16.0)
                    .color(theme::ACCENT),
            );
            ui.separator();

            let has_doc = self.doc.is_some();
            let (can_undo, can_redo) = self
                .doc
                .as_ref()
                .map_or((false, false), |d| (d.can_undo(), d.can_redo()));
            if ui
                .add_enabled(can_undo, egui::Button::new("↶ Undo"))
                .on_hover_text("Undo (Ctrl+Z)")
                .clicked()
            {
                self.undo();
            }
            if ui
                .add_enabled(can_redo, egui::Button::new("↷ Redo"))
                .on_hover_text("Redo (Ctrl+Y)")
                .clicked()
            {
                self.redo();
            }
            let dirty = self.is_dirty();
            let save = egui::Button::new(RichText::new("💾 Save").color(if dirty {
                Color32::BLACK
            } else {
                Color32::GRAY
            }))
            .fill(if dirty {
                theme::ACCENT
            } else {
                Color32::TRANSPARENT
            });
            let hint = if dirty {
                "Save map to the client folder (Ctrl+S)"
            } else {
                "No unsaved changes"
            };
            if ui
                .add_enabled(has_doc && dirty, save)
                .on_hover_text(hint)
                .on_disabled_hover_text(hint)
                .clicked()
            {
                self.save();
            }

            ui.separator();
            ui.label(RichText::new("Show").weak());
            let o = &mut self.overlays;
            theme::toggle_chip(ui, &mut o.grid, "Grid", Color32::from_gray(200), "G");
            theme::toggle_chip(ui, &mut o.blocked, "Blocked", colors::BLOCKED, "B");
            theme::toggle_chip(ui, &mut o.portals, "Portals", colors::PORTAL, "P");
            theme::toggle_chip(ui, &mut o.objects, "Objects", colors::COVER, "O");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(has_doc, egui::Button::new("Fit"))
                    .on_hover_text("Fit map to view (F)")
                    .clicked()
                {
                    self.fit_pending = true;
                }
                if ui
                    .add_enabled(has_doc, egui::Button::new("+"))
                    .on_hover_text("Zoom in (+ or scroll)")
                    .clicked()
                {
                    self.cam.zoom_by(1.25);
                }
                let zoom = RichText::new(format!("{:.0}%", self.cam.zoom * 100.0)).monospace();
                ui.add_sized([52.0, 20.0], egui::Label::new(zoom));
                if ui
                    .add_enabled(has_doc, egui::Button::new("−"))
                    .on_hover_text("Zoom out (- or scroll)")
                    .clicked()
                {
                    self.cam.zoom_by(0.8);
                }
            });
        });
    }

    /// Floating tool palette in the viewport's top-left corner.
    pub(crate) fn tool_palette(&mut self, ctx: &egui::Context, viewport: egui::Rect) {
        egui::Area::new(egui::Id::unique("tool_palette"))
            .fixed_pos(viewport.min + egui::vec2(12.0, 12.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for tool in Tool::ALL {
                        let selected = self.editor.tool == tool;
                        let text = RichText::new(format!("{}  {}", tool.icon(), tool.label()));
                        let r = ui
                            .add_sized([118.0, 26.0], egui::Button::selectable(selected, text))
                            .on_hover_text(format!(
                                "{} ({})\n{}",
                                tool.label(),
                                tool.key(),
                                tool.hint()
                            ));
                        if r.clicked() {
                            self.editor.tool = tool;
                        }
                    }
                    if self.editor.tool.uses_brush() {
                        ui.separator();
                        ui.label(RichText::new("Brush size").weak().small());
                        let mut size = self.editor.brush * 2 + 1;
                        let slider = egui::Slider::new(&mut size, 1..=MAX_BRUSH * 2 + 1)
                            .step_by(2.0)
                            .suffix(" cells");
                        if ui.add(slider).on_hover_text("[ and ] to change").changed() {
                            self.editor.brush = (size - 1) / 2;
                        }
                    }
                });
            });
    }

    pub(crate) fn map_list(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.heading("Maps");
            ui.label(RichText::new(self.maps.len().to_string()).weak());
        });
        ui.add_space(4.0);
        let search = ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .hint_text("Search maps…  (Ctrl+F)")
                .desired_width(f32::INFINITY),
        );
        if ui.input(|i| i.modifiers.command && i.key_pressed(Key::F)) {
            search.request_focus();
        }
        let list = self.filtered_maps();
        if search.has_focus()
            && ui.input(|i| i.key_pressed(Key::Enter))
            && let Some(first) = list.first()
        {
            self.open_map(first);
        }
        ui.add_space(4.0);
        if self.client.is_none() {
            ui.label(RichText::new("Set a valid client folder in the inspector.").weak());
            return;
        }
        if list.is_empty() {
            ui.label(RichText::new(format!("No maps match “{}”", self.filter)).weak());
            return;
        }
        let mut open = None;
        let footer = 22.0;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .max_height(ui.available_height() - footer)
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                    for m in &list {
                        let doc = self.doc.as_ref().filter(|d| &d.path == m);
                        let name = m.rsplit('/').next().unwrap_or(m);
                        let name = name
                            .strip_suffix(".DMap")
                            .or_else(|| name.strip_suffix(".dmap"))
                            .unwrap_or(name);
                        let label = match doc {
                            Some(d) if d.is_dirty() => format!("● {name}"),
                            _ => name.to_owned(),
                        };
                        let r = ui.add(
                            egui::Button::selectable(doc.is_some(), label)
                                .min_size(egui::vec2(0.0, 22.0)),
                        );
                        if doc.is_some() && self.fit_pending {
                            r.scroll_to_me(Some(egui::Align::Center));
                        }
                        if r.clicked() {
                            open = Some(m.clone());
                        }
                    }
                });
            });
        if let Some(m) = open {
            self.open_map(&m);
        }
        ui.label(
            RichText::new("PgUp / PgDn: previous / next map")
                .weak()
                .small(),
        );
    }

    pub(crate) fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                if let Some(doc) = &self.doc {
                    theme::section(ui, "Map", |ui| {
                        theme::kv(ui, "Name", doc.name());
                        theme::kv(
                            ui,
                            "Cells",
                            &format!("{} × {}", doc.scene.grid.width, doc.scene.grid.height),
                        );
                        if let Some(g) = &doc.scene.ground {
                            theme::kv(ui, "Ground", &format!("{} × {} tiles", g.width, g.height));
                            theme::kv(ui, "Ground file", &g.source);
                            theme::kv(ui, "Tile set", &g.tileset.source);
                        }
                        let pct = doc.scene.grid.walkable_ratio() * 100.0;
                        theme::kv(ui, "Walkable", &format!("{pct:.1}%"));
                        theme::kv(ui, "Objects", &doc.scene.entities.len().to_string());
                    });

                    if !doc.warnings.is_empty() {
                        theme::section(
                            ui,
                            &format!("⚠ Warnings ({})", doc.warnings.len()),
                            |ui| {
                                for w in &doc.warnings {
                                    ui.colored_label(theme::WARNING, w);
                                }
                            },
                        );
                    }

                    theme::section(ui, "Hovered cell", |ui| {
                        match self.hover.and_then(|c| doc.cell(c).map(|i| (c, i))) {
                            Some(((x, y), c)) => {
                                theme::kv(ui, "Position", &format!("{x}, {y}"));
                                let (text, color) = if !c.is_blocked() {
                                    ("Walkable", colors::COVER)
                                } else {
                                    ("Blocked", theme::DANGER)
                                };
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Access").weak());
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| ui.colored_label(color, text),
                                    );
                                });
                                theme::kv(ui, "Surface", &c.surface.to_string());
                                theme::kv(ui, "Height", &c.height.to_string());
                            }
                            None => {
                                ui.label(RichText::new("Hover the map to inspect a cell.").weak());
                            }
                        }
                    });

                    let mut jump = None;
                    theme::section(
                        ui,
                        &format!("Portals ({})", doc.scene.portals.len()),
                        |ui| {
                            if doc.scene.portals.is_empty() {
                                ui.label(
                                    RichText::new(
                                        "No portals. Use the Portal tool (4) to add one.",
                                    )
                                    .weak(),
                                );
                            }
                            ui.with_layout(
                                egui::Layout::top_down_justified(egui::Align::LEFT),
                                |ui| {
                                    for p in &doc.scene.portals {
                                        let label = format!(
                                            "Portal {}  ·  cell {}, {}",
                                            p.id, p.cell.0, p.cell.1
                                        );
                                        if ui
                                            .add(egui::Button::new(label).frame(false))
                                            .on_hover_text("Click to jump here")
                                            .clicked()
                                        {
                                            jump = Some(doc.cell_to_world(p.cell));
                                        }
                                    }
                                },
                            );
                        },
                    );
                    if let Some(p) = jump {
                        self.cam.center = p;
                        self.cam.zoom = self.cam.zoom.max(0.75);
                    }

                    theme::section(ui, "Legend", |ui| {
                        theme::legend(ui, colors::BLOCKED, "Blocked cell");
                        theme::legend(ui, colors::PORTAL, "Portal");
                        theme::legend(ui, colors::COVER, "Cover object (trees, buildings)");
                        theme::legend(ui, colors::TERRAIN, "Terrain object");
                        theme::legend(ui, colors::SOUND, "Sound source");
                        theme::legend(ui, colors::EFFECT, "Effect");
                    });
                }

                theme::section(ui, "Client", |ui| {
                    ui.label(RichText::new("Folder").weak());
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.client_dir)
                            .desired_width(f32::INFINITY),
                    );
                    if r.lost_focus() {
                        self.open_client();
                    }
                    if self.client.is_none() {
                        ui.colored_label(theme::DANGER, "Not loaded");
                    } else {
                        ui.label(RichText::new(format!("{} maps found", self.maps.len())).weak());
                    }
                });
            });
    }

    pub(crate) fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            if let Some(e) = &self.error {
                ui.colored_label(theme::DANGER, format!("⚠ {e}"));
            } else if let Some(n) = self.active_notice() {
                ui.colored_label(colors::COVER, format!("✔ {n}"));
            } else if let Some(doc) = &self.doc {
                ui.label(&doc.path);
                ui.separator();
                if self.streaming {
                    ui.add(egui::Spinner::new().size(12.0));
                    ui.label(format!(
                        "Loading tiles {}/{}",
                        doc.loaded_tiles(),
                        doc.used_tiles()
                    ));
                } else {
                    ui.label(RichText::new(format!("{} tiles loaded", doc.loaded_tiles())).weak());
                }
                if let Some((x, y)) = self.hover {
                    ui.separator();
                    ui.label(RichText::new(format!("Cell {x}, {y}")).monospace());
                }
            } else {
                ui.label(RichText::new("No map open").weak());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tool = self.editor.tool;
                ui.label(
                    RichText::new(format!(
                        "{}  ·  Right-drag pans · Scroll zooms",
                        tool.hint()
                    ))
                    .weak()
                    .small(),
                );
                ui.label(
                    RichText::new(format!("{} {}", tool.icon(), tool.label()))
                        .color(theme::ACCENT)
                        .small(),
                );
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
            Pending::Quit => "quitting",
        };
        let mut choice: Option<Option<bool>> = None; // Some(Some(save)) or Some(None) = cancel
        let modal = egui::Modal::new(egui::Id::unique("unsaved_prompt")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading("Unsaved changes");
            ui.add_space(4.0);
            ui.label(format!("Save changes to {name} before {action}?"));
            ui.label(
                RichText::new("A backup of the original is kept as .bak on first save.")
                    .weak()
                    .small(),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let save = egui::Button::new(RichText::new("Save").color(Color32::BLACK))
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
