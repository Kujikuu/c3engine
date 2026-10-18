//! The fonts of the game's text. Game windows draw in the `game` family: the project's own
//! fonts (Project Settings > Localization) in order, then egui's defaults and the icon font
//! for what they lack. The host's own UI (the Studio) keeps its fonts.

use egui::{FontData, FontDefinitions, FontFamily, FontId};
use std::sync::Arc;

/// The family game text is drawn in.
pub const GAME_FAMILY: &str = "game";

/// A font file of the project: its name (any unique text) and its bytes (TTF/OTF).
pub struct GameFont {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Sets egui's default fonts, the icon font and the `game` family made of `game_fonts`
/// (empty: the default fonts).
pub fn install(ctx: &egui::Context, game_fonts: Vec<GameFont>) {
    let mut fonts = FontDefinitions::default();
    crate::icons::add_to(&mut fonts);
    let mut family: Vec<String> = Vec::new();
    for font in game_fonts {
        let key = format!("{GAME_FAMILY}:{}", font.name);
        fonts
            .font_data
            .insert(key.clone(), Arc::new(FontData::from_owned(font.bytes)));
        family.push(key);
    }
    family.extend(
        fonts
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default(),
    );
    fonts
        .families
        .insert(FontFamily::Name(GAME_FAMILY.into()), family);
    ctx.set_fonts(fonts);
}

/// A game font of `size` points.
pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(GAME_FAMILY.into()))
}

/// Makes text in `ui` (and what it holds) use the game's fonts, at the sizes it had.
pub fn use_game_font(ui: &mut egui::Ui) {
    for font in ui.style_mut().text_styles.values_mut() {
        font.family = FontFamily::Name(GAME_FAMILY.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_family_is_usable_without_project_fonts() {
        let ctx = egui::Context::default();
        install(&ctx, Vec::new());
        let mut output = ctx.run_ui(Default::default(), |ui| {
            use_game_font(ui);
            ui.label("Bag 3/40");
            ui.painter().text(
                egui::Pos2::ZERO,
                egui::Align2::LEFT_TOP,
                "x",
                font(12.0),
                egui::Color32::WHITE,
            );
        });
        output.textures_delta.clear();
    }
}
