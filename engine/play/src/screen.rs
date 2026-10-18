//! The game on screen: a [`Session`] with its camera and the player's mouse and keys, laid
//! out in an egui rect. Hosts (the client window, the Studio's Game view) give it a rect each
//! frame and draw [`GameScreen::world`] with their renderer under the UI.
//!
//! Mouse: click to act (walk, attack, talk, pick up), drag to pan, wheel to zoom. Keys: R
//! walk/run, Tab attack nearest, Space follow the hero, F fit the map, Q/E turn, WASD/arrows
//! pan; B bag and Enter chat belong to the in-game UI.

use crate::session::{Screen, Session};
use crate::world::World;
use c3_game::EntityId;
use c3_render::camera::Camera2d;
use egui::{Key, PointerButton, Rect, Sense};
use glam::Vec2;

/// World pixels per second when panning with the keyboard, at zoom 1.
const PAN_SPEED: f32 = 900.0;
/// Longest frame the game advances by (a stall doesn't jump the game ahead).
pub const MAX_FRAME_SECONDS: f32 = 0.1;

pub struct GameScreen {
    pub session: Session,
    pub camera: Camera2d,
    /// The camera keeps the hero centred.
    pub follow: bool,
    /// Size of the game view last frame.
    viewport: Vec2,
}

impl GameScreen {
    pub fn new(session: Session) -> Self {
        let center = session.hero_position().unwrap_or_default();
        Self {
            session,
            camera: Camera2d { center, zoom: 1.0 },
            follow: true,
            viewport: Vec2::new(1280.0, 800.0),
        }
    }

    pub fn world(&mut self) -> &mut World {
        &mut self.session.world
    }

    pub fn viewport(&self) -> Vec2 {
        self.viewport
    }

    /// Takes this frame's mouse and keys over `rect` and runs `seconds` of game. Returns the
    /// monster under the pointer. Then the host draws the world and calls [`Self::overlay`].
    pub fn update(&mut self, ui: &egui::Ui, rect: Rect, seconds: f32) -> Option<EntityId> {
        let resp = ui.interact(
            rect,
            ui.scope_id().with("game view"),
            Sense::click_and_drag(),
        );
        self.viewport = Vec2::new(rect.width(), rect.height());
        self.pointer(ui, rect, &resp);
        self.keys(ui);
        self.session.advance(seconds, ui.input(|i| i.time));
        if self.follow
            && let Some(hero) = self.session.hero_position()
        {
            self.camera.center = hero;
        }
        let screen = Screen {
            rect,
            camera: &self.camera,
        };
        let hovered = resp
            .hover_pos()
            .and_then(|p| self.session.monster_at(screen.to_world(p)));
        if hovered.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        hovered
    }

    /// Draws the in-game UI over the world; `hovered` from [`Self::update`].
    pub fn overlay(&mut self, ui: &mut egui::Ui, rect: Rect, hovered: Option<EntityId>) {
        let screen = Screen {
            rect,
            camera: &self.camera,
        };
        self.session.ui(ui, &screen, hovered);
    }

    fn pointer(&mut self, ui: &egui::Ui, rect: Rect, resp: &egui::Response) {
        if [
            PointerButton::Primary,
            PointerButton::Secondary,
            PointerButton::Middle,
        ]
        .into_iter()
        .any(|b| resp.dragged_by(b))
        {
            let d = resp.drag_delta();
            self.camera.center -= Vec2::new(d.x, d.y) / self.camera.zoom;
            self.follow = false;
        }
        let screen = Screen {
            rect,
            camera: &self.camera,
        };
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let point = screen.to_world(p);
            self.session.click(point);
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let anchor = resp.hover_pos().map_or(self.viewport / 2.0, |p| {
                    Vec2::new(p.x - rect.left(), p.y - rect.top())
                });
                self.camera
                    .zoom_at((scroll * 0.002).exp(), anchor, self.viewport);
            }
        }
    }

    fn keys(&mut self, ui: &egui::Ui) {
        if self.session.typing() || ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        let (run, attack, follow, fit, turn, pan, dt) = ui.input(|i| {
            let mut pan = Vec2::ZERO;
            for (keys, dir) in [
                ([Key::A, Key::ArrowLeft], Vec2::NEG_X),
                ([Key::D, Key::ArrowRight], Vec2::X),
                ([Key::W, Key::ArrowUp], Vec2::NEG_Y),
                ([Key::S, Key::ArrowDown], Vec2::Y),
            ] {
                if keys.iter().any(|k| i.key_down(*k)) {
                    pan += dir;
                }
            }
            (
                i.key_pressed(Key::R),
                i.key_pressed(Key::Tab),
                i.key_pressed(Key::Space),
                i.key_pressed(Key::F),
                i8::from(i.key_pressed(Key::E)) - i8::from(i.key_pressed(Key::Q)),
                pan,
                i.stable_dt.min(MAX_FRAME_SECONDS),
            )
        });
        if run {
            self.session.toggle_running();
        }
        if attack {
            self.session.attack_nearest();
        }
        if follow {
            self.follow = true;
        }
        if fit {
            self.camera.fit(self.session.world.bounds(), self.viewport);
            self.follow = false;
        }
        if turn != 0
            && let Some(hero) = self.session.world.hero_mut()
        {
            hero.turn(turn);
        }
        if pan != Vec2::ZERO {
            self.follow = false;
            self.camera.center += pan.normalize_or_zero() * PAN_SPEED * dt / self.camera.zoom;
        }
    }
}
