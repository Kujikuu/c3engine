//! The Inspector's Import section for a file of the project's assets: its id and how it is
//! imported (an image's filter, pivot, grid of frames and animations; a sound's volume and
//! looping; a model's scale). Changes go through `AssetCommand::SetImport`, so they undo.

use crate::app::{Focus, Studio};
use crate::icons;
use crate::theme;
use c3_core::asset::{
    Animation, AssetMeta, Filter, Grid, ImageImport, Importer, ModelImport, SoundImport,
};
use c3_edit::AssetCommand;
use eframe::egui::{self, DragValue, RichText};
use std::collections::HashMap;

/// A change the section asks for.
enum Edit {
    /// Starts one undo step for a drag or a typed value.
    Begin,
    Set(Importer),
    End,
}

/// The section's widgets for one asset, collecting what changed.
struct Form<'a> {
    edits: Vec<Edit>,
    /// Typed text not yet committed (animation names and frame lists), by field.
    buffers: &'a mut HashMap<String, String>,
}

impl Form<'_> {
    fn track(&mut self, resp: &egui::Response) {
        if resp.drag_started() || resp.gained_focus() {
            self.edits.push(Edit::Begin);
        }
        if resp.drag_stopped() || resp.lost_focus() {
            self.edits.push(Edit::End);
        }
    }

    fn set(&mut self, import: Importer) {
        self.edits.push(Edit::Set(import));
    }

    fn image(&mut self, ui: &mut egui::Ui, i: &ImageImport, size: Option<[u32; 2]>) {
        let mut next = i.clone();
        row(
            ui,
            "Filter",
            "How the image looks drawn larger or smaller",
            |ui| {
                for (filter, name) in [
                    (Filter::Linear, "Smooth"),
                    (Filter::Nearest, "Sharp pixels"),
                ] {
                    if ui.radio(i.filter == filter, name).clicked() {
                        next.filter = filter;
                    }
                }
            },
        );
        row(
            ui,
            "Pivot",
            "The point of a frame that stands on its cell (0 to 1; 0.5, 1 is the bottom middle)",
            |ui| {
                for p in 0..2 {
                    let resp = ui.add(
                        DragValue::new(&mut next.pivot[p])
                            .range(0.0..=1.0)
                            .speed(0.01)
                            .max_decimals(2),
                    );
                    self.track(&resp);
                }
            },
        );
        let mut sliced = i.grid.is_some();
        row(
            ui,
            "Frames",
            "Slice the image into a grid of frames: a sprite sheet or a tileset",
            |ui| {
                if ui.checkbox(&mut sliced, "Sliced into a grid").changed() {
                    next.grid = sliced.then(|| guess_grid(size));
                }
            },
        );
        if i.grid.is_some()
            && let Some(g) = &mut next.grid
        {
            let pairs: [(&str, &str, &mut [u32; 2]); 3] = [
                ("Cell", "Size of one frame, in pixels", &mut g.cell),
                ("Spacing", "Pixels between frames", &mut g.spacing),
                (
                    "Offset",
                    "Pixels before the first frame (left, top)",
                    &mut g.offset,
                ),
            ];
            for (label, hint, value) in pairs {
                row(ui, label, hint, |ui| {
                    for v in value.iter_mut() {
                        let resp = ui.add(DragValue::new(v).range(0..=8192).suffix(" px"));
                        self.track(&resp);
                    }
                });
            }
            row(
                ui,
                "Count",
                "Frames in use, when the last row isn't full",
                |ui| {
                    let mut all = g.count.is_none();
                    if ui.checkbox(&mut all, "All").changed() {
                        let total = size.map_or(1, |[w, h]| i.frame_count(w, h));
                        g.count = (!all).then_some(total);
                    }
                    if let Some(count) = &mut g.count {
                        let resp = ui.add(DragValue::new(count).range(1..=100_000));
                        self.track(&resp);
                    }
                },
            );
            if let Some([w, h]) = size {
                let (cols, rows) = g.columns_rows(w, h);
                let frames = i.frame_count(w, h);
                ui.label(
                    RichText::new(format!(
                        "{frames} frames ({cols} × {rows}), numbered from 0"
                    ))
                    .color(theme::TEXT_DIM)
                    .small(),
                );
            }
        }
        if i.grid.is_some() && next.grid.is_some() {
            ui.add_space(4.0);
            self.animations(ui, i, &mut next, size);
        }
        if next != *i {
            self.set(Importer::Image(next));
        }
    }

    fn animations(
        &mut self,
        ui: &mut egui::Ui,
        i: &ImageImport,
        next: &mut ImageImport,
        size: Option<[u32; 2]>,
    ) {
        ui.label(RichText::new("Animations").strong());
        let mut remove = None;
        for (name, a) in &i.animations {
            ui.horizontal(|ui| {
                let key = format!("name {name}");
                let text = self
                    .buffers
                    .entry(key.clone())
                    .or_insert_with(|| name.clone());
                let resp = ui.add(egui::TextEdit::singleline(text).desired_width(70.0));
                if resp.lost_focus()
                    && *text != *name
                    && !text.trim().is_empty()
                    && !i.animations.contains_key(text.trim())
                {
                    let renamed = text.trim().to_owned();
                    if let Some(anim) = next.animations.remove(name) {
                        next.animations.insert(renamed, anim);
                    }
                }
                if resp.lost_focus() {
                    self.buffers.remove(&key);
                }
                let key = format!("frames {name}");
                let shown = frames_text(&a.frames);
                let text = self.buffers.entry(key.clone()).or_insert(shown);
                let resp = ui
                    .add(
                        egui::TextEdit::singleline(text)
                            .desired_width(90.0)
                            .hint_text("0-3, 5"),
                    )
                    .on_hover_text("Frames in order: numbers and ranges such as 0-3, 5");
                if resp.lost_focus() {
                    if let Some(frames) = parse_frames(text)
                        && let Some(anim) = next.animations.get_mut(name)
                    {
                        anim.frames = frames;
                    }
                    self.buffers.remove(&key);
                }
                if let Some(anim) = next.animations.get_mut(name) {
                    let resp = ui
                        .add(
                            DragValue::new(&mut anim.frame_ms)
                                .range(10..=5000)
                                .suffix(" ms"),
                        )
                        .on_hover_text("Time per frame");
                    self.track(&resp);
                    ui.checkbox(&mut anim.looped, "Loop");
                }
                if ui
                    .small_button(icons::TRASH)
                    .on_hover_text("Remove the animation")
                    .clicked()
                {
                    remove = Some(name.clone());
                }
            });
        }
        if let Some(name) = remove {
            next.animations.remove(&name);
        }
        if ui
            .button(theme::icon_text(icons::PLUS, "Add animation"))
            .clicked()
        {
            let name = (1..)
                .map(|n| format!("animation {n}"))
                .find(|n| !i.animations.contains_key(n))
                .unwrap_or_default();
            let count = size.map_or(1, |[w, h]| i.frame_count(w, h)).max(1);
            next.animations.insert(
                name,
                Animation {
                    frames: (0..count).collect(),
                    frame_ms: 100,
                    looped: true,
                },
            );
        }
    }

    fn sound(&mut self, ui: &mut egui::Ui, s: &SoundImport) {
        let mut next = s.clone();
        row(
            ui,
            "Volume",
            "Loudness, 0 (silent) to 1 (as recorded)",
            |ui| {
                let resp = ui.add(egui::Slider::new(&mut next.volume, 0.0..=1.0));
                self.track(&resp);
            },
        );
        row(ui, "Loop", "Start again when it ends", |ui| {
            ui.checkbox(&mut next.looped, "");
        });
        row(
            ui,
            "Stream",
            "Play it as it is read (music) instead of decoding it first",
            |ui| {
                ui.checkbox(&mut next.stream, "");
            },
        );
        if next != *s {
            self.set(Importer::Sound(next));
        }
    }

    fn model(&mut self, ui: &mut egui::Ui, m: &ModelImport) {
        let mut next = m.clone();
        row(ui, "Scale", "World pixels per model unit", |ui| {
            let resp = ui.add(
                DragValue::new(&mut next.scale)
                    .range(0.01..=1000.0)
                    .speed(0.1),
            );
            self.track(&resp);
        });
        if next != *m {
            self.set(Importer::Model(next));
        }
    }
}

fn row(ui: &mut egui::Ui, label: &str, hint: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [64.0, 18.0],
            egui::Label::new(RichText::new(label).color(theme::TEXT_DIM)),
        )
        .on_hover_text(hint);
        body(ui);
    });
}

/// A first grid for an image of `size`: square frames of its height when it is a strip,
/// else 32 pixels.
fn guess_grid(size: Option<[u32; 2]>) -> Grid {
    let cell = match size {
        Some([w, h]) if w > h && w % h == 0 => [h, h],
        _ => [32, 32],
    };
    Grid {
        cell,
        ..Grid::default()
    }
}

/// `0-3, 5` for frames 0, 1, 2, 3, 5.
pub(crate) fn frames_text(frames: &[u32]) -> String {
    let mut parts = Vec::new();
    let mut i = 0;
    while i < frames.len() {
        let mut j = i;
        while j + 1 < frames.len() && frames[j + 1] == frames[j] + 1 {
            j += 1;
        }
        parts.push(if j > i + 1 {
            format!("{}-{}", frames[i], frames[j])
        } else if j == i + 1 {
            format!("{}, {}", frames[i], frames[j])
        } else {
            frames[i].to_string()
        });
        i = j + 1;
    }
    parts.join(", ")
}

/// Frames from text such as `0-3, 5` (ranges may run backwards: `3-0`).
pub(crate) fn parse_frames(text: &str) -> Option<Vec<u32>> {
    let mut out = Vec::new();
    for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b): (u32, u32) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
                if a <= b {
                    out.extend(a..=b);
                } else {
                    out.extend((b..=a).rev());
                }
            }
            None => out.push(part.parse().ok()?),
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Outlines the frames of `import` over an image preview drawn in `rect` (`size` pixels).
pub(crate) fn draw_grid(
    painter: &egui::Painter,
    rect: egui::Rect,
    import: &ImageImport,
    size: [u32; 2],
) {
    if import.grid.is_none() {
        return;
    }
    let [w, h] = size;
    let scale = rect.width() / w.max(1) as f32;
    let stroke = egui::Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.8));
    for index in 0..import.frame_count(w, h).min(4096) {
        if let Some([x, y, fw, fh]) = import.frame_rect(index, w, h) {
            let min = rect.min + egui::vec2(x as f32, y as f32) * scale;
            let frame = egui::Rect::from_min_size(min, egui::vec2(fw as f32, fh as f32) * scale);
            painter.rect_stroke(frame, 0.0, stroke, egui::StrokeKind::Inside);
            if frame.width() > 18.0 {
                painter.text(
                    frame.min + egui::vec2(2.0, 1.0),
                    egui::Align2::LEFT_TOP,
                    index.to_string(),
                    egui::FontId::proportional(9.0),
                    theme::ACCENT,
                );
            }
        }
    }
}

impl Studio {
    /// Play and Stop for sound `rel`, as the game plays it (its import settings).
    pub(crate) fn sound_preview(&mut self, ui: &mut egui::Ui, rel: &str) {
        let (mut play, mut stop) = (false, false);
        theme::section(ui, "Listen", |ui| {
            ui.horizontal(|ui| {
                play = ui.button(theme::icon_text(icons::PLAY, "Play")).clicked();
                stop = ui.button(theme::icon_text(icons::STOP, "Stop")).clicked();
            });
            ui.label(
                RichText::new("Plays as the game would: its volume, and looping when set.")
                    .color(theme::TEXT_DIM)
                    .small(),
            );
        });
        if !(play || stop) {
            return;
        }
        let game = self.game.clone();
        let audio = self.preview_audio.get_or_insert_with(|| {
            c3_audio::Audio::new(Box::new(move |path| {
                c3_play::audio::sound_file(&**game.as_ref()?, path)
            }))
        });
        if !audio.has_device() {
            self.error = Some("No sound device to play on".into());
            return;
        }
        audio.set_volumes(c3_audio::Volumes {
            master: 1.0,
            music: 1.0,
            sound: 1.0,
        });
        audio.set_music(None);
        if play {
            // Settings may have changed since it was last played.
            audio.forget(Some(rel));
            audio.set_music(Some(rel));
        }
    }

    /// The asset `rel` of the project's Assets folder, with its sidecar, when the database
    /// covers it.
    pub(crate) fn asset_meta(&self, root: usize, rel: &str) -> Option<&AssetMeta> {
        let doc = self.assets.as_ref()?;
        let dir = &self.browser.roots.get(root)?.dir;
        (dir == doc.db().root()).then(|| doc.db().meta(rel))?
    }

    /// The Import section for asset `rel` (image `size` when known).
    pub(crate) fn import_section(
        &mut self,
        ui: &mut egui::Ui,
        root: usize,
        rel: &str,
        size: Option<[u32; 2]>,
    ) {
        let Some(meta) = self.asset_meta(root, rel).cloned() else {
            return;
        };
        let mut form = Form {
            edits: Vec::new(),
            buffers: &mut self.import_buffers,
        };
        theme::section(ui, "Import", |ui| {
            theme::kv(ui, "Imported as", meta.import.name());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Id").color(theme::TEXT_DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Label::new(
                                RichText::new(meta.id.to_string()).monospace().small(),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_text("Stays when the file is renamed or moved · click to copy")
                        .clicked()
                    {
                        ui.ctx().copy_text(meta.id.to_string());
                    }
                });
            });
            match &meta.import {
                Importer::Image(i) => form.image(ui, i, size),
                Importer::Sound(s) => form.sound(ui, s),
                Importer::Model(m) => form.model(ui, m),
                Importer::Font | Importer::Data | Importer::Other => {}
            }
        });
        let edits = form.edits;
        if edits.is_empty() {
            return;
        }
        self.focus = Focus::Assets;
        let Some(doc) = &mut self.assets else {
            return;
        };
        for edit in edits {
            match edit {
                Edit::Begin => doc.begin_group(format!("Change how {rel} is imported")),
                Edit::End => doc.end_group(),
                Edit::Set(import) => {
                    let command = AssetCommand::SetImport {
                        path: rel.to_owned(),
                        import: serde_json::to_value(import).unwrap_or_default(),
                    };
                    if let Err(e) = doc.execute(&command) {
                        self.error = Some(e.to_string());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_read_and_write_as_text() {
        assert_eq!(parse_frames("0-3, 5"), Some(vec![0, 1, 2, 3, 5]));
        assert_eq!(parse_frames("3-1"), Some(vec![3, 2, 1]));
        assert_eq!(parse_frames("x"), None);
        assert_eq!(parse_frames(""), None);
        assert_eq!(frames_text(&[0, 1, 2, 3, 5, 6, 9]), "0-3, 5, 6, 9");
    }
}
