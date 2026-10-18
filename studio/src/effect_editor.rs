//! The effect editor: a project's `.c3fx` effect, edited beside its preview. Timing and
//! layers — flipbooks (and their bursts), models and trails — each change one undo step,
//! written to the file at once (as assets are) and played again.

use crate::app::{Focus, Studio};
use crate::icons;
use crate::theme;
use crate::value_form::{Touch, comment_header, field, grid, models_in, pick};
use c3_edit::ValueDoc;
use c3_play::native_effect::{self, BlendMode, EffectFile, LayerFile, TrailFile};
use eframe::egui::{self, DragValue, RichText};

/// The effect open for editing.
pub(crate) struct EffectEditor {
    /// Its asset path.
    pub path: String,
    doc: ValueDoc<EffectFile>,
    /// The file's opening comment lines, written back above the settings.
    header: String,
    /// The project's images and models, to pick layers' art from.
    images: Vec<String>,
    models: Vec<String>,
}

/// What a layer is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LayerKind {
    Flipbook,
    Model,
    Trail,
}

impl LayerKind {
    const ALL: [(Self, &'static str); 3] = [
        (Self::Flipbook, "Flipbook"),
        (Self::Model, "Model"),
        (Self::Trail, "Trail"),
    ];

    fn of(layer: &LayerFile) -> Self {
        match (&layer.model, &layer.trail) {
            (Some(_), _) => Self::Model,
            (None, Some(_)) => Self::Trail,
            _ => Self::Flipbook,
        }
    }
}

impl Studio {
    /// Opens effect `path` for editing when it is one of the project's own (`.c3fx`).
    pub(crate) fn open_effect(&mut self, path: &str) {
        if self.effect_editor.as_ref().is_some_and(|e| e.path == path) {
            return;
        }
        let Some(game) = &self.game else { return };
        let Some(native) = game.native() else { return };
        let text = native.read(path).and_then(|b| String::from_utf8(b).ok());
        let Some(text) = text else {
            self.effect_editor = None;
            return;
        };
        match native_effect::parse(&text) {
            Ok(file) => {
                let dir = self.project.as_ref().map(|p| p.assets_dir());
                let (images, models) = match &dir {
                    Some(dir) => (crate::new_map::images_in(dir), models_in(dir)),
                    None => Default::default(),
                };
                let header = comment_header(&text);
                self.effect_editor = Some(EffectEditor {
                    path: path.to_owned(),
                    doc: ValueDoc::new(file),
                    header,
                    images,
                    models,
                });
            }
            Err(e) => self.error = Some(format!("Can't edit {path}: {e}")),
        }
    }

    /// The editor's sections in the Preview panel's column; returns whether anything changed.
    pub(crate) fn effect_editor_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(editor) = &self.effect_editor else {
            return false;
        };
        let mut file = editor.doc.value().clone();
        let mut touch = Touch::default();
        let mut label = "Edit the effect";
        theme::section(ui, &theme::icon_text(icons::SPARKLE, "Edit effect"), |ui| {
            ui.label(RichText::new(&editor.path).small().color(theme::TEXT_DIM));
            grid(ui, "effect timing", |ui| {
                field(ui, "Frame time", |ui| {
                    touch.track(
                        ui.add(
                            DragValue::new(&mut file.frame_ms)
                                .range(1..=2000)
                                .suffix(" ms"),
                        ),
                    );
                });
                field(ui, "Plays", |ui| {
                    touch
                        .track(ui.add(DragValue::new(&mut file.loops).range(0..=99)))
                        .on_hover_text("Times it plays; 0 plays forever");
                });
                field(ui, "Delay", |ui| {
                    touch.track(
                        ui.add(
                            DragValue::new(&mut file.delay_ms)
                                .range(0..=10_000)
                                .suffix(" ms"),
                        ),
                    );
                });
                field(ui, "Gap", |ui| {
                    touch
                        .track(
                            ui.add(
                                DragValue::new(&mut file.loop_gap_ms)
                                    .range(0..=10_000)
                                    .suffix(" ms"),
                            ),
                        )
                        .on_hover_text("Pause between plays");
                });
            });
        });
        let mut remove = None;
        let mut swap = None;
        for (i, layer) in file.layers.iter_mut().enumerate() {
            let title = format!("Layer {}", i + 1);
            theme::section(ui, &title, |ui| {
                ui.horizontal(|ui| {
                    for (kind, name) in LayerKind::ALL {
                        if ui
                            .selectable_label(LayerKind::of(layer) == kind, name)
                            .clicked()
                            && LayerKind::of(layer) != kind
                        {
                            set_kind(layer, kind);
                            touch.click();
                            label = "Change a layer's kind";
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(icons::TRASH)
                            .on_hover_text("Remove the layer")
                            .clicked()
                        {
                            remove = Some(i);
                        }
                        if i > 0
                            && ui
                                .button(icons::ARROW_UP)
                                .on_hover_text("Draw it earlier")
                                .clicked()
                        {
                            swap = Some(i);
                        }
                    });
                });
                layer_fields(ui, layer, editor, &mut touch, game_animations(self, layer));
            });
        }
        // New layers start from the last layer's sheet (else the first image).
        let sheet = (file.layers.iter().rev())
            .find_map(|l| l.sheet.clone())
            .or_else(|| editor.images.first().cloned());
        ui.horizontal(|ui| {
            if ui
                .button(theme::icon_text(icons::PLUS, "Flipbook layer"))
                .on_hover_text("A sheet's animation on a square facing the camera")
                .clicked()
            {
                file.layers.push(LayerFile {
                    sheet: sheet.clone(),
                    ..LayerFile::default()
                });
                touch.click();
                label = "Add a layer";
            }
            if ui.button(theme::icon_text(icons::PLUS, "Trail")).clicked() {
                file.layers.push(LayerFile {
                    sheet: sheet.clone(),
                    trail: Some(TrailFile::default()),
                    ..LayerFile::default()
                });
                touch.click();
                label = "Add a trail";
            }
        });
        if let Some(i) = remove {
            file.layers.remove(i);
            touch.click();
            label = "Remove a layer";
        }
        if let Some(i) = swap {
            file.layers.swap(i, i - 1);
            touch.click();
            label = "Reorder layers";
        }
        self.apply_effect_edit(file, &touch, label)
    }

    /// Records `file` as the effect (one step per drag or field visit) and writes it.
    fn apply_effect_edit(&mut self, file: EffectFile, touch: &Touch, label: &str) -> bool {
        let Some(editor) = &mut self.effect_editor else {
            return false;
        };
        if touch.started {
            editor.doc.begin_group(label);
            self.focus = Focus::Effect;
        }
        let changed = touch.changed && editor.doc.set(file, label).changed;
        if touch.finished {
            editor.doc.end_group();
        }
        if changed {
            self.write_effect();
        }
        changed
    }

    /// Writes the effect file and plays it again.
    fn write_effect(&mut self) {
        let (Some(editor), Some(project)) = (&self.effect_editor, &self.project) else {
            return;
        };
        let file = project.assets_dir().join(&editor.path);
        let text = format!(
            "{}{}",
            editor.header,
            native_effect::write(editor.doc.value())
        );
        if let Err(e) = std::fs::write(&file, text) {
            self.error = Some(format!("Can't write {}: {e}", editor.path));
            return;
        }
        if let Some(stage) = &mut self.stage
            && let Err(e) = stage.reload_assets()
        {
            self.notify(format!("The effect doesn't play yet: {e}"));
        }
    }

    /// Undoes (or redoes) the last effect edit; returns its label.
    pub(crate) fn effect_step(&mut self, redo: bool) -> Option<String> {
        let editor = self.effect_editor.as_mut()?;
        let label = if redo {
            editor.doc.redo()
        } else {
            editor.doc.undo()
        };
        if label.is_some() {
            self.write_effect();
        }
        label
    }

    pub(crate) fn effect_history(&self) -> Option<(String, Vec<c3_edit::HistoryEntry>)> {
        let editor = self.effect_editor.as_ref()?;
        Some((format!("Effect {}", editor.path), editor.doc.history()))
    }

    pub(crate) fn effect_labels(&self) -> (Option<String>, Option<String>) {
        let doc = self.effect_editor.as_ref().map(|e| &e.doc);
        (
            doc.and_then(|d| d.undo_label()).map(str::to_owned),
            doc.and_then(|d| d.redo_label()).map(str::to_owned),
        )
    }
}

/// The animations of the layer's sheet (what the game reads from its import settings).
fn game_animations(studio: &Studio, layer: &LayerFile) -> Vec<String> {
    let (Some(game), Some(sheet)) = (&studio.game, &layer.sheet) else {
        return Vec::new();
    };
    (game.assets().sprite_set(sheet))
        .map(|s| s.animations.keys().cloned().collect())
        .unwrap_or_default()
}

/// Turns `layer` into `kind`, keeping what still applies.
fn set_kind(layer: &mut LayerFile, kind: LayerKind) {
    match kind {
        LayerKind::Flipbook => {
            layer.model = None;
            layer.trail = None;
        }
        LayerKind::Model => {
            layer.trail = None;
            layer.model = Some(String::new());
        }
        LayerKind::Trail => {
            layer.model = None;
            layer.trail = Some(TrailFile::default());
        }
    }
}

fn layer_fields(
    ui: &mut egui::Ui,
    layer: &mut LayerFile,
    editor: &EffectEditor,
    touch: &mut Touch,
    animations: Vec<String>,
) {
    let kind = LayerKind::of(layer);
    grid(ui, &format!("layer {:p}", layer as *const _), |ui| {
        match kind {
            LayerKind::Model => field(ui, "Model", |ui| {
                let current = layer.model.clone().unwrap_or_default();
                if let Some(pick) = pick(ui, "model", &current, &editor.models) {
                    layer.model = Some(pick);
                    touch.click();
                }
            }),
            LayerKind::Flipbook | LayerKind::Trail => {
                field(ui, "Sheet", |ui| {
                    let current = layer.sheet.clone().unwrap_or_default();
                    if let Some(pick) = pick(ui, "sheet", &current, &editor.images) {
                        layer.sheet = Some(pick);
                        touch.click();
                    }
                });
                if !animations.is_empty() {
                    field(ui, "Animation", |ui| {
                        if let Some(pick) = pick(ui, "animation", &layer.animation, &animations) {
                            layer.animation = pick;
                            touch.click();
                        }
                    });
                }
            }
        }
        field(ui, "Blend", |ui| {
            ui.horizontal(|ui| {
                for (mode, name) in [(BlendMode::Additive, "Glow"), (BlendMode::Alpha, "Normal")] {
                    if ui.radio(layer.blend == mode, name).clicked() && layer.blend != mode {
                        layer.blend = mode;
                        touch.click();
                    }
                }
            });
        });
        field(ui, "Rise", |ui| {
            touch
                .track(
                    ui.add(
                        DragValue::new(&mut layer.rise)
                            .range(0.0..=400.0)
                            .suffix(" px"),
                    ),
                )
                .on_hover_text("Height above the ground");
        });
        if kind == LayerKind::Flipbook {
            field(ui, "Size", |ui| {
                touch.track(
                    ui.add(
                        DragValue::new(&mut layer.size)
                            .range(1.0..=1000.0)
                            .suffix(" px"),
                    ),
                );
            });
            field(ui, "Burst", |ui| {
                touch
                    .track(ui.add(DragValue::new(&mut layer.count).range(1..=64)))
                    .on_hover_text("How many fly out together");
            });
            if layer.count > 1 {
                field(ui, "Spread", |ui| {
                    touch.track(
                        ui.add(
                            DragValue::new(&mut layer.spread)
                                .range(0.0..=600.0)
                                .suffix(" px"),
                        ),
                    );
                });
            }
            field(ui, "Climb", |ui| {
                touch
                    .track(
                        ui.add(
                            DragValue::new(&mut layer.climb)
                                .range(-400.0..=400.0)
                                .suffix(" px"),
                        ),
                    )
                    .on_hover_text("How far up it goes over the animation");
            });
        }
        if let Some(trail) = &mut layer.trail {
            field(ui, "Radius", |ui| {
                touch.track(
                    ui.add(
                        DragValue::new(&mut trail.radius)
                            .range(0.0..=600.0)
                            .suffix(" px"),
                    ),
                );
            });
            field(ui, "Width", |ui| {
                touch.track(
                    ui.add(
                        DragValue::new(&mut trail.width)
                            .range(1.0..=400.0)
                            .suffix(" px"),
                    ),
                );
            });
            field(ui, "Turns", |ui| {
                touch.track(
                    ui.add(
                        DragValue::new(&mut trail.turns)
                            .range(-8.0..=8.0)
                            .speed(0.05),
                    ),
                );
            });
            field(ui, "Frames", |ui| {
                touch.track(ui.add(DragValue::new(&mut trail.frames).range(2..=240)));
            });
            field(ui, "Length", |ui| {
                touch
                    .track(ui.add(DragValue::new(&mut trail.segments).range(2..=40)))
                    .on_hover_text("How many steps the ribbon trails behind");
            });
        }
    });
}
