//! Quests in the UI: the quest log (taken quests with their goal, progress and rewards, then
//! the completed ones), a small tracker of taken quests while the log is closed, and marks
//! over NPCs: a gold `!` for a quest to take, a gold `?` for one to hand in, a grey `?` for
//! one still running.

use crate::fonts;
use crate::icons;
use crate::style;
use crate::{GameView, item_name};
use c3_game::{Player, Sim};
use c3_script::quest::{DONE, NOT_TAKEN, TAKEN};
use c3_script::{Goal, Quest};
use egui::{Color32, RichText};

/// Below the host's hint line along the top of the game view.
const TOP: f32 = 44.0;
const LOG_WIDTH: f32 = 290.0;

/// What a quest's goal asks, in words ("Kill Pheasant").
fn goal_text(sim: &Sim, q: &Quest) -> String {
    let monster = |kind: i64| {
        u32::try_from(kind)
            .ok()
            .and_then(|k| sim.rules().monster(k))
            .map_or_else(|| format!("monster {kind}"), |m| m.name.clone())
    };
    let item = |kind: i64| {
        u32::try_from(kind).map_or_else(|_| format!("item {kind}"), |k| item_name(sim, k))
    };
    match q.goal {
        Goal::Talk => "Talk to the quest giver".to_owned(),
        Goal::Kill { monster: m, .. } => format!("Kill {}", monster(m)),
        Goal::Collect { item: i, .. } => format!("Bring {}", item(i)),
    }
}

/// The rewards of a quest in one line.
fn rewards_text(sim: &Sim, q: &Quest) -> String {
    let mut parts = Vec::new();
    if q.reward_money > 0 {
        parts.push(format!("{} {}", icons::COINS, q.reward_money));
    }
    if q.reward_exp > 0 {
        parts.push(format!("{} {} exp", icons::STAR, q.reward_exp));
    }
    if let Ok(kind) = u32::try_from(q.reward_item)
        && kind > 0
    {
        parts.push(format!("{} {}", icons::GIFT, item_name(sim, kind)));
    }
    parts.join("    ")
}

/// Whether `p` has met quest `q`'s goal.
fn ready(p: &Player, q: &Quest) -> bool {
    let (done, needed) = p.quest_progress(q);
    p.quest_state(q.id) == TAKEN && done >= needed
}

/// A bar filled to `done` of `needed`, with the count written on it.
fn progress_bar(ui: &mut egui::Ui, done: i64, needed: i64) {
    let fill = done as f32 / needed.max(1) as f32;
    let color = if done >= needed {
        style::GOOD
    } else {
        style::EXP
    };
    ui.add(
        egui::ProgressBar::new(fill)
            .desired_width(ui.available_width())
            .desired_height(14.0)
            .fill(color.gamma_multiply(0.7))
            .text(
                RichText::new(format!("{done} / {needed}"))
                    .size(10.5)
                    .color(Color32::WHITE),
            ),
    );
}

/// The quest log window.
pub(crate) fn window(ui: &mut egui::Ui, view: &GameView, open: &mut bool) {
    let Some(p) = view.player() else { return };
    let quests = view.sim.quests();
    let taken: Vec<&Quest> = quests
        .iter()
        .filter(|q| p.quest_state(q.id) == TAKEN)
        .collect();
    let done: Vec<&Quest> = quests
        .iter()
        .filter(|q| p.quest_state(q.id) == DONE)
        .collect();
    egui::Window::new("game quests")
        .title_bar(false)
        .frame(style::panel())
        .resizable(false)
        .constrain_to(view.rect)
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(12.0, TOP))
        .show(ui.ctx(), |ui| {
            fonts::use_game_font(ui);
            ui.set_width(LOG_WIDTH);
            ui.horizontal(|ui| {
                ui.label(style::title(icons::SCROLL, "Quests"));
                ui.label(RichText::new(format!("{} taken", taken.len())).color(style::DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(icons::X).frame(false))
                        .on_hover_text("Close (L)")
                        .clicked()
                    {
                        *open = false;
                    }
                });
            });
            ui.add_space(6.0);
            if taken.is_empty() {
                ui.label(RichText::new("No quests taken.").color(style::TEXT));
                ui.label(
                    RichText::new("NPCs with a gold ! over their head have work for you.")
                        .color(style::DIM)
                        .small(),
                );
            }
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    for q in &taken {
                        quest_entry(ui, view.sim, p, q);
                        ui.add_space(8.0);
                    }
                    if !done.is_empty() {
                        let heading = RichText::new(format!(
                            "{}  Completed ({})",
                            icons::CHECK_CIRCLE,
                            done.len()
                        ))
                        .color(style::DIM);
                        egui::CollapsingHeader::new(heading)
                            .id_salt("completed quests")
                            .show(ui, |ui| {
                                for q in &done {
                                    ui.label(RichText::new(&q.name).color(style::DIM));
                                }
                            });
                    }
                });
        });
}

/// One taken quest: name, what to do, progress and rewards.
fn quest_entry(ui: &mut egui::Ui, sim: &Sim, p: &Player, q: &Quest) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(&q.name).color(style::GOLD).strong());
        if ready(p, q) {
            ui.label(
                RichText::new(format!("{} Ready", icons::CHECK_CIRCLE))
                    .color(style::GOOD)
                    .small(),
            );
        }
    });
    if !q.description.is_empty() {
        ui.label(RichText::new(&q.description).color(style::TEXT).small());
    }
    let (done, needed) = p.quest_progress(q);
    ui.label(RichText::new(goal_text(sim, q)).color(style::DIM).small());
    if !matches!(q.goal, Goal::Talk) {
        progress_bar(ui, done, needed);
    }
    let rewards = rewards_text(sim, q);
    if !rewards.is_empty() {
        ui.label(RichText::new(rewards).color(style::MONEY).small());
    }
}

/// The taken quests' progress in a corner, while the log is closed.
pub(crate) fn tracker(ui: &mut egui::Ui, view: &GameView) {
    let Some(p) = view.player() else { return };
    let taken: Vec<&Quest> = view
        .sim
        .quests()
        .iter()
        .filter(|q| p.quest_state(q.id) == TAKEN)
        .collect();
    if taken.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::unique("game quest tracker"))
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(12.0, TOP))
        .constrain_to(view.rect)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            fonts::use_game_font(ui);
            egui::Frame::new()
                .fill(style::backing(0.8))
                .corner_radius(4.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.set_max_width(220.0);
                    ui.label(
                        RichText::new(format!("{}  Quests (L)", icons::SCROLL))
                            .color(style::GOLD)
                            .small(),
                    );
                    for q in taken {
                        let (done, needed) = p.quest_progress(q);
                        let (line, color) = if ready(p, q) {
                            (
                                format!("{}  {}: hand in", icons::CHECK_CIRCLE, q.name),
                                style::GOOD,
                            )
                        } else if matches!(q.goal, Goal::Talk) {
                            (q.name.clone(), style::TEXT)
                        } else {
                            (format!("{}  {done} / {needed}", q.name), style::TEXT)
                        };
                        ui.label(RichText::new(line).color(color).small());
                    }
                });
        });
}

/// Marks over the NPCs who give quests.
pub(crate) fn npc_marks(painter: &egui::Painter, view: &GameView) {
    let Some(p) = view.player() else { return };
    let quests = view.sim.quests();
    for npc in view.sim.npcs() {
        let given = || quests.iter().filter(|q| q.giver == i64::from(npc.id));
        let mark = if given().any(|q| ready(p, q)) {
            Some(("?", style::GOLD))
        } else if given()
            .any(|q| p.quest_state(q.id) == NOT_TAKEN && i64::from(p.level) >= q.min_level)
        {
            Some(("!", style::GOLD))
        } else if given().any(|q| p.quest_state(q.id) == TAKEN) {
            Some(("?", style::DIM))
        } else {
            None
        };
        let Some((text, color)) = mark else { continue };
        let at = (view.npc_head)(npc.cell);
        if !view.rect.contains(at) {
            continue;
        }
        painter.circle_filled(at, 11.0, style::backing(1.2));
        painter.circle_stroke(at, 11.0, egui::Stroke::new(1.5, color));
        painter.text(
            at,
            egui::Align2::CENTER_CENTER,
            text,
            fonts::font(16.0),
            color,
        );
    }
}
