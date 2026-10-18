//! What Play mode draws over the game: the hero's life and purse, game messages, damage
//! numbers, names and life of monsters under the pointer or being fought, and labels on
//! drops. It turns the simulation's events into that feedback.

use crate::icons;
use crate::theme;
use c3_game::{EntityId, Event, Sim, Task};
use c3_render::camera::Camera2d;
use eframe::egui::{self, Color32, FontId, Pos2, Rect};
use game_conquer::sim_view::SimView;
use game_conquer::world::World;
use glam::Vec2;

/// Seconds a game message stays on screen.
const MESSAGE_SECONDS: f64 = 8.0;
/// Most game messages shown at once.
const MAX_MESSAGES: usize = 6;
/// Seconds a damage number rises before it is gone.
const FLOAT_SECONDS: f64 = 1.0;
/// World pixels above an actor's feet where its name and numbers go.
const HEAD_HEIGHT: f32 = 110.0;
const MONEY: Color32 = Color32::from_rgb(240, 200, 90);
const LIFE: Color32 = Color32::from_rgb(200, 60, 60);

/// A line for the Studio's console.
pub enum LogLine {
    Info(String),
    Error(String),
}

/// A number rising from an entity (damage, a miss).
struct Float {
    at: f64,
    entity: EntityId,
    text: String,
    color: Color32,
}

#[derive(Default)]
pub struct Hud {
    /// Game messages: when shown (seconds), text, colour.
    messages: Vec<(f64, String, Color32)>,
    floats: Vec<Float>,
    log: Vec<LogLine>,
}

/// The game being shown: the simulation, the hero, and the world drawing it.
#[derive(Clone, Copy)]
pub struct Game<'a> {
    pub sim: &'a Sim,
    pub hero: EntityId,
    pub world: &'a World,
    pub view: &'a SimView,
}

/// Where the game is drawn and how the world maps onto it.
pub struct Screen<'a> {
    pub rect: Rect,
    pub camera: &'a Camera2d,
    pub viewport: Vec2,
}

impl Screen<'_> {
    fn to_screen(&self, world: Vec2) -> Pos2 {
        let p = self.camera.world_to_screen(world, self.viewport);
        self.rect.min + egui::vec2(p.x, p.y)
    }
}

impl Hud {
    pub fn message(&mut self, now: f64, text: impl Into<String>, color: Color32) {
        self.messages.push((now, text.into(), color));
        if self.messages.len() > MAX_MESSAGES {
            self.messages.remove(0);
        }
    }

    pub fn log(&mut self, line: LogLine) {
        self.log.push(line);
    }

    /// Lines for the console since the last call.
    pub fn take_log(&mut self) -> Vec<LogLine> {
        std::mem::take(&mut self.log)
    }

    /// The name of item type `kind`.
    pub fn item_name(sim: &Sim, kind: u32) -> String {
        sim.rules()
            .item(kind)
            .map_or_else(|| format!("item {kind}"), |t| t.name.clone())
    }

    /// Feedback for what the simulation did.
    pub fn events(&mut self, events: &[Event], sim: &Sim, hero: EntityId, now: f64) {
        for event in events {
            match event {
                Event::Attacked {
                    attacker,
                    target,
                    damage,
                } => {
                    let (text, color) = match damage {
                        0 => ("Miss".to_owned(), theme::TEXT_DIM),
                        d if *target == hero => (format!("-{d}"), LIFE),
                        d if *attacker == hero => (d.to_string(), Color32::WHITE),
                        d => (d.to_string(), theme::TEXT),
                    };
                    self.floats.push(Float {
                        at: now,
                        entity: *target,
                        text,
                        color,
                    });
                }
                Event::Died { id, .. } if *id == hero => self.message(
                    now,
                    format!(
                        "You died. You come back in {} seconds.",
                        c3_game::REVIVE_MS / 1000
                    ),
                    theme::DANGER,
                ),
                Event::Revived(id) if *id == hero => {
                    self.message(now, "You are back.", theme::SUCCESS)
                }
                Event::LeveledUp { player, level } if *player == hero => self.message(
                    now,
                    format!("Level up! You are level {level}."),
                    theme::BRAND,
                ),
                Event::PickedUp {
                    player,
                    money,
                    kind,
                } if *player == hero => {
                    let text = match kind {
                        Some(k) => format!("You picked up {}.", Self::item_name(sim, *k)),
                        None => format!("You picked up {money} silver."),
                    };
                    self.message(now, text, theme::SUCCESS);
                }
                Event::Bought {
                    player,
                    kind,
                    price,
                } if *player == hero => self.message(
                    now,
                    format!(
                        "You bought {} for {price} silver.",
                        Self::item_name(sim, *kind)
                    ),
                    theme::TEXT,
                ),
                Event::Sold {
                    player,
                    kind,
                    price,
                } if *player == hero => self.message(
                    now,
                    format!(
                        "You sold {} for {price} silver.",
                        Self::item_name(sim, *kind)
                    ),
                    theme::TEXT,
                ),
                Event::Used { player, kind } if *player == hero => self.message(
                    now,
                    format!("You used {}.", Self::item_name(sim, *kind)),
                    theme::TEXT,
                ),
                Event::Message { player, text } if *player == hero => {
                    self.message(now, text.clone(), theme::WARNING)
                }
                Event::Warning(text) => self.log(LogLine::Error(text.clone())),
                _ => {}
            }
        }
    }

    /// Draws the feedback; `hovered` is the monster under the pointer.
    pub fn draw(&mut self, ui: &egui::Ui, screen: &Screen, game: &Game, hovered: Option<EntityId>) {
        let Game {
            sim,
            hero,
            world,
            view,
        } = *game;
        let now = ui.input(|i| i.time);
        self.messages.retain(|m| now - m.0 < MESSAGE_SECONDS);
        self.floats.retain(|f| now - f.at < FLOAT_SECONDS);
        let painter = ui.painter_at(screen.rect);
        let head = |id: EntityId| -> Option<Pos2> {
            let actor = world.actors.get(view.actor(id)?)?;
            let feet = actor.position(&world.scene, world.now_ms());
            Some(screen.to_screen(feet - Vec2::Y * HEAD_HEIGHT))
        };
        drop_labels(&painter, screen, sim, world);
        // The monster being fought and the one under the pointer show name and life.
        let fighting = sim.player(hero).and_then(|p| match p.task {
            Task::Attack(t) => Some(t),
            _ => None,
        });
        for id in [fighting, hovered].into_iter().flatten() {
            if let (Some(m), Some(at)) = (sim.monster(id).filter(|m| m.is_alive()), head(id)) {
                let name = sim.rules().monster(m.kind).map_or("?", |t| t.name.as_str());
                name_plate(
                    &painter,
                    at,
                    &format!("{name}  Lv {}", m.level),
                    m.life,
                    m.stats.max_life,
                );
            }
        }
        for f in &self.floats {
            let Some(at) = head(f.entity) else { continue };
            let t = ((now - f.at) / FLOAT_SECONDS) as f32;
            let pos = at + egui::vec2(0.0, -24.0 - 30.0 * t);
            painter.text(
                pos,
                egui::Align2::CENTER_BOTTOM,
                &f.text,
                FontId::proportional(17.0),
                f.color.gamma_multiply(1.0 - t * t),
            );
        }
        if let Some(p) = sim.player(hero) {
            status(&painter, screen.rect, p);
        }
        self.message_list(&painter, screen.rect, now);
    }

    fn message_list(&self, painter: &egui::Painter, rect: Rect, now: f64) {
        let mut y = rect.bottom() - 14.0;
        for (at, text, color) in self.messages.iter().rev() {
            // Fade out over the last second.
            let alpha = (MESSAGE_SECONDS - (now - at)).clamp(0.0, 1.0) as f32;
            let galley = painter.layout_no_wrap(
                text.clone(),
                FontId::proportional(13.0),
                color.gamma_multiply(alpha),
            );
            y -= galley.size().y + 4.0;
            let pos = egui::pos2(rect.left() + 14.0, y);
            painter.rect_filled(
                Rect::from_min_size(
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

/// A name with a life bar under it, centred above `at`.
fn name_plate(painter: &egui::Painter, at: Pos2, name: &str, life: u32, max: u32) {
    let galley =
        painter.layout_no_wrap(name.to_owned(), FontId::proportional(12.0), Color32::WHITE);
    let width = galley.size().x.max(70.0);
    let top = at - egui::vec2(width / 2.0, galley.size().y + 10.0);
    painter.rect_filled(
        Rect::from_min_size(
            top - egui::vec2(5.0, 3.0),
            egui::vec2(width + 10.0, galley.size().y + 14.0),
        ),
        3.0,
        Color32::from_black_alpha(150),
    );
    painter.galley(
        top + egui::vec2((width - galley.size().x) / 2.0, 0.0),
        galley,
        Color32::WHITE,
    );
    bar(
        painter,
        Rect::from_min_size(at - egui::vec2(width / 2.0, 6.0), egui::vec2(width, 4.0)),
        life,
        max,
        LIFE,
    );
}

fn bar(painter: &egui::Painter, rect: Rect, value: u32, max: u32, color: Color32) {
    painter.rect_filled(rect, 1.0, Color32::from_gray(40));
    let fill = if max == 0 {
        0.0
    } else {
        value as f32 / max as f32
    };
    let mut full = rect;
    full.set_width(rect.width() * fill.clamp(0.0, 1.0));
    painter.rect_filled(full, 1.0, color);
}

/// The hero's life, level, money and bag, at the bottom right.
fn status(painter: &egui::Painter, rect: Rect, p: &c3_game::Player) {
    let text = format!(
        "{}  Lv {}   {}  {} silver   {}  {}/{}",
        icons::USER,
        p.level,
        icons::COINS,
        p.money,
        icons::PACKAGE,
        p.bag.len(),
        c3_game::BAG_SIZE
    );
    let galley = painter.layout_no_wrap(text, FontId::proportional(13.0), theme::TEXT);
    let pad = egui::vec2(10.0, 6.0);
    let size = egui::vec2(galley.size().x.max(220.0), galley.size().y + 16.0) + pad * 2.0;
    let at = rect.right_bottom() - size - egui::vec2(10.0, 10.0);
    painter.rect_filled(
        Rect::from_min_size(at, size),
        4.0,
        Color32::from_black_alpha(160),
    );
    painter.galley(at + pad, galley, theme::TEXT);
    let bar_rect = Rect::from_min_size(
        at + pad + egui::vec2(0.0, size.y - pad.y * 2.0 - 8.0),
        egui::vec2(size.x - pad.x * 2.0, 8.0),
    );
    bar(painter, bar_rect, p.life, p.stats.max_life, LIFE);
    painter.text(
        bar_rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{}/{}", p.life, p.stats.max_life),
        FontId::proportional(9.0),
        Color32::WHITE,
    );
}

/// A marker and label on each drop in view.
fn drop_labels(painter: &egui::Painter, screen: &Screen, sim: &Sim, world: &World) {
    for item in sim.ground_items() {
        let at = screen.to_screen(Vec2::from(world.scene.cell_to_world(item.cell)));
        if !screen.rect.contains(at) {
            continue;
        }
        let (text, color) = match item.kind {
            Some(k) => (Hud::item_name(sim, k), Color32::from_rgb(140, 200, 255)),
            None => (format!("{} silver", item.money), MONEY),
        };
        painter.circle_filled(at, 4.0, color);
        painter.circle_stroke(at, 4.0, egui::Stroke::new(1.0, Color32::BLACK));
        let galley = painter.layout_no_wrap(text, FontId::proportional(11.0), color);
        let pos = at + egui::vec2(-galley.size().x / 2.0, -galley.size().y - 7.0);
        painter.rect_filled(
            Rect::from_min_size(
                pos - egui::vec2(3.0, 1.0),
                galley.size() + egui::vec2(6.0, 2.0),
            ),
            2.0,
            Color32::from_black_alpha(140),
        );
        painter.galley(pos, galley, color);
    }
}
