//! The game UI's look: dark translucent panels with a warm gold edge, as classic MMO windows,
//! and the colours of life, mana, money and messages.

use egui::{Color32, Frame, RichText, Stroke};

pub const PANEL: Color32 = Color32::from_rgba_premultiplied(15, 17, 22, 246);
pub const EDGE: Color32 = Color32::from_rgb(150, 120, 64);
pub const GOLD: Color32 = Color32::from_rgb(232, 190, 100);
pub const TEXT: Color32 = Color32::from_gray(225);
pub const DIM: Color32 = Color32::from_gray(150);
pub const LIFE: Color32 = Color32::from_rgb(200, 52, 52);
pub const MANA: Color32 = Color32::from_rgb(56, 110, 220);
pub const EXP: Color32 = Color32::from_rgb(120, 200, 90);
pub const MONEY: Color32 = Color32::from_rgb(240, 200, 90);
pub const ITEM: Color32 = Color32::from_rgb(140, 200, 255);
pub const GOOD: Color32 = Color32::from_rgb(120, 210, 120);
pub const WARN: Color32 = Color32::from_rgb(240, 190, 80);
pub const BAD: Color32 = Color32::from_rgb(235, 96, 96);
pub const SLOT: Color32 = Color32::from_rgba_premultiplied(40, 42, 50, 220);

/// A game window's frame.
pub fn panel() -> Frame {
    Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, EDGE))
        .corner_radius(6.0)
        .inner_margin(12.0)
}

/// A small dark backing for text over the game.
pub fn backing(alpha: f32) -> Color32 {
    Color32::from_black_alpha((150.0 * alpha) as u8)
}

/// A window title: icon and name in gold.
pub fn title(icon: &str, text: &str) -> RichText {
    RichText::new(format!("{icon}  {text}"))
        .color(GOLD)
        .strong()
}
