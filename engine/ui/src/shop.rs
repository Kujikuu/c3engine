//! The shop window: an NPC's goods with their prices; Buy puts one in the bag.

use crate::fonts;
use crate::icons;
use crate::style;
use crate::{GameView, Icons, UiAction};
use egui::{Color32, RichText, Vec2};

const ICON: f32 = 32.0;

pub(crate) fn window(
    ui: &mut egui::Ui,
    view: &GameView,
    npc: u32,
    icons: &mut dyn Icons,
    actions: &mut Vec<UiAction>,
) {
    let Some(p) = view.player() else { return };
    let rules = view.sim.rules();
    let name = view
        .sim
        .npcs()
        .iter()
        .find(|n| n.id == npc)
        .map_or("Shop", |n| n.name.as_str());
    let rect = view.rect;
    egui::Window::new("game shop")
        .title_bar(false)
        .frame(style::panel())
        .resizable(false)
        .constrain_to(rect)
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(12.0, 44.0))
        .show(ui.ctx(), |ui| {
            fonts::use_game_font(ui);
            ui.set_width(270.0);
            ui.horizontal(|ui| {
                ui.label(style::title(icons::STOREFRONT, name));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(icons::X).frame(false))
                        .on_hover_text("Close (Esc)")
                        .clicked()
                    {
                        actions.push(UiAction::CloseShop);
                    }
                });
            });
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .max_height((rect.height() - 200.0).max(120.0))
                .show(ui, |ui| {
                    for &kind in rules.shop(npc) {
                        let Some(t) = rules.item(kind) else { continue };
                        ui.horizontal(|ui| {
                            let (slot, _) =
                                ui.allocate_exact_size(Vec2::splat(ICON), egui::Sense::hover());
                            ui.painter().rect_filled(slot, 3.0, style::SLOT);
                            if let Some(texture) = icons.item(ui.ctx(), kind) {
                                ui.painter().image(
                                    texture,
                                    slot.shrink(2.0),
                                    egui::Rect::from_min_max(
                                        egui::pos2(0.0, 0.0),
                                        egui::pos2(1.0, 1.0),
                                    ),
                                    Color32::WHITE,
                                );
                            }
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&t.name).color(style::TEXT));
                                ui.label(
                                    RichText::new(format!("{}  {}", icons::COINS, t.price))
                                        .color(style::MONEY)
                                        .small(),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let affordable = p.money >= t.price;
                                    let buy = ui
                                        .add_enabled(affordable, egui::Button::new("Buy"))
                                        .on_disabled_hover_text("Not enough money");
                                    if buy.clicked() {
                                        actions.push(UiAction::Buy { npc, kind });
                                    }
                                },
                            );
                        });
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{}  {} {}",
                        icons::COINS,
                        p.money,
                        view.sim.settings().currency
                    ))
                    .color(style::MONEY),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new("Right-click bag items to sell")
                            .color(style::DIM)
                            .small(),
                    );
                });
            });
        });
}
