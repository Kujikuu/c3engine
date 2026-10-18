//! The icon font (Phosphor Icons, regular weight, MIT: `assets/fonts/LICENSE-Phosphor.txt`)
//! shared by the game UI and the Studio. Icons are text: `format!("{COINS} 120")`. Find more
//! codepoints with `python scripts/phosphor_icon.py engine/ui/assets/fonts/Phosphor.ttf <name>`.

use egui::FontDefinitions;
use std::sync::Arc;

const FONT_NAME: &str = "phosphor";
const FONT: &[u8] = include_bytes!("../assets/fonts/Phosphor.ttf");

/// Adds the icon font as a fallback to every text family of `fonts`.
pub fn add_to(fonts: &mut FontDefinitions) {
    fonts.font_data.insert(
        FONT_NAME.to_owned(),
        Arc::new(egui::FontData::from_static(FONT)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push(FONT_NAME.to_owned());
    }
}

/// Sets egui's default fonts plus the icon font.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    add_to(&mut fonts);
    ctx.set_fonts(fonts);
}

pub const BACKPACK: &str = "\u{E922}";
pub const CHAT_CIRCLE_DOTS: &str = "\u{E16C}";
pub const CHAT_TEXT: &str = "\u{E17A}";
pub const CHECK_CIRCLE: &str = "\u{E184}";
pub const COINS: &str = "\u{E78E}";
pub const DROP: &str = "\u{E210}";
pub const GIFT: &str = "\u{E276}";
pub const HEART: &str = "\u{E2A8}";
pub const SCROLL: &str = "\u{EB7A}";
pub const SHIELD: &str = "\u{E40A}";
pub const SPEAKER_HIGH: &str = "\u{E44A}";
pub const SPEAKER_X: &str = "\u{E45C}";
pub const STAR: &str = "\u{E46A}";
pub const STOREFRONT: &str = "\u{E470}";
pub const SWORD: &str = "\u{E5BA}";
pub const T_SHIRT: &str = "\u{E670}";
pub const X: &str = "\u{E4F6}";
