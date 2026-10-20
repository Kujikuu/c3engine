//! The chat log at the bottom left: system messages, notices and what players say, with an
//! input line (Enter to type, Enter to send, Esc to stop).

use crate::fonts;
use crate::style;
use crate::{UiAction, item_name};
use c3_game::{EntityId, Event, Sim};
use egui::{Color32, Rect};
use std::collections::VecDeque;

/// Lines kept in the log.
const KEEP: usize = 100;
/// Lines shown.
const SHOWN: usize = 7;
const WIDTH: f32 = 380.0;

#[derive(Default)]
pub(crate) struct Chat {
    lines: VecDeque<(String, Color32)>,
    input: String,
    pub typing: bool,
    /// Opened this frame (by an Enter that mustn't also send).
    opening: bool,
}

impl Chat {
    pub fn push(&mut self, text: String, color: Color32) {
        self.lines.push_back((text, color));
        if self.lines.len() > KEEP {
            self.lines.pop_front();
        }
    }

    /// A line for what happened to the hero.
    pub fn event(&mut self, event: &Event, sim: &Sim, hero: EntityId) {
        let currency = &sim.settings().currency;
        let line = match event {
            Event::Died { id, .. } if *id == hero => Some((
                format!(
                    "You died. You come back in {} seconds.",
                    sim.settings().revive_seconds
                ),
                style::BAD,
            )),
            Event::Revived(id) if *id == hero => Some(("You are back.".to_owned(), style::GOOD)),
            Event::StatusEnded { target, status } if *target == hero => sim
                .rules()
                .status(*status)
                .map(|t| (format!("{} wore off.", t.name), style::DIM)),
            Event::LeveledUp { player, level } if *player == hero => {
                Some((format!("Level up! You are level {level}."), style::GOLD))
            }
            Event::PickedUp {
                player,
                money,
                kind,
            } if *player == hero => Some((
                match kind {
                    Some(k) => format!("You picked up {}.", item_name(sim, *k)),
                    None => format!("You picked up {money} {currency}."),
                },
                style::GOOD,
            )),
            Event::Bought {
                player,
                kind,
                price,
            } if *player == hero => Some((
                format!(
                    "You bought {} for {price} {currency}.",
                    item_name(sim, *kind)
                ),
                style::TEXT,
            )),
            Event::Sold {
                player,
                kind,
                price,
            } if *player == hero => Some((
                format!("You sold {} for {price} {currency}.", item_name(sim, *kind)),
                style::TEXT,
            )),
            Event::Used { player, kind } if *player == hero => {
                Some((format!("You used {}.", item_name(sim, *kind)), style::TEXT))
            }
            Event::Message { player, text } if *player == hero => Some((text.clone(), style::WARN)),
            Event::Notice { player, text } if *player == hero => Some((text.clone(), style::GOLD)),
            Event::Chat {
                name,
                text,
                channel,
                ..
            } => {
                let tag = match channel {
                    c3_game::ChatChannel::Local => "",
                    c3_game::ChatChannel::Whisper => "[Whisper] ",
                    c3_game::ChatChannel::Team => "[Team] ",
                    c3_game::ChatChannel::Guild => "[Guild] ",
                    c3_game::ChatChannel::Friend => "[Friend] ",
                };
                Some((format!("{tag}{name}: {text}"), style::TEXT))
            }
            Event::CommandReply { player, ok, text } if *player == hero => {
                Some((text.clone(), if *ok { style::GOOD } else { style::WARN }))
            }
            _ => None,
        };
        if let Some((text, color)) = line {
            self.push(text, color);
        }
    }

    /// Clears the lines shown.
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Starts typing a line.
    pub fn open(&mut self) {
        self.typing = true;
        self.opening = true;
    }

    /// Draws the log and the line being typed, `lift` above its usual place (clear of the
    /// skill bar).
    pub fn show(&mut self, ui: &mut egui::Ui, rect: Rect, lift: f32, actions: &mut Vec<UiAction>) {
        let bottom = rect.bottom() - 74.0 - lift;
        let painter = ui.painter_at(rect);
        let font = fonts::font(13.0);
        let mut y = bottom - if self.typing { 28.0 } else { 0.0 };
        for (text, color) in self.lines.iter().rev().take(SHOWN) {
            let galley = painter.layout(text.clone(), font.clone(), *color, WIDTH);
            y -= galley.size().y + 3.0;
            let pos = egui::pos2(rect.left() + 12.0, y);
            painter.rect_filled(
                Rect::from_min_size(
                    pos - egui::vec2(5.0, 1.0),
                    galley.size() + egui::vec2(10.0, 2.0),
                ),
                3.0,
                style::backing(0.8),
            );
            painter.galley(pos, galley, *color);
        }
        if !self.typing {
            return;
        }
        let id = egui::Id::unique("game chat input");
        egui::Area::new(id.with("area"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(rect.left() + 8.0, bottom - 24.0))
            .show(ui.ctx(), |ui| {
                fonts::use_game_font(ui);
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.input)
                        .id(id)
                        .hint_text("Say something, or /help")
                        .desired_width(WIDTH),
                );
                field.request_focus();
                let (send, cancel) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::Enter),
                        i.key_pressed(egui::Key::Escape),
                    )
                });
                // The Enter that opened the line isn't the one that sends it.
                let opening = std::mem::take(&mut self.opening);
                let send = send && !opening;
                if send {
                    let text = std::mem::take(&mut self.input);
                    if !text.trim().is_empty() {
                        actions.push(UiAction::Say(text.trim().to_owned()));
                    }
                    self.typing = false;
                } else if cancel {
                    self.typing = false;
                }
            });
    }
}
