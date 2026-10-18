//! Play mode: the open map played inside the Game view, like Unity's Play button. The game
//! is built from the map as it is now (unsaved edits included) and Stop drops it, so playing
//! never changes the map. Pause freezes game time; Step runs one frame.
//!
//! The game runs in the engine's simulation (`c3_game::Sim`) with the project's rules: the
//! hero walks, fights the monsters of the map's spawn areas, picks up drops and talks to NPCs
//! (see `play_talk`); `SimView` shows it in the world and `play_hud` draws the feedback.

use crate::doc::MapDoc;
use crate::gpu::GpuView;
use crate::play_hud::{Game, Hud, LogLine, Screen};
use crate::play_talk::Talk;
use crate::theme;
use crate::tq_plugin::Client;
use c3_game::{EntityId, Sim};
use c3_render::camera::Camera2d;
use c3_scene::{CellPos, path};
use eframe::egui::{self, Color32, PointerButton, Pos2, Rect, Sense};
use eframe::egui_wgpu;
use game_conquer::character::DEFAULT_HERO;
use game_conquer::sim_view::SimView;
use game_conquer::world::World;
use glam::Vec2;
use std::path::PathBuf;

/// Game time one Step runs.
const STEP_SECONDS: f32 = 1.0 / 30.0;
/// Longest frame the game advances by (a stall doesn't jump the game ahead).
const MAX_FRAME_SECONDS: f32 = 0.1;
/// How far (cells) the hero may start from where the Scene view looks.
const START_SEARCH: i32 = 50;
/// World pixels per second when panning with the keyboard, at zoom 1.
const PAN_SPEED: f32 = 900.0;
/// Money the hero starts with, to try shops.
const START_MONEY: u64 = 1000;
/// Cells within which Enter picks a monster to attack.
const ATTACK_SEARCH: i32 = 10;

/// What a play session reads from the project.
pub struct PlaySetup {
    pub scripts: Option<PathBuf>,
    pub tables: Option<PathBuf>,
    /// Game map ids of the map being played (script teleports within it work).
    pub map_ids: Vec<u32>,
}

/// A game being played in the Game view.
pub struct PlaySession {
    sim: Sim,
    hero: EntityId,
    world: World,
    view: SimView,
    camera: Camera2d,
    /// The camera keeps the hero centred.
    follow: bool,
    pub paused: bool,
    /// Run one frame while paused.
    step: bool,
    /// The game's own renderer: its textures live there.
    gpu: GpuView,
    /// Size of the Game view last frame.
    viewport: Vec2,
    talk: Talk,
    hud: Hud,
}

/// The game keys of one frame (read by the Studio's shortcut handling).
#[derive(Default)]
pub struct GameKeys {
    pub run: bool,
    pub attack: bool,
    pub follow: bool,
    pub fit: bool,
    pub turn: i8,
    /// Keyboard panning direction.
    pub pan: Vec2,
    pub dt: f32,
}

impl PlaySession {
    /// Starts playing `doc` as it is now, the hero on the walkable cell nearest `start`.
    pub fn start(
        client: &Client,
        doc: &MapDoc,
        start: CellPos,
        render_state: &egui_wgpu::RenderState,
        setup: PlaySetup,
    ) -> Result<Self, String> {
        let scene = doc.scene().clone();
        let cell = path::nearest_walkable(&scene.grid, start, START_SEARCH)
            .ok_or("there is no walkable cell near where the Scene view looks")?;
        let mut hud = Hud::default();
        let (rules, problems) = client.game_rules(setup.tables.as_deref());
        for p in problems {
            hud.log(LogLine::Error(format!("Game rules: {p}")));
        }
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_secs());
        let mut sim = Sim::new(
            scene.grid.clone(),
            scene.npcs.clone(),
            scene.spawns.clone(),
            rules,
            seed,
        );
        let hero = sim.add_player("Hero", cell, 1);
        sim.set_money(hero, START_MONEY);
        // The hero wears what it is drawn in.
        for kind in [
            DEFAULT_HERO.armor,
            DEFAULT_HERO.armet,
            DEFAULT_HERO.r_weapon,
            DEFAULT_HERO.l_weapon,
        ] {
            sim.equip(hero, kind);
        }
        // Monsters born at the start are already in the view.
        let _ = sim.take_events();
        let mut world = World::new(client.shared_content(), doc.name().to_owned(), scene);
        let view = SimView::attach(&sim, hero, &mut world)?;
        let center = Vec2::from(world.scene.cell_to_world(cell));
        Ok(Self {
            sim,
            hero,
            world,
            view,
            camera: Camera2d { center, zoom: 1.0 },
            follow: true,
            paused: false,
            step: false,
            gpu: GpuView::new(render_state),
            viewport: Vec2::new(1280.0, 800.0),
            talk: Talk::new(setup.scripts, setup.tables, setup.map_ids),
            hud,
        })
    }

    /// Walks the hero up to NPC `id` and talks to it, as if it was clicked.
    pub fn talk_to(&mut self, id: u32) {
        if let Some(npc) = self.sim.npcs().iter().find(|n| n.id == id).cloned() {
            self.talk
                .approach(&npc, &mut self.sim, self.hero, &mut self.hud, 0.0);
        }
    }

    /// Sends the hero after the nearest monster it can reach, as Enter does.
    pub fn attack_nearest(&mut self) {
        if let Some(target) = self.sim.nearest_monster(self.hero, ATTACK_SEARCH) {
            self.talk.cancel();
            self.sim.attack(self.hero, target);
        }
    }

    /// Lines for the Studio's console since the last call.
    pub fn take_log(&mut self) -> Vec<LogLine> {
        self.hud.take_log()
    }

    /// Runs one frame of game time (while paused).
    pub fn step(&mut self) {
        self.step = true;
    }

    /// Whether the hero runs instead of walking.
    pub fn running(&self) -> bool {
        self.sim.player(self.hero).is_some_and(|p| p.running)
    }

    /// Applies this frame's game keys.
    pub fn keys(&mut self, keys: &GameKeys) {
        if keys.run {
            let running = self.running();
            self.sim.set_running(self.hero, !running);
        }
        if keys.turn != 0
            && let Some(hero) = self.world.hero_mut()
        {
            hero.turn(keys.turn);
        }
        // Enter belongs to the dialogue while one is open.
        if keys.attack && !self.talk.is_talking() {
            self.attack_nearest();
        }
        if keys.follow {
            self.follow = true;
        }
        if keys.fit {
            self.camera.fit(self.world.bounds(), self.viewport);
            self.follow = false;
        }
        if keys.pan != Vec2::ZERO {
            self.follow = false;
            self.camera.center +=
                keys.pan.normalize_or_zero() * PAN_SPEED * keys.dt / self.camera.zoom;
        }
    }

    /// A click on world point `target`: talk to an NPC, attack a monster, pick up a drop, or
    /// walk there.
    fn click(&mut self, target: Vec2, now: f64) {
        if let Some(npc) = self.world.npc_at(target).cloned() {
            self.talk
                .approach(&npc, &mut self.sim, self.hero, &mut self.hud, now);
            return;
        }
        self.talk.cancel();
        if let Some(monster) = self.view.monster_at(&self.sim, &self.world, target) {
            self.sim.attack(self.hero, monster);
        } else if let Some(item) = self.view.item_at(&self.sim, &self.world, target) {
            self.sim.pick_up(self.hero, item);
        } else {
            let cell = self.world.scene.world_to_cell((target.x, target.y));
            self.sim.walk(self.hero, cell);
            if self
                .world
                .scene
                .grid
                .get(cell)
                .is_some_and(|c| !c.is_blocked())
            {
                // The click marker is decoration: a client without it still walks.
                let _ = self.world.place_effect("mouse", cell, false);
            }
        }
    }

    /// Draws the game into the Game view and handles the pointer: click acts, drag pans,
    /// the wheel zooms.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let viewport = Vec2::new(rect.width(), rect.height());
        self.viewport = viewport;
        let to_view = |p: Pos2| Vec2::new(p.x - rect.left(), p.y - rect.top());
        let now = ui.input(|i| i.time);

        if resp.dragged_by(PointerButton::Primary)
            || resp.dragged_by(PointerButton::Secondary)
            || resp.dragged_by(PointerButton::Middle)
        {
            let d = resp.drag_delta();
            self.camera.center -= Vec2::new(d.x, d.y) / self.camera.zoom;
            self.follow = false;
        }
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            self.click(self.camera.screen_to_world(to_view(p), viewport), now);
        }
        let hovered = resp
            .hover_pos()
            .map(|p| self.camera.screen_to_world(to_view(p), viewport))
            .and_then(|w| self.view.monster_at(&self.sim, &self.world, w));
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let anchor = ui
                    .input(|i| i.pointer.hover_pos())
                    .map_or(viewport / 2.0, to_view);
                self.camera
                    .zoom_at((scroll * 0.002).exp(), anchor, viewport);
            }
        }

        let dt = ui.input(|i| i.stable_dt).min(MAX_FRAME_SECONDS);
        let run = if !self.paused {
            dt
        } else if std::mem::take(&mut self.step) {
            STEP_SECONDS
        } else {
            0.0
        };
        self.advance(run, now);
        if self.follow
            && let Some(hero) = self.world.hero()
        {
            self.camera.center = hero.position(&self.world.scene, self.world.now_ms());
        }

        let ppp = ui.ctx().pixels_per_point();
        let size = [
            (rect.width() * ppp).round() as u32,
            (rect.height() * ppp).round() as u32,
        ];
        self.world
            .prepare(&mut self.gpu.renderer, &self.camera, viewport);
        let meshes = self.world.mesh_draws();
        let frame = self.world.frame(&meshes, self.camera.view_rect(viewport));
        let texture = self.gpu.render(size, &frame);
        let painter = ui.painter_at(rect);
        painter.image(
            texture,
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
        let screen = Screen {
            rect,
            camera: &self.camera,
            viewport,
        };
        let game = Game {
            sim: &self.sim,
            hero: self.hero,
            world: &self.world,
            view: &self.view,
        };
        self.hud.draw(ui, &screen, &game, hovered);
        self.hints(&painter, rect);
        self.talk
            .ui(ui, rect, &mut self.sim, self.hero, &mut self.hud);
        if hovered.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        ui.ctx().request_repaint();
    }

    /// Runs `seconds` of game: the simulation, then the world showing it.
    fn advance(&mut self, seconds: f32, now: f64) {
        self.sim.advance((seconds * 1000.0).round() as u64);
        let events = self.sim.take_events();
        self.view.apply(&events, &self.sim, &mut self.world);
        self.hud.events(&events, &self.sim, self.hero, now);
        self.world.advance(seconds);
        self.world.tick();
        self.talk.update(
            &mut self.sim,
            self.hero,
            &mut self.world,
            &mut self.hud,
            now,
        );
    }

    /// Controls over the game, like a game's own HUD hints.
    fn hints(&self, painter: &egui::Painter, rect: Rect) {
        let run = if self.running() { "run" } else { "walk" };
        let follow = if self.follow {
            ""
        } else {
            " · Space: follow hero"
        };
        let hint = format!(
            "Click: {run} · Click a monster: attack · Click an NPC: talk · Click a drop: pick up · \
             R: walk/run · Enter: attack nearest · Wheel: zoom{follow}"
        );
        let font = egui::FontId::proportional(12.0);
        let galley = painter.layout_no_wrap(hint, font, theme::TEXT);
        let pad = egui::vec2(8.0, 5.0);
        let at = rect.left_top() + egui::vec2(10.0, 10.0);
        painter.rect_filled(
            Rect::from_min_size(at, galley.size() + pad * 2.0),
            4.0,
            Color32::from_black_alpha(150),
        );
        painter.galley(at + pad, galley, theme::TEXT);
        if self.paused {
            let galley = painter.layout_no_wrap(
                "Paused".into(),
                egui::FontId::proportional(14.0),
                Color32::WHITE,
            );
            let at = rect.right_top() + egui::vec2(-galley.size().x - 26.0, 10.0);
            painter.rect_filled(
                Rect::from_min_size(at, galley.size() + pad * 2.0),
                4.0,
                theme::WARNING.gamma_multiply(0.8),
            );
            painter.galley(at + pad, galley, Color32::WHITE);
        }
    }
}
