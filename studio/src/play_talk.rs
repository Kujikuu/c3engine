//! NPC dialogue in Play mode: clicking an NPC walks the hero up to it and runs its script;
//! the dialogue window shows the NPC's page and answers (number keys pick, Esc closes).
//! Scripts are read from disk on each talk, so saving a script changes the next dialogue.
//! What a script gives or takes lands on the hero in the simulation.

use crate::icons;
use crate::play_hud::{Hud, LogLine};
use crate::theme;
use c3_game::{EntityId, Sim, Task, distance};
use c3_scene::{CellPos, Npc};
use c3_script::{Conversation, Effect, NpcInfo, PlayerState, ScriptError, ScriptHost, Step};
use eframe::egui::{self, Color32, RichText};
use game_conquer::world::World;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Cells from the NPC within which the hero can talk.
const TALK_RANGE: i32 = 3;
const DIALOG_WIDTH: f32 = 420.0;

pub struct Talk {
    host: Option<ScriptHost>,
    /// Game map ids of the map being played (teleports within it work).
    map_ids: Vec<u32>,
    /// Script variables kept with the hero.
    flags: BTreeMap<String, i64>,
    conversation: Option<Conversation>,
    /// The NPC the hero walks up to before talking.
    approach: Option<(NpcInfo, String, CellPos)>,
}

impl Talk {
    /// Dialogues running the scripts in `scripts` (reading tables from `tables`).
    pub fn new(scripts: Option<PathBuf>, tables: Option<PathBuf>, map_ids: Vec<u32>) -> Self {
        Self {
            host: scripts.map(|dir| ScriptHost::new(dir, tables)),
            map_ids,
            flags: BTreeMap::new(),
            conversation: None,
            approach: None,
        }
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
    pub fn approach(&mut self, npc: &Npc, sim: &mut Sim, hero: EntityId, hud: &mut Hud, now: f64) {
        self.cancel();
        if npc.script.is_empty() {
            hud.message(
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
        let Some(from) = sim.player(hero).map(|p| p.cell) else {
            return;
        };
        if distance(from, npc.cell) > 1 {
            let goal = beside(sim, npc.cell, from);
            sim.walk(hero, goal);
        }
    }

    /// Opens the dialogue once the hero has walked up to the NPC.
    pub fn update(
        &mut self,
        sim: &mut Sim,
        hero: EntityId,
        world: &mut World,
        hud: &mut Hud,
        now: f64,
    ) {
        let Some(p) = sim.player(hero) else { return };
        if p.task != Task::Idle || self.approach.is_none() {
            return;
        }
        let (npc, script, cell) = self.approach.take().expect("checked above");
        if distance(p.cell, cell) > TALK_RANGE {
            hud.message(now, format!("Can't reach {}.", npc.name), theme::WARNING);
            return;
        }
        if let Some(actor) = world.hero_mut() {
            actor.face(cell);
        }
        let Some(host) = &mut self.host else { return };
        let mut state = player_state(sim, hero, &self.flags);
        let step = Conversation::start(host, &script, npc, &mut state);
        self.after_step(step, state, sim, hero, hud, now);
    }

    fn choose(&mut self, index: usize, sim: &mut Sim, hero: EntityId, hud: &mut Hud, now: f64) {
        let (Some(conversation), Some(host)) = (self.conversation.take(), &mut self.host) else {
            return;
        };
        let mut state = player_state(sim, hero, &self.flags);
        let step = conversation.choose(host, index, &mut state);
        self.after_step(step, state, sim, hero, hud, now);
    }

    /// Carries out a dialogue step: the hero takes the script's changes, the effects show.
    fn after_step(
        &mut self,
        step: Result<Step, ScriptError>,
        state: PlayerState,
        sim: &mut Sim,
        hero: EntityId,
        hud: &mut Hud,
        now: f64,
    ) {
        // Changes made before an error stay, as on a server.
        self.flags = state.flags.clone();
        apply_state(sim, hero, &state);
        match step {
            Ok((next, effects)) => {
                self.conversation = next;
                for effect in effects {
                    self.show(effect, sim, hero, hud, now);
                }
            }
            Err(e) => {
                hud.message(now, "Script error; see the Console.", theme::DANGER);
                hud.log(LogLine::Error(format!("Script error: {e}")));
            }
        }
    }

    fn show(&mut self, effect: Effect, sim: &mut Sim, hero: EntityId, hud: &mut Hud, now: f64) {
        let name = |kind: i64| Hud::item_name(sim, kind.try_into().unwrap_or(0));
        match effect {
            Effect::Message(text) => hud.message(now, text, theme::BRAND),
            Effect::GaveItem { item, count } => hud.message(
                now,
                format!("You received {count} × {}.", name(item)),
                theme::SUCCESS,
            ),
            Effect::TookItem { item, count } => hud.message(
                now,
                format!("You handed over {count} × {}.", name(item)),
                theme::TEXT,
            ),
            Effect::Money(n) if n >= 0 => {
                hud.message(now, format!("You received {n} silver."), theme::SUCCESS)
            }
            Effect::Money(n) => hud.message(now, format!("You paid {} silver.", -n), theme::TEXT),
            Effect::Teleport { map, cell } => {
                let here = map == 0 || self.map_ids.iter().any(|&id| i64::from(id) == map);
                let cell = (cell.0 as i32, cell.1 as i32);
                if here && sim.grid().get(cell).is_some_and(|c| !c.is_blocked()) {
                    self.conversation = None;
                    sim.place(hero, cell);
                } else {
                    hud.message(
                        now,
                        format!(
                            "Teleport to map {map} ({}, {}): Play mode runs only the open map.",
                            cell.0, cell.1
                        ),
                        theme::WARNING,
                    );
                }
            }
            Effect::Log(text) => hud.log(LogLine::Info(format!("Script: {text}"))),
        }
    }

    /// Draws the dialogue window over the game view and takes its answers.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        sim: &mut Sim,
        hero: EntityId,
        hud: &mut Hud,
    ) {
        let now = ui.input(|i| i.time);
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
            self.choose(i, sim, hero, hud, now);
        } else if close {
            self.conversation = None;
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

/// The hero as scripts see it.
fn player_state(sim: &Sim, hero: EntityId, flags: &BTreeMap<String, i64>) -> PlayerState {
    let Some(p) = sim.player(hero) else {
        return PlayerState::default();
    };
    let mut items = BTreeMap::new();
    for item in &p.bag {
        *items.entry(i64::from(item.kind)).or_insert(0) += 1;
    }
    PlayerState {
        name: p.name.clone(),
        level: p.level.into(),
        money: p.money as i64,
        items,
        flags: flags.clone(),
    }
}

/// Puts a script's changes to money and items on the hero.
fn apply_state(sim: &mut Sim, hero: EntityId, after: &PlayerState) {
    let before = player_state(sim, hero, &after.flags);
    if before.money != after.money {
        sim.set_money(hero, after.money.max(0) as u64);
    }
    let kinds: BTreeSet<i64> = before
        .items
        .keys()
        .chain(after.items.keys())
        .copied()
        .collect();
    for kind in kinds {
        let Ok(k) = u32::try_from(kind) else { continue };
        let change = after.item_count(kind) - before.item_count(kind);
        if change > 0 {
            for _ in 0..change {
                sim.give_item(hero, k);
            }
        } else if change < 0 {
            sim.take_items(hero, k, (-change) as usize);
        }
    }
}

/// The walkable cell next to `npc` nearest `from` (or the NPC's own cell).
fn beside(sim: &Sim, npc: CellPos, from: CellPos) -> CellPos {
    let grid = sim.grid();
    (-1..=1)
        .flat_map(|dx| (-1..=1).map(move |dy| (npc.0 + dx, npc.1 + dy)))
        .filter(|&c| c != npc && grid.get(c).is_some_and(|cell| !cell.is_blocked()))
        .min_by_key(|&c| distance(c, from) * 100 + (c.0 - from.0).abs() + (c.1 - from.1).abs())
        .unwrap_or(npc)
}
