//! The Ground tool's Inspector: what a click does (paint, fill, erase, pick), the brush size
//! and the palette of images to paint with. A classic map paints with its own puzzle tiles
//! only (its ground file names them by number); an engine map also with the project's images.

use crate::app::Studio;
use crate::editor::GroundMode;
use crate::icons;
use crate::maps::MapRef;
use crate::theme;
use eframe::egui::{self, RichText};
use std::collections::HashMap;

/// Side of a palette thumbnail, in points.
const THUMB: f32 = 56.0;

/// Palette thumbnails by image path (`None`: the image couldn't be read).
#[derive(Default)]
pub(crate) struct Thumbnails {
    textures: HashMap<String, Option<egui::TextureHandle>>,
}

impl Thumbnails {
    /// Whether `image` was tried and couldn't be read.
    fn unreadable(&self, image: &str) -> bool {
        matches!(self.textures.get(image), Some(None))
    }
}

impl Studio {
    pub(crate) fn ground_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(doc) = &self.doc else { return };
        let Some(ground) = &doc.scene().ground else {
            theme::empty_state(
                ui,
                icons::PAINT_BRUSH,
                "No ground layer",
                "This map has no ground to paint.",
            );
            return;
        };
        let classic = matches!(doc.map, MapRef::Client(_));
        let mut images: Vec<String> = ground.tileset.frames.values().cloned().collect();
        if !classic && let Some(project) = &self.project {
            for image in crate::new_map::images_in(&project.assets_dir()) {
                if !images.contains(&image) {
                    images.push(image);
                }
            }
        }
        theme::section(ui, "Ground brush", |ui| {
            let brush = &mut self.editor.ground;
            ui.horizontal(|ui| {
                for (mode, icon, tip) in [
                    (
                        GroundMode::Paint,
                        icons::PAINT_BRUSH,
                        "Paint: drag over tiles",
                    ),
                    (
                        GroundMode::Fill,
                        icons::PAINT_BUCKET,
                        "Fill: the joined tiles that look alike",
                    ),
                    (GroundMode::Erase, icons::ERASER, "Erase: leave tiles empty"),
                    (
                        GroundMode::Pick,
                        icons::EYEDROPPER,
                        "Pick: take the image under the pointer",
                    ),
                ] {
                    if theme::icon_toggle(ui, brush.mode == mode, icon, tip).clicked() {
                        brush.mode = mode;
                    }
                }
            });
            let current = self.editor.ground.image.as_deref().unwrap_or("none picked");
            ui.label(
                RichText::new("Brush size: on the Scene toolbar ([ and ])")
                    .small()
                    .color(theme::TEXT_DIM),
            );
            ui.label(
                RichText::new(format!("Image: {current}"))
                    .small()
                    .color(theme::TEXT_DIM),
            );
            if classic {
                ui.label(
                    RichText::new("A client map paints with its own puzzle tiles.")
                        .small()
                        .color(theme::TEXT_DIM),
                );
            }
        });
        ui.add_space(4.0);
        // Tilesets can name images the content doesn't have; they aren't offered.
        images.retain(|i| !self.ground_thumbs.unreadable(i));
        let picked = self.palette(ui, &images);
        if let Some(image) = picked {
            let brush = &mut self.editor.ground;
            if matches!(brush.mode, GroundMode::Erase | GroundMode::Pick) {
                brush.mode = GroundMode::Paint;
            }
            brush.image = Some(image);
        }
    }

    /// Thumbnails of `images` in rows; returns the one clicked.
    fn palette(&mut self, ui: &mut egui::Ui, images: &[String]) -> Option<String> {
        // Room for the scroll bar and the buttons' frames.
        let per_row = (((ui.available_width() - 16.0) / (THUMB + 10.0)).floor() as usize).max(1);
        let rows = images.len().div_ceil(per_row);
        let mut picked = None;
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(
            ui,
            THUMB + 6.0,
            rows,
            |ui, range| {
                for row in range {
                    ui.horizontal(|ui| {
                        for image in images.iter().skip(row * per_row).take(per_row) {
                            let on = self.editor.ground.image.as_deref() == Some(image.as_str());
                            let thumb = self.thumbnail(ui.ctx(), image);
                            let size = egui::vec2(THUMB, THUMB);
                            let button = match &thumb {
                                Some(texture) => egui::Button::image(
                                    egui::Image::new(texture).fit_to_exact_size(size),
                                ),
                                None => egui::Button::new(icons::IMAGE).min_size(size),
                            };
                            if ui.add(button.selected(on)).on_hover_text(image).clicked() {
                                picked = Some(image.clone());
                            }
                        }
                    });
                }
            },
        );
        picked
    }

    /// A small texture of `image`, made once.
    fn thumbnail(&mut self, ctx: &egui::Context, image: &str) -> Option<egui::TextureHandle> {
        if let Some(t) = self.ground_thumbs.textures.get(image) {
            return t.clone();
        }
        let texture = self
            .game
            .as_ref()
            .and_then(|g| g.assets().image(image))
            .and_then(|img| {
                let full = image::RgbaImage::from_raw(img.width, img.height, img.rgba)?;
                let small = image::imageops::thumbnail(&full, 64, 64);
                let pixels = egui::ColorImage::from_rgba_unmultiplied(
                    [small.width() as usize, small.height() as usize],
                    small.as_raw(),
                );
                Some(ctx.load_texture(format!("ground {image}"), pixels, Default::default()))
            });
        self.ground_thumbs
            .textures
            .insert(image.to_owned(), texture.clone());
        texture
    }
}
