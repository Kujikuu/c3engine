//! The animation set editor: a project's `.c3char`, edited beside its character on the
//! Preview stage. The model and look it dresses, what each action plays and the models held
//! at its parts — each change one undo step, written to the file at once and shown again.

use crate::app::{Focus, Studio};
use crate::icons;
use crate::theme;
use crate::value_form::{Touch, comment_header, field, grid, models_in, pick, sounds_in};
use c3_edit::ValueDoc;
use c3_play::native_character::{
    self, ACTS, ActionFile, AttachmentFile, CharacterFile, Role, act_name,
};
use c3_play::{Look, LookKind};
use eframe::egui::{self, DragValue, RichText};

/// The animation set open for editing.
pub(crate) struct CharacterEditor {
    /// Its asset path.
    pub path: String,
    doc: ValueDoc<CharacterFile>,
    /// The file's opening comment lines, written back above the settings.
    header: String,
    /// The project's models and sounds, to pick from.
    models: Vec<String>,
    sounds: Vec<String>,
}

/// What the set's model offers: its animations and its parts (to hold models at).
#[derive(Default)]
struct ModelInfo {
    animations: Vec<String>,
    parts: Vec<String>,
}

impl Studio {
    /// Opens animation set `path` for editing and puts its character on the stage.
    pub(crate) fn open_character(&mut self, path: &str) {
        let Some(game) = &self.game else { return };
        let Some(native) = game.native() else { return };
        let Some(text) = native.read(path).and_then(|b| String::from_utf8(b).ok()) else {
            self.error = Some(format!("Can't read {path}"));
            return;
        };
        match native_character::parse(&text) {
            Ok(file) => {
                let dir = self.project.as_ref().map(|p| p.assets_dir());
                let (models, sounds) = match &dir {
                    Some(dir) => (models_in(dir), sounds_in(dir)),
                    None => Default::default(),
                };
                self.effect_editor = None;
                self.preview_look = Some(look_of(&file));
                self.character_editor = Some(CharacterEditor {
                    path: path.to_owned(),
                    doc: ValueDoc::new(file),
                    header: comment_header(&text),
                    models,
                    sounds,
                });
            }
            Err(e) => self.error = Some(format!("Can't edit {path}: {e}")),
        }
    }

    /// The editor's sections in the Preview panel's column; returns whether anything changed.
    pub(crate) fn character_editor_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(editor) = &self.character_editor else {
            return false;
        };
        let info = self.model_info(&editor.doc.value().model);
        let mut file = editor.doc.value().clone();
        let mut touch = Touch::default();
        let mut label = "Edit the animation set";
        theme::section(
            ui,
            &theme::icon_text(icons::PERSON_SIMPLE, "Animation set"),
            |ui| {
                ui.label(RichText::new(&editor.path).small().color(theme::TEXT_DIM));
                grid(ui, "character set", |ui| {
                    field(ui, "Model", |ui| {
                        if let Some(model) = pick(ui, "model", &file.model, &editor.models) {
                            file.model = model;
                            touch.click();
                            label = "Change the model";
                        }
                    });
                    field(ui, "Look", |ui| {
                        touch
                            .track(ui.add(DragValue::new(&mut file.look).range(0..=u32::MAX)))
                            .on_hover_text("Roles whose body is this look wear the set");
                    });
                    field(ui, "Worn by", |ui| {
                        if role_pick(ui, &mut file.role) {
                            touch.click();
                            label = "Change who wears it";
                        }
                    });
                    field(ui, "Frame time", |ui| {
                        touch.track(
                            ui.add(
                                DragValue::new(&mut file.frame_ms)
                                    .range(1..=2000)
                                    .suffix(" ms"),
                            ),
                        );
                    });
                });
                if info.animations.is_empty() && !file.model.is_empty() {
                    ui.label(
                        RichText::new("The model has no animations.")
                            .small()
                            .color(theme::WARNING),
                    );
                }
            },
        );
        theme::section(ui, &theme::icon_text(icons::PLAY, "Actions"), |ui| {
            ui.label(
                RichText::new("Actions left unset play the stand's animation.")
                    .small()
                    .color(theme::TEXT_DIM),
            );
            for act in ACTS {
                let name = act_name(act);
                let mut action = file.actions.get(name).cloned();
                let before = action.clone();
                action_row(ui, name, &mut action, &info, editor, &mut touch);
                if action != before {
                    match action {
                        Some(a) => file.actions.insert(name.to_owned(), a),
                        None => file.actions.remove(name),
                    };
                    label = "Edit an action";
                }
            }
        });
        theme::section(ui, &theme::icon_text(icons::SWORD, "Held models"), |ui| {
            let mut remove = None;
            for (i, held) in file.attachments.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    grid(ui, &format!("held {i}"), |ui| {
                        field(ui, "Model", |ui| {
                            if let Some(model) = pick(ui, "held model", &held.model, &editor.models)
                            {
                                held.model = model;
                                touch.click();
                            }
                        });
                        field(ui, "Held at", |ui| {
                            if let Some(part) = pick(ui, "held part", &held.socket, &info.parts) {
                                held.socket = part;
                                touch.click();
                            }
                        });
                    });
                    if ui
                        .button(icons::TRASH)
                        .on_hover_text("Stop holding it")
                        .clicked()
                    {
                        remove = Some(i);
                    }
                });
                ui.separator();
            }
            if let Some(i) = remove {
                file.attachments.remove(i);
                touch.click();
                label = "Remove a held model";
            }
            if ui
                .button(theme::icon_text(icons::PLUS, "Hold a model"))
                .on_hover_text("A model that follows one of the character's parts")
                .clicked()
            {
                file.attachments.push(AttachmentFile {
                    socket: info.parts.last().cloned().unwrap_or_default(),
                    model: editor.models.first().cloned().unwrap_or_default(),
                });
                touch.click();
                label = "Hold a model";
            }
        });
        self.apply_character_edit(file, &touch, label)
    }

    /// The animations and parts of model `path`, as the game reads it.
    fn model_info(&self, path: &str) -> ModelInfo {
        let Some(native) = self.game.as_ref().and_then(|g| g.native()) else {
            return ModelInfo::default();
        };
        let Ok(imported) = native.model(path) else {
            return ModelInfo::default();
        };
        let mut parts: Vec<String> = (imported.model.parts.iter())
            .map(|p| {
                let name = &p.mesh.name;
                name.rsplit_once('.')
                    .map_or(name.as_str(), |(mesh, _)| mesh)
                    .to_owned()
            })
            .collect();
        parts.dedup();
        ModelInfo {
            animations: imported.animations.iter().map(|a| a.name.clone()).collect(),
            parts,
        }
    }

    /// Records `file` as the set (one step per drag or field visit) and writes it.
    fn apply_character_edit(&mut self, file: CharacterFile, touch: &Touch, label: &str) -> bool {
        let Some(editor) = &mut self.character_editor else {
            return false;
        };
        if touch.started {
            editor.doc.begin_group(label);
            self.focus = Focus::Character;
        }
        let changed = touch.changed && editor.doc.set(file, label).changed;
        if touch.finished {
            editor.doc.end_group();
        }
        if changed {
            self.write_character();
        }
        changed
    }

    /// Writes the set's file and dresses the stage's character in it again.
    fn write_character(&mut self) {
        let (Some(editor), Some(project)) = (&self.character_editor, &self.project) else {
            return;
        };
        let file = project.assets_dir().join(&editor.path);
        let set = editor.doc.value();
        let text = format!("{}{}", editor.header, native_character::write(set));
        if let Err(e) = std::fs::write(&file, text) {
            self.error = Some(format!("Can't write {}: {e}", editor.path));
            return;
        }
        let look = look_of(set);
        let Some(stage) = &mut self.stage else { return };
        let reloaded = stage.reload_assets();
        stage.look = look;
        stage.rebuild();
        if let Err(e) = reloaded {
            self.notify(format!("The set doesn't show yet: {e}"));
        }
    }

    /// Undoes (or redoes) the last set edit; returns its label.
    pub(crate) fn character_step(&mut self, redo: bool) -> Option<String> {
        let editor = self.character_editor.as_mut()?;
        let label = if redo {
            editor.doc.redo()
        } else {
            editor.doc.undo()
        };
        if label.is_some() {
            self.write_character();
        }
        label
    }

    pub(crate) fn character_history(&self) -> Option<(String, Vec<c3_edit::HistoryEntry>)> {
        let editor = self.character_editor.as_ref()?;
        Some((
            format!("Animation set {}", editor.path),
            editor.doc.history(),
        ))
    }

    pub(crate) fn character_labels(&self) -> (Option<String>, Option<String>) {
        let doc = self.character_editor.as_ref().map(|e| &e.doc);
        (
            doc.and_then(|d| d.undo_label()).map(str::to_owned),
            doc.and_then(|d| d.redo_label()).map(str::to_owned),
        )
    }
}

/// The look a role wearing `set` has, to show it on the stage.
fn look_of(set: &CharacterFile) -> Look {
    let kind = match set.role {
        Some(Role::Monster) => LookKind::Monster,
        Some(Role::Npc) => LookKind::Npc,
        Some(Role::Player) | None => LookKind::Player,
    };
    Look {
        kind,
        body: set.look,
        ..Look::default()
    }
}

/// The roles a set dresses: any, or one kind; returns whether it changed.
fn role_pick(ui: &mut egui::Ui, role: &mut Option<Role>) -> bool {
    let shown = (Role::ALL.iter())
        .find(|(r, _)| Some(*r) == *role)
        .map_or("Every role", |(_, name)| name);
    let mut picked = None;
    egui::ComboBox::from_id_salt("set role")
        .selected_text(shown)
        .width(170.0)
        .show_ui(ui, |ui| {
            if ui.selectable_label(role.is_none(), "Every role").clicked() {
                picked = Some(None);
            }
            for (r, name) in Role::ALL {
                if ui.selectable_label(*role == Some(r), name).clicked() {
                    picked = Some(Some(r));
                }
            }
        });
    match picked {
        Some(p) if p != *role => {
            *role = p;
            true
        }
        _ => false,
    }
}

/// One action: the animation it plays (or none: the stand's), its frame time and sound.
fn action_row(
    ui: &mut egui::Ui,
    name: &str,
    action: &mut Option<ActionFile>,
    info: &ModelInfo,
    editor: &CharacterEditor,
    touch: &mut Touch,
) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [64.0, 18.0],
            egui::Label::new(RichText::new(name).color(theme::TEXT_DIM)),
        );
        let current = action.as_ref().map_or("", |a| a.animation.as_str());
        if let Some(animation) = pick(ui, name, current, &info.animations) {
            action.get_or_insert_with(ActionFile::default).animation = animation;
            touch.click();
        }
        if action.is_some()
            && ui
                .small_button(icons::X)
                .on_hover_text("Play the stand's animation instead")
                .clicked()
        {
            *action = None;
            touch.click();
        }
    });
    let Some(a) = action else { return };
    ui.indent(name, |ui| {
        ui.horizontal(|ui| {
            let mut own_time = a.frame_ms.is_some();
            if ui
                .checkbox(&mut own_time, "Own frame time")
                .on_hover_text("Else the set's frame time")
                .changed()
            {
                a.frame_ms = own_time.then_some(33);
                touch.click();
            }
            if let Some(ms) = &mut a.frame_ms {
                touch.track(ui.add(DragValue::new(ms).range(1..=2000).suffix(" ms")));
            }
        });
        ui.horizontal(|ui| {
            ui.label(theme::icon_text(icons::SPEAKER_HIGH, ""));
            if let Some(sound) = pick(ui, &format!("{name} sound"), &a.sound, &editor.sounds) {
                a.sound = sound;
                touch.click();
            }
            if !a.sound.is_empty() && ui.small_button(icons::X).on_hover_text("Silence").clicked() {
                a.sound.clear();
                touch.click();
            }
        });
    });
}
