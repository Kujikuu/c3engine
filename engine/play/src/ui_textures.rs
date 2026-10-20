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
    cache: HashMap<(u32, u32), Option<TextureHandle>>,
    glows: HashMap<u32, Vec<TextureHandle>>,
    images: HashMap<String, Option<(TextureHandle, egui::Vec2)>>,
}

impl GameTextures {
    pub fn new(game: Rc<dyn GameModule>) -> Self {
        Self {
            game,
            cache: HashMap::new(),
            glows: HashMap::new(),
            images: HashMap::new(),
        }
    }
}

impl Icons for GameTextures {
    fn item(&mut self, ctx: &egui::Context, kind: u32, look: u32) -> Option<TextureId> {
        let game = &self.game;
        self.cache
            .entry((kind, look))
            .or_insert_with(|| {
                let image = game.item_icon(kind, look)?;
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    [image.width as usize, image.height as usize],
                    &image.rgba,
                );
                let name = format!("item {kind} {look}");
                Some(ctx.load_texture(name, color, egui::TextureOptions::LINEAR))
            })
            .as_ref()
            .map(TextureHandle::id)
    }

    fn item_glow(&mut self, ctx: &egui::Context, kind: u32, time: f64) -> Option<TextureId> {
        let game = &self.game;
        let frames = self.glows.entry(kind).or_insert_with(|| {
            let images = game.item_glow(kind).into_iter().enumerate();
            images
                .map(|(n, image)| {
                    let size = [image.width as usize, image.height as usize];
                    let color = egui::ColorImage::from_rgba_unmultiplied(size, &image.rgba);
                    let name = format!("glow {kind} {n}");
                    ctx.load_texture(name, color, egui::TextureOptions::LINEAR)
                })
                .collect()
        });
        let at = (time * 10.0) as usize % frames.len().max(1);
        frames.get(at).map(TextureHandle::id)
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
