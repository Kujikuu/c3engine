//! Models workspace: browse `3dobj.ini` models, preview them with the engine renderer,
//! and scrub their animation.

use crate::app::Studio;
use crate::model_view::{self, ModelDoc, OrbitCamera};
use crate::theme;
use crate::tq_plugin::ModelEntry;
use eframe::egui::{self, Key, RichText};

impl Studio {
    pub(crate) fn filtered_models(&self) -> Vec<ModelEntry> {
        let f = self.filter.to_ascii_lowercase();
        self.models
            .iter()
            .filter(|m| m.path.to_ascii_lowercase().contains(&f) || m.id.to_string().contains(&f))
            .cloned()
            .collect()
    }

    pub(crate) fn open_model(&mut self, entry: &ModelEntry) {
        let Some(client) = &self.client else { return };
        if self.model.as_ref().is_some_and(|m| m.entry == *entry) {
            return;
        }
        match client.import_model(&entry.path) {
            Ok(model) => {
                let texture = client.texture_for(entry.id).map(str::to_owned);
                if let (Some(old), Some(gpu)) = (&mut self.model, &mut self.gpu) {
                    old.release(gpu);
                }
                self.model = Some(ModelDoc::new(entry.clone(), model, texture));
                self.orbit = OrbitCamera::default();
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub(crate) fn model_shortcuts(&mut self, ctx: &egui::Context) {
        let (toggle, step, reset, page) = ctx.input(|i| {
            (
                i.key_pressed(Key::Space),
                i.key_pressed(Key::ArrowRight) as i32 - i.key_pressed(Key::ArrowLeft) as i32,
                i.key_pressed(Key::F),
                i.key_pressed(Key::PageDown) as i32 - i.key_pressed(Key::PageUp) as i32,
            )
        });
        if let Some(m) = &mut self.model {
            if toggle {
                m.playing ^= true;
            }
            if step != 0 {
                m.step(step);
            }
        }
        if reset {
            self.orbit = OrbitCamera::default();
        }
        if page != 0 {
            let list = self.filtered_models();
            if !list.is_empty() {
                let cur = self
                    .model
                    .as_ref()
                    .and_then(|m| list.iter().position(|e| *e == m.entry));
                let next = cur.map_or(0, |c| {
                    (c as i32 + page).rem_euclid(list.len() as i32) as usize
                });
                self.open_model(&list[next]);
            }
        }
    }

    pub(crate) fn model_list(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.heading("Models");
            ui.label(RichText::new(self.models.len().to_string()).weak());
        });
        ui.add_space(4.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .hint_text("Search id or path…")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(4.0);
        let list = self.filtered_models();
        if list.is_empty() {
            ui.label(RichText::new("No models match.").weak());
            return;
        }
        let mut open = None;
        let row_height = 22.0;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .max_height(ui.available_height() - 22.0)
            .show_rows(ui, row_height, list.len(), |ui, rows| {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                    for e in &list[rows] {
                        let selected = self.model.as_ref().is_some_and(|m| m.entry == *e);
                        let name = e.path.rsplit('/').next().unwrap_or(&e.path);
                        let r = ui
                            .add(
                                egui::Button::selectable(selected, format!("{}  {name}", e.id))
                                    .min_size(egui::vec2(0.0, row_height)),
                            )
                            .on_hover_text(&e.path);
                        if r.clicked() {
                            open = Some(e.clone());
                        }
                    }
                });
            });
        if let Some(e) = open {
            self.open_model(&e);
        }
        ui.label(
            RichText::new("PgUp / PgDn: previous / next model")
                .weak()
                .small(),
        );
    }

    pub(crate) fn model_inspector(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        let Some(m) = &mut self.model else {
            ui.label(RichText::new("Select a model to inspect it.").weak());
            return;
        };
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                theme::section(ui, "Model", |ui| {
                    theme::kv(ui, "Id", &m.entry.id.to_string());
                    theme::kv(ui, "File", &m.entry.path);
                    theme::kv(
                        ui,
                        "Texture",
                        m.texture_path.as_deref().unwrap_or("none (untextured)"),
                    );
                    let verts: usize = m.model.parts.iter().map(|p| p.mesh.vertices.len()).sum();
                    let tris: usize = m.model.parts.iter().map(|p| p.mesh.indices.len() / 3).sum();
                    theme::kv(
                        ui,
                        "Geometry",
                        &format!("{verts} vertices · {tris} triangles"),
                    );
                });
                theme::section(ui, &format!("Parts ({})", m.model.parts.len()), |ui| {
                    for p in &m.model.parts {
                        let anim = p.clip.as_ref().map_or("static".to_owned(), |c| {
                            format!("{} bones, {} frames", c.bone_count, c.frame_count)
                        });
                        theme::kv(ui, &p.mesh.name, &anim);
                    }
                });
                theme::section(ui, "Playback", |ui| {
                    let frames = m.frame_count();
                    if frames == 0 {
                        ui.label(RichText::new("No animation.").weak());
                        return;
                    }
                    ui.horizontal(|ui| {
                        let label = if m.playing { "⏸ Pause" } else { "▶ Play" };
                        if ui.button(label).on_hover_text("Space").clicked() {
                            m.playing ^= true;
                        }
                        if ui.button("⏮").on_hover_text("Previous frame (←)").clicked() {
                            m.step(-1);
                        }
                        if ui.button("⏭").on_hover_text("Next frame (→)").clicked() {
                            m.step(1);
                        }
                    });
                    // Whole frames: a fractional value would read as a user edit every repaint.
                    let mut frame = m.frame.floor() as u32;
                    if ui
                        .add(egui::Slider::new(&mut frame, 0..=frames - 1).text("frame"))
                        .changed()
                    {
                        m.playing = false;
                        m.frame = frame as f32;
                    }
                    ui.add(
                        egui::Slider::new(&mut m.speed, 0.1..=3.0)
                            .text("speed")
                            .suffix("×"),
                    );
                });
                theme::section(ui, "Camera", |ui| {
                    ui.label(RichText::new("Drag to orbit · Scroll to zoom · F to reset").weak());
                });
            });
    }

    pub(crate) fn model_viewport(&mut self, ui: &mut egui::Ui) {
        let (Some(client), Some(gpu), Some(doc)) = (&self.client, &mut self.gpu, &mut self.model)
        else {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.4);
                    ui.label(RichText::new("Select a model to preview").size(18.0));
                    ui.label(
                        RichText::new("Pick one from the list on the left, or press PgDn.").weak(),
                    );
                });
            });
            return;
        };
        model_view::show(ui, client, gpu, doc, &mut self.orbit);
    }
}
