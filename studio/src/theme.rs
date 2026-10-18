//! Studio look, modelled on Unity's dark editor theme, and shared widgets, so every panel
//! feels the same.

use crate::dock::DockStyle;
use crate::icons;
use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, RichText, Stroke, TextStyle, Vec2,
};

/// Selection and focus blue (Unity's).
pub const ACCENT: Color32 = Color32::from_rgb(58, 121, 187);
/// Errors and destructive states.
pub const DANGER: Color32 = Color32::from_rgb(235, 96, 96);
/// Non-fatal problems.
pub const WARNING: Color32 = Color32::from_rgb(240, 190, 80);
/// Success confirmations.
pub const SUCCESS: Color32 = Color32::from_rgb(110, 196, 120);
/// The Studio's name in the title area.
pub const BRAND: Color32 = Color32::from_rgb(222, 178, 92);

const WINDOW: Color32 = Color32::from_gray(56);
const PANEL: Color32 = Color32::from_gray(56);
const TAB_BAR: Color32 = Color32::from_gray(40);
const TAB_HOVER: Color32 = Color32::from_gray(50);
const BAR: Color32 = Color32::from_gray(48);
const INPUT: Color32 = Color32::from_gray(42);
const BUTTON: Color32 = Color32::from_gray(72);
const BUTTON_HOVER: Color32 = Color32::from_gray(88);
const BORDER: Color32 = Color32::from_gray(30);
pub const TEXT: Color32 = Color32::from_gray(210);
pub const TEXT_DIM: Color32 = Color32::from_gray(140);

pub fn apply(ctx: &egui::Context) {
    icons::install(ctx);
    let mut style = (*ctx.global_style()).clone();
    style.visuals = egui::Visuals::dark();
    let v = &mut style.visuals;
    v.override_text_color = Some(TEXT);
    v.panel_fill = PANEL;
    v.window_fill = WINDOW;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.extreme_bg_color = INPUT;
    v.faint_bg_color = Color32::from_gray(62);
    v.selection.bg_fill = Color32::from_rgb(44, 93, 135);
    v.selection.stroke = Stroke::new(1.0, Color32::from_rgb(120, 170, 220));
    v.hyperlink_color = Color32::from_rgb(110, 165, 230);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.weak_bg_fill = BUTTON;
    v.widgets.inactive.bg_fill = BUTTON;
    v.widgets.hovered.weak_bg_fill = BUTTON_HOVER;
    v.widgets.hovered.bg_fill = BUTTON_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_gray(110));
    v.widgets.active.weak_bg_fill = Color32::from_gray(64);
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(3);
    }
    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(16.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
    ]
    .into();
    style.spacing.item_spacing = Vec2::new(6.0, 4.0);
    style.spacing.button_padding = Vec2::new(8.0, 3.0);
    style.spacing.interact_size.y = 22.0;
    ctx.set_global_style(style);
}

/// Colours of the docking layout.
pub fn dock_style() -> DockStyle {
    DockStyle {
        background: BORDER,
        tab_bar: TAB_BAR,
        tab_active: PANEL,
        tab_hover: TAB_HOVER,
        text: TEXT,
        text_dim: TEXT_DIM,
        accent: ACCENT,
    }
}

/// Fill of the menu bar and main toolbar.
pub const BAR_FILL: Color32 = BAR;

/// An icon-only toggle button (toolbars), highlighted when `on`.
pub fn icon_toggle(ui: &mut egui::Ui, on: bool, icon: &str, hint: &str) -> egui::Response {
    let color = if on { Color32::WHITE } else { TEXT };
    let button = egui::Button::new(RichText::new(icon).size(16.0).color(color))
        .min_size(Vec2::new(28.0, 24.0))
        .fill(if on { ACCENT } else { Color32::TRANSPARENT })
        .stroke(Stroke::NONE);
    ui.add(button).on_hover_text(hint)
}

/// Toggle styled as a chip with a colour swatch; tooltip shows the shortcut.
pub fn toggle_chip(ui: &mut egui::Ui, on: &mut bool, label: &str, swatch: Color32, key: &str) {
    let mut job = egui::text::LayoutJob::default();
    RichText::new(icons::CIRCLE)
        .color(if *on { swatch.to_opaque() } else { TEXT_DIM })
        .append_to(
            &mut job,
            ui.style(),
            egui::FontSelection::Default,
            egui::Align::Center,
        );
    RichText::new(format!("  {label}"))
        .color(if *on { TEXT } else { TEXT_DIM })
        .append_to(
            &mut job,
            ui.style(),
            egui::FontSelection::Default,
            egui::Align::Center,
        );
    let mut btn = egui::Button::new(job).corner_radius(CornerRadius::same(10));
    btn = if *on {
        btn.fill(swatch.gamma_multiply(0.25))
            .stroke(Stroke::new(1.0, swatch.gamma_multiply(0.8)))
    } else {
        btn.fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, Color32::from_gray(70)))
    };
    if ui
        .add(btn)
        .on_hover_text(format!("Show {label} ({key})"))
        .clicked()
    {
        *on ^= true;
    }
}

pub fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(RichText::new(title).strong())
        .default_open(true)
        .show(ui, |ui| {
            body(ui);
            ui.add_space(4.0);
        });
}

/// Label/value row, value right-aligned and truncated with full text on hover.
pub fn kv(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).color(TEXT_DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(value).truncate())
                .on_hover_text(value);
        });
    });
}

pub fn legend(ui: &mut egui::Ui, color: Color32, text: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
        ui.painter().rect_filled(r, 3.0, color.to_opaque());
        ui.label(text);
    });
}

/// A centred message for an empty panel: a large icon, a title and a hint.
pub fn empty_state(ui: &mut egui::Ui, icon: &str, title: &str, hint: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.3).max(8.0));
        ui.label(RichText::new(icon).size(40.0).color(Color32::from_gray(96)));
        ui.add_space(4.0);
        ui.label(RichText::new(title).size(15.0));
        ui.label(RichText::new(hint).color(TEXT_DIM));
    });
}

/// `icon  text` as one label.
pub fn icon_text(icon: &str, text: &str) -> String {
    format!("{icon}  {text}")
}
