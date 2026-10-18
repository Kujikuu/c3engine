//! Models: open `3dobj.ini` models, preview them in the Scene view with the engine renderer,
//! and scrub their animation in the Inspector.

use crate::app::Studio;
use crate::icons;
use crate::model_view::{self, ModelAnimation, ModelDoc, OrbitCamera};
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
        if self.model.as_ref().is_some_and(|m| m.entry == *entry) {
            return;
        }
        let doc = if is_native_model(&entry.path) {
            self.native_model(entry)
        } else {
            let Some(client) = &self.client else { return };
            client.import_model(&entry.path).map(|model| {
                let texture = client.texture_for(entry.id).map(str::to_owned);
                ModelDoc::new(entry.clone(), model, texture)
            })
        };
        match doc {
            Ok(doc) => {
                if let (Some(old), Some(gpu)) = (&mut self.model, &mut self.gpu) {
                    old.release(gpu);
                }
                self.model = Some(doc);
                self.orbit = OrbitCamera::default();
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// A glTF model of the project's assets.
    fn native_model(&self, entry: &ModelEntry) -> Result<ModelDoc, String> {
        let native = self
            .game
            .as_ref()
            .and_then(|g| g.native())
            .ok_or("the project's assets aren't open")?;
        let imported = native.model(&entry.path)?;
        let animations = imported
            .animations
            .iter()
            .map(|a| ModelAnimation {
                name: a.name.clone(),
                clips: a.clips.clone(),
            })
            .collect();
        Ok(ModelDoc::new(entry.clone(), imported.model.clone(), None).native(animations))
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

    /// Hierarchy for an open model: its parts.
    pub(crate) fn model_hierarchy(&mut self, ui: &mut egui::Ui) {
        let Some(m) = &self.model else {
            theme::empty_state(
                ui,
                icons::TREE_STRUCTURE,
                "No model open",
                "Open a model from the Project panel.",
            );
            return;
        };
        let name = m.entry.path.rsplit('/').next().unwrap_or(&m.entry.path);
        ui.label(RichText::new(theme::icon_text(icons::CUBE, name)).strong());
        ui.indent("model parts", |ui| {
            for p in &m.model.parts {
                let icon = if p.visible {
                    icons::PERSON_SIMPLE
                } else {
                    icons::CIRCLE
                };
                ui.label(theme::icon_text(icon, &p.mesh.name))
                    .on_hover_text(if p.visible {
                        "Mesh part"
                    } else {
                        "Socket (attachment point, not drawn)"
                    });
            }
        });
    }

    pub(crate) fn model_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(m) = &mut self.model else {
            return;
        };
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                theme::section(ui, "Model", |ui| {
                    if m.entry.id != 0 {
                        theme::kv(ui, "Id", &m.entry.id.to_string());
                    }
                    theme::kv(ui, "File", &m.entry.path);
                    let texture = match &m.texture_path {
                        Some(t) => t.as_str(),
                        None if m.model.parts.iter().any(|p| !p.mesh.texture.is_empty()) => {
                            "each part's own"
                        }
                        None => "none (untextured)",
                    };
                    theme::kv(ui, "Texture", texture);
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
                    if !m.animations.is_empty() {
                        let mut pick = m.animation;
                        egui::ComboBox::from_label("animation")
                            .selected_text(&m.animations[m.animation].name)
                            .show_ui(ui, |ui| {
                                for (i, a) in m.animations.iter().enumerate() {
                                    ui.selectable_value(&mut pick, i, &a.name);
                                }
                            });
                        if pick != m.animation {
                            m.play_animation(pick);
                        }
                    }
                    let frames = m.frame_count();
                    if frames == 0 {
                        ui.label(RichText::new("No animation.").weak());
                        return;
                    }
                    ui.horizontal(|ui| {
                        let (icon, hint) = if m.playing {
                            (icons::PAUSE, "Pause (Space)")
                        } else {
                            (icons::PLAY, "Play (Space)")
                        };
                        if theme::icon_toggle(ui, m.playing, icon, hint).clicked() {
                            m.playing ^= true;
                        }
                        if theme::icon_toggle(ui, false, icons::SKIP_BACK, "Previous frame (←)")
                            .clicked()
                        {
                            m.step(-1);
                        }
                        if theme::icon_toggle(ui, false, icons::SKIP_FORWARD, "Next frame (→)")
                            .clicked()
                        {
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
        let (Some(game), Some(gpu), Some(doc)) = (&self.game, &mut self.gpu, &mut self.model)
        else {
            theme::empty_state(
                ui,
                icons::CUBE,
                "No model open",
                "Open one from Models in the Project panel, or press PgDn.",
            );
            return;
        };
        model_view::show(ui, game.assets(), gpu, doc, &mut self.orbit);
    }
}

/// Whether `path` is a glTF model (the project's own) rather than a TQ one.
pub(crate) fn is_native_model(path: &str) -> bool {
    matches!(c3_core::asset::extension(path).as_str(), "gltf" | "glb")
}
