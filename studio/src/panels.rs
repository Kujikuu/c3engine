//! Studio panels: toolbar, map list, inspector, status bar.

use crate::app::Studio;
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
            ui.label(RichText::new("Overlays").weak());
            let o = &mut self.overlays;
            theme::toggle_chip(ui, &mut o.grid, "Grid", Color32::from_gray(200), "G");
            theme::toggle_chip(ui, &mut o.blocked, "Blocked", colors::BLOCKED, "B");
            theme::toggle_chip(ui, &mut o.portals, "Portals", colors::PORTAL, "P");
            theme::toggle_chip(ui, &mut o.objects, "Objects", colors::COVER, "O");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let enabled = self.doc.is_some();
                if ui
                    .add_enabled(enabled, egui::Button::new("Fit"))
                    .on_hover_text("Fit map to view (F)")
                    .clicked()
                {
                    self.fit_pending = true;
                }
                if ui
                    .add_enabled(enabled, egui::Button::new("+"))
                    .on_hover_text("Zoom in (+ or scroll)")
                    .clicked()
                {
                    self.cam.zoom_by(1.25);
                }
                ui.add_sized(
                    [52.0, 20.0],
                    egui::Label::new(
                        RichText::new(format!("{:.0}%", self.cam.zoom * 100.0)).monospace(),
                    ),
                );
                if ui
                    .add_enabled(enabled, egui::Button::new("−"))
                    .on_hover_text("Zoom out (- or scroll)")
                    .clicked()
                {
                    self.cam.zoom_by(0.8);
                }
            });
        });
    }

    pub(crate) fn map_list(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.heading("Maps");
            ui.label(RichText::new(format!("{}", self.maps.len())).weak());
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
        if self.fs.is_none() {
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
                        let selected = self.doc.as_ref().is_some_and(|d| &d.path == m);
                        let name = m.rsplit('/').next().unwrap_or(m);
                        let name = name
                            .strip_suffix(".DMap")
                            .or_else(|| name.strip_suffix(".dmap"))
                            .unwrap_or(name);
                        let r = ui.add(
                            egui::Button::selectable(selected, name)
                                .min_size(egui::vec2(0.0, 22.0)),
                        );
                        if selected && self.fit_pending {
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
                            &format!("{} × {}", doc.dmap.width, doc.dmap.height),
                        );
                        theme::kv(
                            ui,
                            "Ground",
                            &format!("{} × {} tiles", doc.puzzle.width, doc.puzzle.height),
                        );
                        theme::kv(ui, "Puzzle", &doc.dmap.puzzle_file.as_str());
                        theme::kv(ui, "Tile set", &doc.puzzle.ani_file.as_str());
                        let walkable = doc.dmap.cells.iter().filter(|c| c.mask == 0).count();
                        theme::kv(
                            ui,
                            "Walkable",
                            &format!(
                                "{:.0}%",
                                walkable as f32 * 100.0 / doc.dmap.cells.len().max(1) as f32
                            ),
                        );
                        theme::kv(ui, "Objects", &doc.dmap.interactive.len().to_string());
                    });

                    theme::section(ui, "Hovered cell", |ui| {
                        match self.hover.and_then(|c| doc.cell(c).map(|i| (c, i))) {
                            Some(((x, y), c)) => {
                                theme::kv(ui, "Position", &format!("{x}, {y}"));
                                let (text, color) = if c.mask == 0 {
                                    ("Walkable", colors::COVER)
                                } else {
                                    ("Blocked", Color32::from_rgb(235, 90, 90))
                                };
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Access").weak());
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| ui.colored_label(color, text),
                                    );
                                });
                                theme::kv(ui, "Terrain", &c.terrain.to_string());
                                theme::kv(ui, "Altitude", &c.altitude.to_string());
                            }
                            None => {
                                ui.label(RichText::new("Hover the map to inspect a cell.").weak());
                            }
                        }
                    });

                    let mut jump = None;
                    theme::section(
                        ui,
                        &format!("Portals ({})", doc.dmap.passages.len()),
                        |ui| {
                            if doc.dmap.passages.is_empty() {
                                ui.label(RichText::new("This map has no portals.").weak());
                            }
                            ui.with_layout(
                                egui::Layout::top_down_justified(egui::Align::LEFT),
                                |ui| {
                                    for p in &doc.dmap.passages {
                                        let label =
                                            format!("Portal {}  ·  cell {}, {}", p.index, p.x, p.y);
                                        if ui
                                            .add(egui::Button::new(label).frame(false))
                                            .on_hover_text("Click to jump here")
                                            .clicked()
                                        {
                                            jump = Some(doc.cell_to_world(p.x, p.y));
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
                    if self.fs.is_none() {
                        ui.colored_label(Color32::from_rgb(235, 90, 90), "Not loaded");
                    } else {
                        ui.label(RichText::new(format!("{} maps found", self.maps.len())).weak());
                    }
                });
            });
    }

    pub(crate) fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            if let Some(e) = &self.error {
                ui.colored_label(Color32::from_rgb(235, 90, 90), format!("⚠ {e}"));
                return;
            }
            match &self.doc {
                Some(doc) => {
                    ui.label(&doc.path);
                    ui.separator();
                    if self.streaming {
                        ui.add(egui::Spinner::new().size(12.0));
                        ui.label(format!(
                            "Loading tiles {}/{}",
                            doc.loaded_tiles(),
                            doc.used_tiles
                        ));
                    } else {
                        ui.label(
                            RichText::new(format!("{} tiles loaded", doc.loaded_tiles())).weak(),
                        );
                    }
                    if let Some((x, y)) = self.hover.filter(|&c| doc.cell(c).is_some()) {
                        ui.separator();
                        ui.label(RichText::new(format!("Cell {x}, {y}")).monospace());
                    }
                }
                None => {
                    ui.label(RichText::new("No map open").weak());
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new("Drag to pan · Scroll to zoom · F fit · WASD move")
                        .weak()
                        .small(),
                );
            });
        });
    }
}
