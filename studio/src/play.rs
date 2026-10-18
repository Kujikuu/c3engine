//! Play mode: the open map played inside the Game view, like Unity's Play button. The world
//! is built from the map as it is now (unsaved edits included) and Stop drops it, so playing
//! never changes the map. Pause freezes game time; Step runs one frame.

use crate::doc::MapDoc;
use crate::gpu::GpuView;
use crate::theme;
use crate::tq_plugin::Client;
use c3_render::camera::Camera2d;
use c3_scene::{CellPos, path};
use eframe::egui::{self, Color32, PointerButton, Pos2, Rect, Sense};
use eframe::egui_wgpu;
use game_conquer::actor::Actor;
use game_conquer::character::{DEFAULT_HERO, action};
use game_conquer::world::World;
use glam::Vec2;

/// Game time one Step runs.
const STEP_SECONDS: f32 = 1.0 / 30.0;
/// Longest frame the game advances by (a stall doesn't jump the game ahead).
const MAX_FRAME_SECONDS: f32 = 0.1;
/// How far (cells) the hero may start from where the Scene view looks.
const START_SEARCH: i32 = 50;
/// World pixels per second when panning with the keyboard, at zoom 1.
const PAN_SPEED: f32 = 900.0;

/// A game being played in the Game view.
pub struct PlaySession {
    world: World,
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
    ) -> Result<Self, String> {
        let scene = doc.scene().clone();
        let cell = path::nearest_walkable(&scene.grid, start, START_SEARCH)
            .ok_or("there is no walkable cell near where the Scene view looks")?;
        let content = client.shared_content();
        let mut world = World::new(content, doc.name().to_owned(), scene);
        let hero = Actor::from_look(&world.content, &DEFAULT_HERO, action::STANDBY, cell, 0)?;
        world.actors.push(hero);
        world.add_npcs();
        world.hero_action(action::STANDBY, false);
        let center = Vec2::from(world.scene.cell_to_world(cell));
        Ok(Self {
            world,
            camera: Camera2d { center, zoom: 1.0 },
            follow: true,
            paused: false,
            step: false,
            gpu: GpuView::new(render_state),
            viewport: Vec2::new(1280.0, 800.0),
        })
    }

    /// Runs one frame of game time (while paused).
    pub fn step(&mut self) {
        self.step = true;
    }

    /// Whether the hero runs instead of walking.
    pub fn running(&self) -> bool {
        self.world.hero().is_some_and(|h| h.running)
    }

    /// Applies this frame's game keys.
    pub fn keys(&mut self, keys: &GameKeys) {
        if keys.run
            && let Some(hero) = self.world.hero_mut()
        {
            hero.running = !hero.running;
        }
        if keys.turn != 0
            && let Some(hero) = self.world.hero_mut()
        {
            hero.turn(keys.turn);
        }
        if keys.attack {
            self.world.hero_action(action::ATTACK_0, true);
        }
        if keys.follow {
            self.follow = self.world.hero().is_some();
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

    /// Draws the game into the Game view and handles the pointer: click walks, drag pans,
    /// the wheel zooms.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let viewport = Vec2::new(rect.width(), rect.height());
        self.viewport = viewport;
        let to_view = |p: Pos2| Vec2::new(p.x - rect.left(), p.y - rect.top());

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
            let target = self.camera.screen_to_world(to_view(p), viewport);
            self.world.walk_hero_to(target);
        }
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
        if !self.paused {
            self.world.advance(dt);
        } else if std::mem::take(&mut self.step) {
            self.world.advance(STEP_SECONDS);
        }
        self.world.tick();
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
        self.hud(&painter, rect);
        ui.ctx().request_repaint();
    }

    /// Controls and state over the game, like a game's own HUD hints.
    fn hud(&self, painter: &egui::Painter, rect: Rect) {
        let run = if self.running() { "run" } else { "walk" };
        let follow = if self.follow {
            ""
        } else {
            " · Space: follow hero"
        };
        let hint = format!(
            "Click: {run} there · R: walk/run · Enter: attack · Q/E: turn · Wheel: zoom{follow}"
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
