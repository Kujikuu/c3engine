//! The bag window: worn equipment and the bag's slots. Click an item to use or wear it;
//! right-click sells it while a shop is open. Hovering shows what an item does.

use crate::icons;
use crate::style;
use crate::{GameView, Icons, UiAction};
use c3_game::{BAG_SIZE, Item};
use egui::{Color32, RichText, Sense, Vec2};

const COLUMNS: usize = 8;
const SLOT: f32 = 36.0;

/// What a click on a slot does.
enum Click {
    Use,
    Sell,
}

pub(crate) fn window(
    ui: &mut egui::Ui,
    view: &GameView,
    shop: Option<u32>,
    icons: &mut dyn Icons,
    open: &mut bool,
    actions: &mut Vec<UiAction>,
) {
    let Some(p) = view.player() else { return };
    let rect = view.rect;
    egui::Window::new("game bag")
        .title_bar(false)
        .frame(style::panel())
        .resizable(false)
        .constrain_to(rect)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 44.0))
        .show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                ui.label(style::title(icons::BACKPACK, "Bag"));
                ui.label(RichText::new(format!("{}/{BAG_SIZE}", p.bag.len())).color(style::DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(icons::X).frame(false))
                        .on_hover_text("Close (B)")
                        .clicked()
                    {
                        *open = false;
                    }
                });
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("{}  Worn", icons::T_SHIRT))
                    .color(style::DIM)
                    .small(),
            );
            ui.horizontal_wrapped(|ui| {
                for (slot, name) in view.slots {
                    let worn = p.equipment.iter().find(|e| {
                        view.sim.rules().item(e.kind).and_then(|t| t.slot) == Some(*slot)
                    });
                    let resp = slot_button(ui, view, icons, worn, name);
                    if let Some(item) = worn {
                        if resp
                            .on_hover_text(format!(
                                "{}\nClick: take off",
                                describe(view, item.kind)
                            ))
                            .clicked()
                        {
                            actions.push(UiAction::Unequip(item.id));
                        }
                    } else {
                        resp.on_hover_text(format!("{name}: nothing worn"));
                    }
                }
            });
            ui.add_space(6.0);
            egui::Grid::new("game bag slots")
                .spacing(Vec2::splat(4.0))
                .show(ui, |ui| {
                    for i in 0..BAG_SIZE {
                        let item = p.bag.get(i);
                        let resp = slot_button(ui, view, icons, item, "");
                        if let Some(item) = item {
                            let hint = match shop {
                                Some(_) => format!(
                                    "{}\nClick: use · Right-click: sell for {} silver",
                                    describe(view, item.kind),
                                    view.sim.rules().sell_price(item.kind)
                                ),
                                None => format!("{}\nClick: use", describe(view, item.kind)),
                            };
                            let resp = resp.on_hover_text(hint);
                            let click = if resp.clicked() {
                                Some(Click::Use)
                            } else if resp.secondary_clicked() && shop.is_some() {
                                Some(Click::Sell)
                            } else {
                                None
                            };
                            match (click, shop) {
                                (Some(Click::Use), _) => actions.push(UiAction::Use(item.id)),
                                (Some(Click::Sell), Some(npc)) => {
                                    actions.push(UiAction::Sell { npc, item: item.id })
                                }
                                _ => {}
                            }
                        }
                        if (i + 1) % COLUMNS == 0 {
                            ui.end_row();
                        }
                    }
                });
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("{}  {} silver", icons::COINS, p.money)).color(style::MONEY),
            );
        });
}

/// One slot: the item's icon (or its initials), else an empty square labelled `empty`.
fn slot_button(
    ui: &mut egui::Ui,
    view: &GameView,
    icons: &mut dyn Icons,
    item: Option<&Item>,
    empty: &str,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(SLOT), Sense::click());
    let painter = ui.painter_at(rect);
    let hovered = resp.hovered() && item.is_some();
    painter.rect_filled(rect, 4.0, style::SLOT);
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(
            1.0,
            if hovered {
                style::GOLD
            } else {
                Color32::from_gray(70)
            },
        ),
        egui::StrokeKind::Inside,
    );
    match item {
        Some(item) => match icons.item(ui.ctx(), item.kind) {
            Some(texture) => {
                painter.image(
                    texture,
                    rect.shrink(3.0),
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                let name = view.item_name(item.kind);
                let initials: String = name.chars().filter(|c| c.is_uppercase()).take(3).collect();
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    if initials.is_empty() {
                        name.chars().take(2).collect()
                    } else {
                        initials
                    },
                    egui::FontId::proportional(13.0),
                    style::ITEM,
                );
            }
        },
        None if !empty.is_empty() => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                empty,
                egui::FontId::proportional(9.0),
                Color32::from_gray(90),
            );
        }
        None => {}
    }
    resp
}

/// An item's name and what it does, for its tooltip.
fn describe(view: &GameView, kind: u32) -> String {
    let Some(t) = view.sim.rules().item(kind) else {
        return format!("item {kind}");
    };
    let mut lines = vec![t.name.clone()];
    let b = &t.bonus;
    if b.attack_max > 0 {
        lines.push(format!(
            "{}  Attack {}–{}",
            icons::SWORD,
            b.attack_min,
            b.attack_max
        ));
    }
    if b.defence > 0 {
        lines.push(format!("{}  Defence {}", icons::SHIELD, b.defence));
    }
    if b.dodge > 0 {
        lines.push(format!("Dodge {}", b.dodge));
    }
    if b.max_life > 0 {
        lines.push(format!("{}  Life +{}", icons::HEART, b.max_life));
    }
    if t.restores_life > 0 {
        lines.push(format!(
            "{}  Restores {} life",
            icons::HEART,
            t.restores_life
        ));
    }
    if t.restores_mana > 0 {
        lines.push(format!(
            "{}  Restores {} mana",
            icons::DROP,
            t.restores_mana
        ));
    }
    lines.join("\n")
}
