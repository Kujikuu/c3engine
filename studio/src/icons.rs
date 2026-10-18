//! The Studio's icon set: Phosphor Icons (regular weight, MIT, see
//! `assets/fonts/LICENSE-Phosphor.txt`), bundled as a font. Icons are text, so they sit inline
//! in labels and buttons: `format!("{FLOPPY_DISK}  Save")`. Never use emoji for icons.

use eframe::egui;
use std::sync::Arc;

const FONT_NAME: &str = "phosphor";
const FONT: &[u8] = include_bytes!("../assets/fonts/Phosphor.ttf");

/// Adds the icon font as a fallback to every text family.
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
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
    ctx.set_fonts(fonts);
}

// Codepoints from the Phosphor regular font. Keep this list to icons the Studio uses.
pub const CHECK_CIRCLE: &str = "\u{E184}";
pub const CIRCLE: &str = "\u{E18A}";
pub const CLOCK_COUNTER_CLOCKWISE: &str = "\u{E1A0}";
pub const CORNERS_OUT: &str = "\u{E1D0}";
pub const CUBE: &str = "\u{E1DA}";
pub const DOOR_OPEN: &str = "\u{E7E6}";
pub const DOWNLOAD_SIMPLE: &str = "\u{E20C}";
pub const ERASER: &str = "\u{E21E}";
pub const FILE: &str = "\u{E230}";
pub const FLOPPY_DISK: &str = "\u{E248}";
pub const FOLDER_OPEN: &str = "\u{E256}";
pub const GAME_CONTROLLER: &str = "\u{E26E}";
pub const HAND: &str = "\u{E298}";
pub const INFO: &str = "\u{E2CE}";
pub const LAYOUT: &str = "\u{E6D6}";
pub const MAGNIFYING_GLASS: &str = "\u{E30C}";
pub const MAGNIFYING_GLASS_MINUS: &str = "\u{E30E}";
pub const MAGNIFYING_GLASS_PLUS: &str = "\u{E310}";
pub const MAP_TRIFOLD: &str = "\u{E31A}";
pub const MOUNTAINS: &str = "\u{E7AE}";
pub const PAUSE: &str = "\u{E39E}";
pub const PERSON_SIMPLE: &str = "\u{E72E}";
pub const PLAY: &str = "\u{E3D0}";
pub const PLUS: &str = "\u{E3D4}";
pub const PROHIBIT: &str = "\u{E3DE}";
pub const SKIP_BACK: &str = "\u{E5A4}";
pub const SKIP_FORWARD: &str = "\u{E5A6}";
pub const SLIDERS: &str = "\u{E432}";
pub const SPARKLE: &str = "\u{E6A2}";
pub const SPEAKER_HIGH: &str = "\u{E44A}";
pub const STACK: &str = "\u{E466}";
pub const STOP: &str = "\u{E46C}";
pub const TERMINAL: &str = "\u{E47E}";
pub const TRASH: &str = "\u{E4A6}";
pub const TREE: &str = "\u{E6DA}";
pub const TREE_STRUCTURE: &str = "\u{E67C}";
pub const WARNING: &str = "\u{E4E0}";
pub const X: &str = "\u{E4F6}";
pub const X_CIRCLE: &str = "\u{E4F8}";
