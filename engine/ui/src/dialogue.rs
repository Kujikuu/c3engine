//! The NPC dialogue window: what the NPC says and the answers (click, or number keys).

use crate::UiAction;
use crate::icons;
use crate::style;
use c3_script::Conversation;
use egui::{Color32, Rect, RichText};

const WIDTH: f32 = 420.0;

const NUMBER_KEYS: [egui::Key; 9] = [
    egui::Key::Num1,
    egui::Key::Num2,
    egui::Key::Num3,
    egui::Key::Num4,
    egui::Key::Num5,
    egui::Key::Num6,
    egui::Key::Num7,
    egui::Key::Num8,
    egui::Key::Num9,
];

/// Shows `conversation`; `keys`: number keys may answer.
pub(crate) fn window(
    ui: &mut egui::Ui,
    rect: Rect,
    conversation: &Conversation,
    keys: bool,
    actions: &mut Vec<UiAction>,
) {
    let page = &conversation.page;
    egui::Area::new(egui::Id::unique("game dialogue"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(
            rect.center().x - WIDTH / 2.0,
            rect.bottom() - 84.0,
        ))
        .pivot(egui::Align2::LEFT_BOTTOM)
        .constrain_to(rect)
        .show(ui.ctx(), |ui| {
            style::panel().show(ui, |ui| {
                ui.set_width(WIDTH);
                ui.horizontal(|ui| {
                    ui.label(style::title(
                        icons::CHAT_CIRCLE_DOTS,
                        &conversation.npc.name,
                    ));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(icons::X).frame(false))
                            .on_hover_text("Close (Esc)")
                            .clicked()
                        {
                            actions.push(UiAction::CloseDialogue);
                        }
                    });
                });
                ui.add_space(4.0);
                for line in &page.lines {
                    ui.label(RichText::new(line).color(Color32::WHITE).size(14.0));
                }
                ui.add_space(8.0);
                for (i, option) in page.options.iter().enumerate() {
                    let text = RichText::new(format!("{}. {}", i + 1, option.label)).size(13.5);
                    let button = egui::Button::new(text).min_size(egui::vec2(WIDTH, 26.0));
                    if ui.add(button).clicked() {
                        actions.push(UiAction::Answer(i));
                    }
                }
                if page.options.is_empty()
                    && ui
                        .add(egui::Button::new("Close").min_size(egui::vec2(WIDTH, 26.0)))
                        .clicked()
                {
                    actions.push(UiAction::CloseDialogue);
                }
            });
        });
    if !keys {
        return;
    }
    if let Some(i) = ui.input(|i| NUMBER_KEYS.iter().position(|k| i.key_pressed(*k))) {
        if i < page.options.len() {
            actions.push(UiAction::Answer(i));
        } else if i == 0 && page.options.is_empty() {
            actions.push(UiAction::CloseDialogue);
        }
    }
}
