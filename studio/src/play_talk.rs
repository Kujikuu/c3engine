//! NPC dialogue in Play mode: clicking an NPC walks the hero up to it and runs its script;
//! the dialogue window shows the NPC's page and answers (number keys pick, Esc closes).
//! Scripts are read from disk on each talk, so saving a script changes the next dialogue.

use crate::icons;
use crate::theme;
use c3_scene::{CellPos, Npc};
use c3_script::{Conversation, Effect, NpcInfo, PlayerState, ScriptError, ScriptHost};
use eframe::egui::{self, Color32, RichText};
use game_conquer::world::World;
use std::path::PathBuf;

/// Cells from the NPC within which the hero can talk.
const TALK_RANGE: i32 = 3;
/// Seconds a game message stays on screen.
const MESSAGE_SECONDS: f64 = 8.0;
/// Most game messages shown at once.
const MAX_MESSAGES: usize = 6;
const DIALOG_WIDTH: f32 = 420.0;

/// Where the project keeps what dialogues need.
pub struct TalkSetup {
    pub scripts: Option<PathBuf>,
    pub tables: Option<PathBuf>,
    /// The game's item table, for item names in messages.
    pub item_table: Option<&'static str>,
    /// Game map ids of the map being played (teleports within it work).
    pub map_ids: Vec<u32>,
}

/// A line for the Studio's console.
pub enum LogLine {
    Info(String),
    Error(String),
}

pub struct Talk {
    host: Option<ScriptHost>,
    item_table: Option<&'static str>,
    map_ids: Vec<u32>,
    pub player: PlayerState,
    conversation: Option<Conversation>,
    /// The NPC the hero walks up to before talking.
    approach: Option<(NpcInfo, String, CellPos)>,
    /// Game messages: when shown (seconds), text, colour.
    messages: Vec<(f64, String, Color32)>,
    log: Vec<LogLine>,
}

impl Talk {
    pub fn new(setup: TalkSetup) -> Self {
        Self {
            host: setup.scripts.map(|dir| ScriptHost::new(dir, setup.tables)),
            item_table: setup.item_table,
            map_ids: setup.map_ids,
            player: PlayerState {
                name: "Hero".into(),
                level: 1,
                money: 1000,
                ..Default::default()
            },
            conversation: None,
            approach: None,
            messages: Vec::new(),
            log: Vec::new(),
        }
    }

    /// Lines for the console since the last call.
    pub fn take_log(&mut self) -> Vec<LogLine> {
        std::mem::take(&mut self.log)
    }

    /// Whether a dialogue window is open (it takes the number keys and Esc).
    pub fn is_talking(&self) -> bool {
        self.conversation.is_some()
    }

    /// The player walked off: any dialogue ends.
    pub fn cancel(&mut self) {
        self.conversation = None;
        self.approach = None;
    }

    /// The player clicked `npc`: walk up to it, then talk.
    pub fn approach(&mut self, npc: &Npc, world: &mut World, now: f64) {
        self.cancel();
        if npc.script.is_empty() {
            self.message(
                now,
                format!("{} has nothing to say.", npc.name),
                theme::TEXT_DIM,
            );
            return;
        }
        let info = NpcInfo {
            id: npc.id,
            name: npc.name.clone(),
        };
        self.approach = Some((info, npc.script.clone(), npc.cell));
        let Some(hero) = world.hero() else { return };
        if distance(hero.cell, npc.cell) > 1 || hero.is_moving() {
            let goal = beside(world, npc.cell, hero.cell);
            // The hero is the first actor.
            if let Some(hero) = world.actors.first_mut() {
                hero.walk_to(goal, &world.scene);
            }
        }
    }

    /// Opens the dialogue once the hero has walked up to the NPC.
    pub fn update(&mut self, world: &mut World, now: f64) {
        let Some(hero) = world.hero_mut() else { return };
        if hero.is_moving() {
            return;
        }
        let Some((npc, script, cell)) = self.approach.take() else {
            return;
        };
        if distance(hero.cell, cell) > TALK_RANGE {
            self.message(now, format!("Can't reach {}.", npc.name), theme::WARNING);
            return;
        }
        hero.face(cell);
        let Some(host) = &mut self.host else { return };
        let step = Conversation::start(host, &script, npc, &mut self.player);
        self.after_step(step, world, now);
    }

    fn choose(&mut self, index: usize, world: &mut World, now: f64) {
        let (Some(conversation), Some(host)) = (self.conversation.take(), &mut self.host) else {
            return;
        };
        let step = conversation.choose(host, index, &mut self.player);
        self.after_step(step, world, now);
    }

    fn after_step(
        &mut self,
        step: Result<c3_script::Step, ScriptError>,
        world: &mut World,
        now: f64,
    ) {
        match step {
            Ok((next, effects)) => {
                self.conversation = next;
                for effect in effects {
                    self.apply(effect, world, now);
                }
            }
            Err(e) => {
                self.message(now, "Script error; see the Console.".into(), theme::DANGER);
                self.log.push(LogLine::Error(format!("Script error: {e}")));
            }
        }
    }

    fn apply(&mut self, effect: Effect, world: &mut World, now: f64) {
        match effect {
            Effect::Message(text) => self.message(now, text, theme::BRAND),
            Effect::GaveItem { item, count } => {
                let name = self.item_name(item);
                self.message(
                    now,
                    format!("You received {count} × {name}."),
                    theme::SUCCESS,
                );
            }
            Effect::TookItem { item, count } => {
                let name = self.item_name(item);
                self.message(
                    now,
                    format!("You handed over {count} × {name}."),
                    theme::TEXT,
                );
            }
            Effect::Money(n) if n >= 0 => {
                self.message(now, format!("You received {n} silver."), theme::SUCCESS)
            }
            Effect::Money(n) => self.message(now, format!("You paid {} silver.", -n), theme::TEXT),
            Effect::Teleport { map, cell } => {
                let here = map == 0 || self.map_ids.iter().any(|&id| i64::from(id) == map);
                let cell = (cell.0 as i32, cell.1 as i32);
                if here && world.scene.grid.index(cell).is_some() {
                    self.conversation = None;
                    if let Some(hero) = world.hero_mut() {
                        hero.place(cell);
                    }
                } else {
                    self.message(
                        now,
                        format!(
                            "Teleport to map {map} ({}, {}): Play mode runs only the open map.",
                            cell.0, cell.1
                        ),
                        theme::WARNING,
                    );
                }
            }
            Effect::Log(text) => self.log.push(LogLine::Info(format!("Script: {text}"))),
        }
    }

    fn item_name(&self, item: i64) -> String {
        self.item_table
            .zip(self.host.as_ref())
            .and_then(|(table, host)| host.table_text(table, item, "name"))
            .unwrap_or_else(|| format!("item {item}"))
    }

    fn message(&mut self, now: f64, text: String, color: Color32) {
        self.messages.push((now, text, color));
        if self.messages.len() > MAX_MESSAGES {
            self.messages.remove(0);
        }
    }

    /// Draws the dialogue window, game messages and the player's purse over the game view.
    pub fn ui(&mut self, ui: &mut egui::Ui, rect: egui::Rect, world: &mut World) {
        let now = ui.input(|i| i.time);
        self.messages.retain(|m| now - m.0 < MESSAGE_SECONDS);
        self.purse(ui, rect);
        self.message_list(ui, rect, now);
        let Some(conversation) = &self.conversation else {
            return;
        };
        let mut chosen = None;
        let mut close = false;
        egui::Area::new(egui::Id::unique("play dialogue"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(
                rect.center().x - DIALOG_WIDTH / 2.0,
                rect.bottom() - 40.0,
            ))
            .pivot(egui::Align2::LEFT_BOTTOM)
            .constrain_to(rect)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(18, 20, 24, 235))
                    .stroke(egui::Stroke::new(1.0, theme::BRAND.gamma_multiply(0.6)))
                    .corner_radius(6.0)
                    .inner_margin(14.0)
                    .show(ui, |ui| {
                        ui.set_width(DIALOG_WIDTH);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(theme::icon_text(
                                    icons::CHAT_CIRCLE_DOTS,
                                    &conversation.npc.name,
                                ))
                                .color(theme::BRAND)
                                .strong(),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    close = ui
                                        .add(egui::Button::new(icons::X).frame(false))
                                        .on_hover_text("Close (Esc)")
                                        .clicked();
                                },
                            );
                        });
                        ui.add_space(4.0);
                        for line in &conversation.page.lines {
                            ui.label(RichText::new(line).color(Color32::WHITE).size(14.0));
                        }
                        ui.add_space(8.0);
                        for (i, option) in conversation.page.options.iter().enumerate() {
                            let text = format!("{}. {}", i + 1, option.label);
                            let button = egui::Button::new(RichText::new(text).size(13.5))
                                .min_size(egui::vec2(DIALOG_WIDTH, 26.0));
                            if ui.add(button).clicked() {
                                chosen = Some(i);
                            }
                        }
                        if conversation.page.options.is_empty() {
                            let button =
                                egui::Button::new("Close").min_size(egui::vec2(DIALOG_WIDTH, 26.0));
                            close |= ui.add(button).clicked();
                        }
                    });
            });
        let keys = ui.input(|i| {
            let number = NUMBER_KEYS.iter().position(|k| i.key_pressed(*k));
            (number, i.key_pressed(egui::Key::Escape))
        });
        if let (Some(i), false) = keys {
            if i < conversation.page.options.len() {
                chosen = Some(i);
            } else if i == 0 && conversation.page.options.is_empty() {
                close = true;
            }
        }
        close |= keys.1;
        if let Some(i) = chosen {
            self.choose(i, world, now);
        } else if close {
            self.conversation = None;
        }
    }

    fn purse(&self, ui: &egui::Ui, rect: egui::Rect) {
        let p = &self.player;
        let items: i64 = p.items.values().sum();
        let text = format!(
            "{}  Lv {}   {}  {} silver   {}  {items} items",
            icons::USER,
            p.level,
            icons::COINS,
            p.money,
            icons::PACKAGE
        );
        let painter = ui.painter_at(rect);
        let galley = painter.layout_no_wrap(text, egui::FontId::proportional(13.0), theme::TEXT);
        let pad = egui::vec2(8.0, 5.0);
        let at = rect.right_bottom() - galley.size() - pad * 2.0 - egui::vec2(10.0, 10.0);
        painter.rect_filled(
            egui::Rect::from_min_size(at, galley.size() + pad * 2.0),
            4.0,
            Color32::from_black_alpha(150),
        );
        painter.galley(at + pad, galley, theme::TEXT);
    }

    fn message_list(&self, ui: &egui::Ui, rect: egui::Rect, now: f64) {
        let painter = ui.painter_at(rect);
        let mut y = rect.bottom() - 14.0;
        for (at, text, color) in self.messages.iter().rev() {
            // Fade out over the last second.
            let alpha = (MESSAGE_SECONDS - (now - at)).clamp(0.0, 1.0) as f32;
            let galley = painter.layout_no_wrap(
                text.clone(),
                egui::FontId::proportional(13.0),
                color.gamma_multiply(alpha),
            );
            y -= galley.size().y + 4.0;
            let pos = egui::pos2(rect.left() + 14.0, y);
            painter.rect_filled(
                egui::Rect::from_min_size(
                    pos - egui::vec2(6.0, 2.0),
                    galley.size() + egui::vec2(12.0, 4.0),
                ),
                3.0,
                Color32::from_black_alpha((120.0 * alpha) as u8),
            );
            painter.galley(pos, galley, Color32::WHITE);
        }
    }
}

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

/// Steps between two cells (diagonals count one).
fn distance(a: CellPos, b: CellPos) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

/// The walkable cell next to `npc` nearest `from` (or the NPC's own cell).
fn beside(world: &World, npc: CellPos, from: CellPos) -> CellPos {
    let grid = &world.scene.grid;
    (-1..=1)
        .flat_map(|dx| (-1..=1).map(move |dy| (npc.0 + dx, npc.1 + dy)))
        .filter(|&c| c != npc && grid.get(c).is_some_and(|cell| !cell.is_blocked()))
        .min_by_key(|&c| distance(c, from) * 100 + (c.0 - from.0).abs() + (c.1 - from.1).abs())
        .unwrap_or(npc)
}
