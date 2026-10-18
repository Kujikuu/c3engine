//! Item icons and the game UI's images as egui textures, loaded once each: what the in-game
//! UI (and the Studio's UI editor) draws with.

use crate::GameModule;
use c3_ui::Icons;
use egui::{TextureHandle, TextureId};
use std::collections::HashMap;
use std::rc::Rc;

/// Item icons and the UI's images as the game draws them, loaded once each.
pub struct GameTextures {
    game: Rc<dyn GameModule>,
    cache: HashMap<u32, Option<TextureHandle>>,
    images: HashMap<String, Option<(TextureHandle, egui::Vec2)>>,
}

impl GameTextures {
    pub fn new(game: Rc<dyn GameModule>) -> Self {
        Self {
            game,
            cache: HashMap::new(),
            images: HashMap::new(),
        }
    }
}

impl Icons for GameTextures {
    fn item(&mut self, ctx: &egui::Context, kind: u32) -> Option<TextureId> {
        let game = &self.game;
        self.cache
            .entry(kind)
            .or_insert_with(|| {
                let image = game.item_icon(kind)?;
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    [image.width as usize, image.height as usize],
                    &image.rgba,
                );
                Some(ctx.load_texture(format!("item {kind}"), color, egui::TextureOptions::LINEAR))
            })
            .as_ref()
            .map(TextureHandle::id)
    }
}

impl c3_ui::gui::GuiImages for GameTextures {
    fn image(&mut self, ctx: &egui::Context, path: &str) -> Option<(TextureId, egui::Vec2)> {
        let game = &self.game;
        self.images
            .entry(path.to_owned())
            .or_insert_with(|| {
                let image = game.assets().image(path)?;
                let size = egui::vec2(image.width as f32, image.height as f32);
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    [image.width as usize, image.height as usize],
                    &image.rgba,
                );
                let texture = ctx.load_texture(path, color, egui::TextureOptions::LINEAR);
                Some((texture, size))
            })
            .as_ref()
            .map(|(t, size)| (t.id(), *size))
    }
}
