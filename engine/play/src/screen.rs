//! The game on screen: a [`Session`] with its camera, cursors and the player's mouse and keys,
//! laid out in an egui rect. Hosts (the client window, the Studio's Game view) give it a rect
//! each frame and draw [`GameScreen::world`] with [`GameScreen::view_camera`] under the UI.
//! The camera follows Project Settings > Camera; the cursors Project Settings > Cursors.
//!
//! Mouse: click to act (walk, attack, talk, pick up), drag to pan, wheel to zoom. Keys: R
//! walk/run, Tab attack nearest, Space follow the hero, F fit the map, Q/E turn, WASD/arrows
//! pan; B bag and Enter chat belong to the in-game UI.

use crate::cursor::{CursorKind, Cursors};
use crate::session::{Screen, Session};
use crate::world::World;
use c3_core::ProjectSettings;
use c3_core::settings::CameraSettings;
use c3_game::EntityId;
use c3_render::camera::Camera2d;
use egui::{Key, PointerButton, Rect, Sense};
use glam::Vec2;

/// Longest frame the game advances by (a stall doesn't jump the game ahead).
pub const MAX_FRAME_SECONDS: f32 = 0.1;
/// How long the view shakes after the hero is hit (seconds).
const SHAKE_SECONDS: f64 = 0.3;
/// Wheel zoom per scrolled point, at wheel speed 1.
const WHEEL_ZOOM: f32 = 0.002;

pub struct GameScreen {
    pub session: Session,
    pub camera: Camera2d,
    /// The camera keeps the hero in view.
    pub follow: bool,
    /// Size of the game view last frame.
    viewport: Vec2,
    cursors: Cursors,
    /// The pointer is over the world (not over a game window) this frame.
    over_world: bool,
    /// The view's shake offset this frame (screen pixels).
    shake: Vec2,
}

impl GameScreen {
    pub fn new(session: Session) -> Self {
        let center = session.hero_position().unwrap_or_default();
        let zoom = session.settings().camera.zoom as f32;
        let (cursors, problems) = Cursors::load(&*session.world.game, &session.settings().cursors);
        let mut screen = Self {
            session,
            camera: Camera2d { center, zoom },
            follow: true,
            viewport: Vec2::new(1280.0, 800.0),
            cursors,
            over_world: false,
            shake: Vec2::ZERO,
        };
        screen.session.report(problems);
        screen
    }

    pub fn world(&mut self) -> &mut World {
        &mut self.session.world
    }

    pub fn viewport(&self) -> Vec2 {
        self.viewport
    }

    /// The camera to draw the world with this frame (shaken just after the hero is hit).
    pub fn view_camera(&self) -> Camera2d {
        Camera2d {
            center: self.camera.center + self.shake / self.camera.zoom,
            zoom: self.camera.zoom,
        }
    }

    /// Plays by `settings` from now on: the session's, the camera's and the cursors.
    pub fn set_settings(&mut self, settings: ProjectSettings) {
        if &settings.cursors != self.cursors.settings() {
            let (cursors, problems) = Cursors::load(&*self.session.world.game, &settings.cursors);
            self.cursors = cursors;
            self.session.report(problems);
        }
        let c = &settings.camera;
        self.camera.zoom = self.camera.zoom.clamp(c.min_zoom as f32, c.max_zoom as f32);
        if !c.free_pan {
            self.follow = true;
        }
        self.session.set_settings(settings);
    }

    fn camera_settings(&self) -> &CameraSettings {
        &self.session.settings().camera
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
        let now = ui.input(|i| i.time);
        self.session.advance(seconds, now);
        self.follow_hero(seconds);
        self.clamp_to_map();
        self.shake = self.shake_at(now);
        let screen = Screen {
            rect,
            camera: &self.camera,
        };
        let point = resp.hover_pos().map(|p| screen.to_world(p));
        let hovered = point.and_then(|p| self.session.monster_at(p));
        self.over_world = resp.hovered();
        if let Some(p) = point {
            let kind = match hovered {
                Some(_) => CursorKind::Attack,
                None => self.session.hover(p),
            };
            self.cursors.show(ui.ctx(), kind, resp.hover_pos(), now);
        }
        hovered
    }

    /// Draws the in-game UI over the world; `hovered` from [`Self::update`].
    pub fn overlay(&mut self, ui: &mut egui::Ui, rect: Rect, hovered: Option<EntityId>) {
        let camera = self.view_camera();
        let screen = Screen {
            rect,
            camera: &camera,
        };
        self.session.ui(ui, &screen, hovered);
        // Over the game's own windows its arrow still shows.
        let pointer = ui.ctx().pointer_hover_pos().filter(|p| rect.contains(*p));
        if pointer.is_some() && !self.over_world {
            let now = ui.input(|i| i.time);
            self.cursors
                .show(ui.ctx(), CursorKind::Normal, pointer, now);
        }
    }

    /// Keeps the hero in view: locked on, or eased towards it, once it leaves the dead zone.
    fn follow_hero(&mut self, seconds: f32) {
        let Some(hero) = self.session.hero_position() else {
            return;
        };
        if !self.follow {
            return;
        }
        let c = self.camera_settings();
        let (dead_zone, smoothing) = (c.dead_zone as f32, c.follow_smoothing as f32);
        let offset = (hero - self.camera.center) * self.camera.zoom;
        let excess = offset.length() - dead_zone;
        if excess <= 0.0 {
            return;
        }
        let target = self.camera.center + offset.normalize_or_zero() * excess / self.camera.zoom;
        self.camera.center = if smoothing <= 0.0 {
            target
        } else {
            let t = 1.0 - (-seconds / smoothing).exp();
            self.camera.center.lerp(target, t)
        };
    }

    /// Keeps the view inside the map when the settings ask for it.
    fn clamp_to_map(&mut self) {
        if !self.camera_settings().clamp_to_map {
            return;
        }
        let [x, y, w, h] = self.session.world.bounds();
        let half = self.viewport / (2.0 * self.camera.zoom);
        let axis = |center: f32, min: f32, size: f32, half: f32| {
            if size <= half * 2.0 {
                min + size / 2.0
            } else {
                center.clamp(min + half, min + size - half)
            }
        };
        self.camera.center = Vec2::new(
            axis(self.camera.center.x, x, w, half.x),
            axis(self.camera.center.y, y, h, half.y),
        );
    }

    /// The view's shake offset at `now`: a fading wobble just after the hero is hit.
    fn shake_at(&self, now: f64) -> Vec2 {
        let strength = self.camera_settings().shake as f32;
        match self.session.hit_at() {
            Some(at) if strength > 0.0 && now - at < SHAKE_SECONDS => {
                let t = (now - at) as f32;
                let fade = 1.0 - t / SHAKE_SECONDS as f32;
                Vec2::new((t * 90.0).sin(), (t * 70.0).cos()) * strength * fade
            }
            _ => Vec2::ZERO,
        }
    }

    /// Zooms by `factor` about `anchor` (screen pixels), within the settings' limits.
    fn zoom_by(&mut self, factor: f32, anchor: Vec2) {
        let c = self.camera_settings();
        let (min, max) = (c.min_zoom as f32, c.max_zoom as f32);
        let wanted = (self.camera.zoom * factor).clamp(min, max);
        self.camera
            .zoom_at(wanted / self.camera.zoom, anchor, self.viewport);
    }

    fn pointer(&mut self, ui: &egui::Ui, rect: Rect, resp: &egui::Response) {
        let free_pan = self.camera_settings().free_pan;
        if free_pan
            && [
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
            let speed = self.camera_settings().wheel_speed as f32;
            if scroll != 0.0 && speed > 0.0 {
                let anchor = resp.hover_pos().map_or(self.viewport / 2.0, |p| {
                    Vec2::new(p.x - rect.left(), p.y - rect.top())
                });
                self.zoom_by((scroll * WHEEL_ZOOM * speed).exp(), anchor);
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
        let c = self.camera_settings();
        let (free_pan, pan_speed) = (c.free_pan, c.pan_speed as f32);
        let (min_zoom, max_zoom) = (c.min_zoom as f32, c.max_zoom as f32);
        if run {
            self.session.toggle_running();
        }
        if attack {
            self.session.attack_nearest();
        }
        if follow {
            self.follow = true;
        }
        if fit && free_pan {
            self.camera.fit(self.session.world.bounds(), self.viewport);
            self.camera.zoom = self.camera.zoom.clamp(min_zoom, max_zoom);
            self.follow = false;
        }
        if turn != 0
            && let Some(hero) = self.session.world.hero_mut()
        {
            hero.turn(turn);
        }
        if pan != Vec2::ZERO && free_pan {
            self.follow = false;
            self.camera.center += pan.normalize_or_zero() * pan_speed * dt / self.camera.zoom;
        }
    }
}
