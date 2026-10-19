//! The bag window: worn equipment and the bag's slots. Click an item to use or wear it;
//! right-click sells it while a shop is open. Hovering shows what an item does.

use crate::fonts;
use crate::icons;
use crate::style;
use crate::{GameView, Icons, UiAction};
use c3_game::Item;
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
    let (bag_size, currency) = (view.sim.bag_size(), &view.sim.settings().currency);
    egui::Window::new("game bag")
        .title_bar(false)
        .frame(style::panel())
        .resizable(false)
        .constrain_to(rect)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 44.0))
        .show(ui.ctx(), |ui| {
            fonts::use_game_font(ui);
            ui.horizontal(|ui| {
                ui.label(style::title(icons::BACKPACK, "Bag"));
                ui.label(RichText::new(format!("{}/{bag_size}", p.bag.len())).color(style::DIM));
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
                    let worn = (p.equipment.iter())
                        .find(|e| e.slot(view.sim.rules().item(e.kind)) == Some(*slot));
                    let resp = slot_button(ui, view, icons, worn, name);
                    if let Some(item) = worn {
                        if resp
                            .on_hover_text(format!(
                                "{}\nClick: take off",
                                describe_item(view, item)
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
                    for i in 0..bag_size {
                        let item = p.bag.get(i);
                        let resp = slot_button(ui, view, icons, item, "");
                        if let Some(item) = item {
                            let hint = match shop {
                                Some(_) => format!(
                                    "{}\nClick: use · Right-click: sell one for {} {}",
                                    describe_item(view, item),
                                    view.sim.rules().sell_price(item.kind),
                                    price_currency(view, item.kind)
                                ),
                                None => format!("{}\nClick: use", describe_item(view, item)),
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
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("{}  {} {currency}", icons::COINS, p.money))
                        .color(style::MONEY),
                );
                for name in &view.sim.settings().currencies {
                    let amount = p.wallet.get(name).copied().unwrap_or(0);
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("{}  {amount} {name}", icons::DIAMOND))
                            .color(style::MONEY),
                    );
                }
            });
        });
}

/// The currency item `kind` is priced in, as players call it.
pub(crate) fn price_currency(view: &GameView, kind: u32) -> String {
    let currency = view
        .sim
        .rules()
        .item(kind)
        .map(|t| t.currency.clone())
        .unwrap_or_default();
    view.sim.currency_name(&currency).to_owned()
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
    if let Some(item) = item.filter(|i| i.count > 1) {
        let at = rect.right_bottom() - egui::vec2(3.0, 1.0);
        let text = item.count.to_string();
        let font = fonts::font(11.0);
        painter.text(
            at + egui::vec2(1.0, 1.0),
            egui::Align2::RIGHT_BOTTOM,
            &text,
            font.clone(),
            Color32::BLACK,
        );
        painter.text(at, egui::Align2::RIGHT_BOTTOM, text, font, style::TEXT);
    }
    let broken = item.is_some_and(|i| {
        view.sim
            .rules()
            .item(i.kind)
            .is_some_and(|t| i.is_broken(t))
    });
    if broken {
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.5, style::BAD),
            egui::StrokeKind::Inside,
        );
    }
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
                    fonts::font(13.0),
                    style::ITEM,
                );
            }
        },
        None if !empty.is_empty() => {
            // The slot's name, wrapped to fit (`Right hand` on two lines).
            let galley = painter.layout(
                empty.to_owned(),
                fonts::font(9.0),
                Color32::from_gray(110),
                SLOT - 4.0,
            );
            let at = rect.center() - galley.size() / 2.0;
            painter.galley(at, galley, Color32::from_gray(110));
        }
        None => {}
    }
    resp
}

/// A carried item: its type's tooltip with the stack, quality and durability.
fn describe_item(view: &GameView, item: &Item) -> String {
    let mut text = describe(view, item.kind);
    let Some(t) = view.sim.rules().item(item.kind) else {
        return text;
    };
    let settings = view.sim.settings();
    if item.count > 1 {
        text.push_str(&format!("\n× {}", item.count));
    }
    if item.quality > 0 || t.slot.is_some() {
        let percent = u32::from(item.quality) * settings.quality_bonus_percent;
        let name = settings.quality_name(item.quality);
        if percent > 0 {
            text.push_str(&format!("\n{name} (+{percent} %)"));
        } else if !name.is_empty() {
            text.push_str(&format!("\n{name}"));
        }
    }
    if t.durability > 0 {
        if item.durability == 0 {
            text.push_str("\nBroken: it gives nothing until mended");
        } else {
            text.push_str(&format!(
                "\nDurability {}/{}",
                item.durability, t.durability
            ));
        }
    }
    text
}

/// An item type's name and what it does, for its tooltip.
pub(crate) fn describe(view: &GameView, kind: u32) -> String {
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
