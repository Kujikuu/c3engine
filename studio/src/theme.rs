//! Studio look and shared widgets, so every panel feels the same.

use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, Vec2};

/// Warm gold accent, a nod to Conquer's UI.
pub const ACCENT: Color32 = Color32::from_rgb(222, 178, 92);
/// Errors and destructive states.
pub const DANGER: Color32 = Color32::from_rgb(235, 90, 90);
/// Non-fatal problems.
pub const WARNING: Color32 = Color32::from_rgb(240, 190, 80);

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    style.visuals = egui::Visuals::dark();
    let v = &mut style.visuals;
    v.panel_fill = Color32::from_rgb(24, 25, 28);
    v.window_fill = Color32::from_rgb(30, 31, 35);
    v.extreme_bg_color = Color32::from_rgb(16, 17, 19);
    v.faint_bg_color = Color32::from_rgb(32, 33, 37);
    v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(5);
    }
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 4.0);
    ctx.set_global_style(style);
}

/// Toggle styled as a chip with a colour swatch; tooltip shows the shortcut.
pub fn toggle_chip(ui: &mut egui::Ui, on: &mut bool, label: &str, swatch: Color32, key: &str) {
    let text = RichText::new(format!("●  {label}")).color(if *on {
        Color32::WHITE
    } else {
        Color32::from_gray(120)
    });
    let mut btn = egui::Button::new(text).corner_radius(CornerRadius::same(12));
    btn = if *on {
        btn.fill(swatch.gamma_multiply(0.35))
            .stroke(Stroke::new(1.0, swatch.to_opaque()))
    } else {
        btn.fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, Color32::from_gray(60)))
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
        ui.label(RichText::new(key).weak());
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

pub fn empty_state(ui: &mut egui::Ui, client_ok: bool) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.35);
            ui.label(RichText::new("🗺").size(48.0).color(Color32::from_gray(90)));
            if client_ok {
                ui.label(RichText::new("Select a map to start").size(18.0));
                ui.label(
                    RichText::new("Pick one from the list on the left, or press PgDn.").weak(),
                );
            } else {
                ui.label(RichText::new("No client loaded").size(18.0));
                ui.label(
                    RichText::new("Set the client folder in the inspector on the right.").weak(),
                );
            }
        });
    });
}
