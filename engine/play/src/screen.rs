//! The game on screen: a [`Session`] with its camera, cursors and controls, laid out in an
//! egui rect. Hosts (the client window, the Studio's Game view) call [`GameScreen::before_frame`]
//! with the frame's raw input, give it a rect each frame and draw [`GameScreen::world`] with
//! [`GameScreen::view_camera`] under the UI.
//!
//! What the player does arrives as actions (Project Settings > Input, then the player's own
//! rebinds): keyboard and mouse, gamepads and touch alike. The camera follows Project Settings
//! > Camera; the cursors Project Settings > Cursors.

use crate::controls::PlayerControls;
use crate::cursor::{CursorKind, Cursors};
use crate::input::{
    ActionFrame, Bindings, DeviceFrame, Gamepads, InputMapper, KeyState, PadButton, PadState,
    pad_pressed,
};
use crate::session::{Screen, Session};
use crate::touch::TouchOverlay;
use crate::world::World;
use c3_core::ProjectSettings;
use c3_core::input::{Action, ActionKind, Binding, Device, MouseControl, PadControl, TouchControl};
use c3_core::settings::CameraSettings;
use c3_game::EntityId;
use c3_render::camera::Camera2d;
use c3_ui::{ControlRow, ControlsView, UiAction};
use egui::{Key, PointerButton, Rect, Sense};
use glam::Vec2;

/// Longest frame the game advances by (a stall doesn't jump the game ahead).
pub const MAX_FRAME_SECONDS: f32 = 0.1;
/// How long the view shakes after the hero is hit (seconds).
const SHAKE_SECONDS: f64 = 0.3;
/// How far a stick must lean to move between menu buttons.
const NAV_LEAN: f32 = 0.6;

/// What the hint line names, in order, with what the action does.
const HINTS: [(Action, &str); 11] = [
    (Action::Click, "act"),
    (Action::Move, "move"),
    (Action::Interact, "interact"),
    (Action::AttackNearest, "attack nearest"),
    (Action::NextTarget, "target"),
    (Action::ToggleRun, "walk/run"),
    (Action::Bag, "bag"),
    (Action::Quests, "quests"),
    (Action::Chat, "chat"),
    (Action::Zoom, "zoom"),
    (Action::Menu, "menu"),
];

pub struct GameScreen {
    pub session: Session,
    pub camera: Camera2d,
    /// The camera keeps the hero in view.
    pub follow: bool,
    /// The Studio shows the touch controls to try them with the mouse.
    pub simulate_touch: bool,
    /// Size of the game view last frame.
    viewport: Vec2,
    cursors: Cursors,
    /// The pointer is over the world (not over a game window) this frame.
    over_world: bool,
    /// The view's shake offset this frame (screen pixels).
    shake: Vec2,
    mapper: InputMapper,
    gamepads: Gamepads,
    /// The gamepad this frame and last frame.
    pad: Option<PadState>,
    pad_before: PadState,
    controls: PlayerControls,
    bindings: Bindings,
    touch: TouchOverlay,
    /// The action waiting for a new key or button (Controls page).
    listening: Option<Action>,
    /// Seconds until a held direction steps the hero again.
    step_wait: f32,
    /// The player chose Leave the game.
    left: bool,
}

impl GameScreen {
    /// The game of `session`, played with the project's controls and the player's `controls`.
    pub fn new(session: Session, controls: PlayerControls) -> Self {
        let center = session.hero_position().unwrap_or_default();
        let zoom = session.settings().camera.zoom as f32;
        let (cursors, problems) = Cursors::load(&*session.world.game, &session.settings().cursors);
        let bindings = Bindings {
            project: session.settings().input.clone(),
            player: controls.bindings.clone(),
        };
        let mut screen = Self {
            session,
            camera: Camera2d { center, zoom },
            follow: true,
            simulate_touch: false,
            viewport: Vec2::new(1280.0, 800.0),
            cursors,
            over_world: false,
            shake: Vec2::ZERO,
            mapper: InputMapper::default(),
            gamepads: Gamepads::default(),
            pad: None,
            pad_before: PadState::default(),
            controls,
            bindings,
            touch: TouchOverlay::default(),
            listening: None,
            step_wait: 0.0,
            left: false,
        };
        screen.session.report(problems);
        let prefs = screen.controls.prefs;
        screen.session.set_player_prefs(prefs);
        screen
    }

    /// The player's volumes and window (the host makes the window fullscreen or not).
    pub fn prefs(&self) -> c3_core::settings::PlayerPrefs {
        self.controls.prefs
    }

    pub fn world(&mut self) -> &mut World {
        &mut self.session.world
    }

    pub fn viewport(&self) -> Vec2 {
        self.viewport
    }

    /// Whether the player chose to leave the game.
    pub fn left(&self) -> bool {
        self.left
    }

    /// The device the player uses now.
    pub fn device(&self) -> Device {
        if self.simulate_touch {
            Device::Touch
        } else {
            self.mapper.active()
        }
    }

    /// The camera to draw the world with this frame (shaken just after the hero is hit).
    pub fn view_camera(&self) -> Camera2d {
        Camera2d {
            center: self.camera.center + self.shake / self.camera.zoom,
            zoom: self.camera.zoom,
        }
    }

    /// Plays by `settings` from now on: the session's, the camera's, the controls and cursors.
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
        self.bindings.project = settings.input.clone();
        self.session.set_settings(settings);
    }

    fn camera_settings(&self) -> &CameraSettings {
        &self.session.settings().camera
    }

    /// Reads the gamepads before the UI's frame; while a game window is open, the menu
    /// controls move between its buttons and press them (as Tab and Enter would).
    pub fn before_frame(&mut self, raw: &mut egui::RawInput) {
        self.pad_before = self.pad.take().unwrap_or_default();
        self.pad = self.gamepads.read();
        let Some(pad) = &self.pad else { return };
        if self.listening.is_some() || !self.session.window_open() {
            return;
        }
        let pressed = |control: PadControl| self.pad_control_pressed(control, pad);
        let nav = self.pad_bindings(Action::Navigate);
        let next = nav.iter().any(|c| match c {
            PadControl::DPad => pressed(PadControl::DPadDown) || pressed(PadControl::DPadRight),
            PadControl::LeftStick => leaned(pad.left_stick, self.pad_before.left_stick, |v| {
                v.y > NAV_LEAN || v.x > NAV_LEAN
            }),
            PadControl::RightStick => leaned(pad.right_stick, self.pad_before.right_stick, |v| {
                v.y > NAV_LEAN || v.x > NAV_LEAN
            }),
            _ => false,
        });
        let previous = nav.iter().any(|c| match c {
            PadControl::DPad => pressed(PadControl::DPadUp) || pressed(PadControl::DPadLeft),
            PadControl::LeftStick => leaned(pad.left_stick, self.pad_before.left_stick, |v| {
                v.y < -NAV_LEAN || v.x < -NAV_LEAN
            }),
            PadControl::RightStick => leaned(pad.right_stick, self.pad_before.right_stick, |v| {
                v.y < -NAV_LEAN || v.x < -NAV_LEAN
            }),
            _ => false,
        });
        let confirm = self.pad_bindings(Action::Confirm).into_iter().any(pressed);
        let key = |key: Key, shift: bool| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                shift,
                ..Default::default()
            },
        };
        if next {
            raw.events.push(key(Key::Tab, false));
        }
        if previous {
            raw.events.push(key(Key::Tab, true));
        }
        if confirm {
            raw.events.push(key(Key::Enter, false));
        }
    }

    /// The gamepad controls `action` is bound to.
    fn pad_bindings(&self, action: Action) -> Vec<PadControl> {
        (self.bindings.of(action).into_iter())
            .filter_map(|b| match b {
                Binding::Pad(c) => Some(c),
                _ => None,
            })
            .collect()
    }

    /// Whether gamepad button `control` went down this frame.
    fn pad_control_pressed(&self, control: PadControl, pad: &PadState) -> bool {
        PadButton::of(control)
            .is_some_and(|b| pad.buttons.contains(&b) && !self.pad_before.buttons.contains(&b))
    }

    /// Takes this frame's input over `rect` and runs `seconds` of game. Returns the monster
    /// under the pointer. Then the host draws the world and calls [`Self::overlay`].
    pub fn update(&mut self, ui: &egui::Ui, rect: Rect, seconds: f32) -> Option<EntityId> {
        let resp = ui.interact(
            rect,
            ui.scope_id().with("game view"),
            Sense::click_and_drag(),
        );
        self.viewport = Vec2::new(rect.width(), rect.height());
        if ui.input(|i| i.any_touches()) {
            self.mapper.touched();
        }
        let frame = self.device_frame(ui, rect, &resp, seconds);
        let actions = self.mapper.map(&frame, &self.bindings);
        if self.listening.is_some() {
            self.listen(ui);
        } else {
            self.act(ui, rect, &resp, &actions, seconds);
        }
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

    /// What the keyboard, mouse, gamepad and touch controls did this frame.
    fn device_frame(
        &mut self,
        ui: &egui::Ui,
        rect: Rect,
        resp: &egui::Response,
        seconds: f32,
    ) -> DeviceFrame {
        let over = resp.hovered();
        let typing = self.session.typing() || ui.ctx().egui_wants_keyboard_input();
        let keys = ui.input(|i| {
            let mut k = KeyState {
                typing,
                ..KeyState::default()
            };
            for event in &i.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    ..
                } = event
                {
                    k.pressed.insert(*key);
                }
            }
            k.down = i.keys_down.clone();
            for (button, control) in [
                (PointerButton::Primary, MouseControl::Left),
                (PointerButton::Secondary, MouseControl::Right),
                (PointerButton::Middle, MouseControl::Middle),
            ] {
                if over && i.pointer.button_pressed(button) {
                    k.mouse_pressed.insert(control);
                }
                if over && i.pointer.button_down(button) {
                    k.mouse_down.insert(control);
                }
            }
            if over {
                k.wheel = i.smooth_scroll_delta.y;
            }
            k
        });
        let touch = if self.simulate_touch || self.mapper.touch_controls(&self.bindings.project) {
            let joystick = self.bound_to_touch(Action::Move, TouchControl::Joystick);
            let buttons: Vec<Action> = Action::ALL
                .into_iter()
                .filter(|a| self.bound_to_touch(*a, TouchControl::Button))
                .collect();
            self.touch.show(ui.ctx(), rect, joystick, &buttons)
        } else {
            Default::default()
        };
        DeviceFrame {
            keys,
            pad: self.pad.clone(),
            touch,
            dt: seconds,
        }
    }

    fn bound_to_touch(&self, action: Action, control: TouchControl) -> bool {
        self.bindings.of(action).contains(&Binding::Touch(control))
    }

    /// Carries out this frame's actions.
    fn act(&mut self, ui: &egui::Ui, rect: Rect, resp: &egui::Response, a: &ActionFrame, dt: f32) {
        let in_window = self.session.window_open();
        let pad_in_menu = in_window && self.mapper.active() == Device::Gamepad;
        self.pointer(rect, resp);
        if a.pressed(Action::Menu) && !self.session.back() {
            self.session.toggle_menu();
        }
        if a.pressed(Action::Back) {
            self.session.back();
        }
        if a.pressed(Action::Bag) {
            self.session.toggle_bag();
        }
        if a.pressed(Action::Quests) {
            self.session.toggle_quests();
        }
        if a.pressed(Action::Chat) {
            self.session.open_chat();
        }
        if !pad_in_menu {
            self.world_actions(a, dt);
        }
        self.camera_actions(ui, rect, resp, a, dt);
    }

    fn world_actions(&mut self, a: &ActionFrame, dt: f32) {
        if a.pressed(Action::ToggleRun) {
            self.session.toggle_running();
        }
        if a.pressed(Action::AttackNearest) {
            self.session.attack_nearest();
        }
        if a.pressed(Action::NextTarget) {
            self.session.cycle_target(1);
        }
        if a.pressed(Action::PreviousTarget) {
            self.session.cycle_target(-1);
        }
        if a.pressed(Action::Interact) {
            self.session.interact();
        }
        for (slot, action) in Action::SKILLS.into_iter().enumerate() {
            if a.pressed(action) {
                self.session.cast_slot(slot);
            }
        }
        let turn = i8::from(a.pressed(Action::TurnRight)) - i8::from(a.pressed(Action::TurnLeft));
        if turn != 0
            && let Some(hero) = self.session.world.hero_mut()
        {
            hero.turn(turn);
        }
        let lean = a.vector(Action::Move);
        if lean == Vec2::ZERO {
            self.step_wait = 0.0;
        } else {
            self.step_wait -= dt;
            if self.step_wait <= 0.0 {
                self.session.step_hero(lean);
                self.step_wait = self.session.settings().input.repeat_seconds as f32;
                self.follow = true;
            }
        }
    }

    fn camera_actions(
        &mut self,
        ui: &egui::Ui,
        rect: Rect,
        resp: &egui::Response,
        a: &ActionFrame,
        dt: f32,
    ) {
        let c = self.camera_settings();
        let (free_pan, pan_speed, wheel_speed) =
            (c.free_pan, c.pan_speed as f32, c.wheel_speed as f32);
        let (min_zoom, max_zoom) = (c.min_zoom as f32, c.max_zoom as f32);
        if a.pressed(Action::Follow) {
            self.follow = true;
        }
        if a.pressed(Action::FitMap) && free_pan {
            self.camera.fit(self.session.world.bounds(), self.viewport);
            self.camera.zoom = self.camera.zoom.clamp(min_zoom, max_zoom);
            self.follow = false;
        }
        let pan = a.vector(Action::Pan);
        if pan != Vec2::ZERO && free_pan {
            self.follow = false;
            self.camera.center += pan * pan_speed * dt / self.camera.zoom;
        }
        let zoom = a.axis(Action::Zoom) * wheel_speed;
        if zoom != 0.0 {
            let anchor = resp
                .hover_pos()
                .filter(|_| ui.input(|i| i.smooth_scroll_delta.y != 0.0))
                .map_or(self.viewport / 2.0, |p| {
                    Vec2::new(p.x - rect.left(), p.y - rect.top())
                });
            self.zoom_by(zoom.exp(), anchor);
        }
    }

    /// Mouse drags pan; clicks (and taps) act on what is under the pointer.
    fn pointer(&mut self, rect: Rect, resp: &egui::Response) {
        let buttons = [
            (PointerButton::Primary, MouseControl::Left),
            (PointerButton::Secondary, MouseControl::Right),
            (PointerButton::Middle, MouseControl::Middle),
        ];
        if self.camera_settings().free_pan && buttons.iter().any(|(b, _)| resp.dragged_by(*b)) {
            let d = resp.drag_delta();
            self.camera.center -= Vec2::new(d.x, d.y) / self.camera.zoom;
            self.follow = false;
        }
        let tap = self.bound_to_touch(Action::Click, TouchControl::Tap);
        let clicked = buttons.into_iter().any(|(b, m)| {
            resp.clicked_by(b) && (self.clicks_with(m) || (tap && b == PointerButton::Primary))
        });
        let screen = Screen {
            rect,
            camera: &self.camera,
        };
        if clicked && let Some(p) = resp.interact_pointer_pos() {
            self.session.click(screen.to_world(p));
        }
        self.session
            .point_at(resp.hover_pos().map(|p| screen.to_world(p)));
    }

    fn clicks_with(&self, m: MouseControl) -> bool {
        self.bindings.of(Action::Click).contains(&Binding::Mouse(m))
    }

    /// Waits for the key or button to bind the listening action to; Esc or the gamepad's
    /// Back cancels.
    fn listen(&mut self, ui: &egui::Ui) {
        let Some(action) = self.listening else { return };
        let key = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    ..
                } => Some(*key),
                _ => None,
            })
        });
        let pad = self
            .pad
            .as_ref()
            .and_then(|now| pad_pressed(now, &self.pad_before));
        let binding = match (key, pad) {
            (Some(Key::Escape), _) => None,
            (Some(key), _) => Some(Binding::Key(key.name().to_owned())),
            (None, Some(b)) if self.pad_bindings(Action::Back).contains(&b.control()) => None,
            (None, Some(b)) => Some(Binding::Pad(b.control())),
            (None, None) => return,
        };
        self.listening = None;
        if let Some(binding) = binding {
            self.controls
                .rebind(&self.bindings.project, action, binding);
            self.controls_changed();
        }
    }

    fn controls_changed(&mut self) {
        self.bindings.player = self.controls.bindings.clone();
        if let Err(e) = self.controls.save() {
            self.session
                .report(vec![format!("Can't keep your controls: {e}")]);
        }
    }

    /// Draws the in-game UI over the world; `hovered` from [`Self::update`].
    pub fn overlay(&mut self, ui: &mut egui::Ui, rect: Rect, hovered: Option<EntityId>) {
        let camera = self.view_camera();
        let screen = Screen {
            rect,
            camera: &camera,
        };
        let device = self.device();
        let hint = self.hint(device);
        let rows: Vec<ControlRow> = Action::ALL
            .into_iter()
            .map(|action| ControlRow {
                action,
                bindings: self.bindings.label(action, device),
                rebindable: action.kind() == ActionKind::Button
                    && action != Action::Click
                    && device != Device::Touch,
            })
            .collect();
        let controls = ControlsView {
            device,
            rows: &rows,
            listening: self.listening,
            prefs: self.controls.prefs,
        };
        for action in self.session.ui(ui, &screen, hovered, (&hint, &controls)) {
            match action {
                UiAction::Rebind(a) => self.listening = Some(a),
                UiAction::ResetControls => {
                    self.controls.reset();
                    self.controls_changed();
                }
                UiAction::Leave => self.left = true,
                UiAction::SetPrefs(prefs) => {
                    self.controls.prefs = prefs;
                    self.session.set_player_prefs(prefs);
                    self.controls_changed();
                }
                _ => {}
            }
        }
        // Over the game's own windows its arrow still shows.
        let pointer = ui.ctx().pointer_hover_pos().filter(|p| rect.contains(*p));
        if pointer.is_some() && !self.over_world {
            let now = ui.input(|i| i.time);
            self.cursors
                .show(ui.ctx(), CursorKind::Normal, pointer, now);
        }
    }

    /// What the controls do on `device`, as one line.
    fn hint(&self, device: Device) -> String {
        let mut parts: Vec<String> = HINTS
            .iter()
            .filter_map(|(action, does)| {
                // An on-screen button shows its own icon.
                let button = Binding::Touch(TouchControl::Button);
                if device == Device::Touch && self.bindings.of(*action).contains(&button) {
                    return None;
                }
                let keys = self.bindings.label(*action, device);
                (!keys.is_empty()).then(|| format!("{keys}: {does}"))
            })
            .collect();
        if !self.follow {
            let keys = self.bindings.label(Action::Follow, device);
            if !keys.is_empty() {
                parts.push(format!("{keys}: follow the hero"));
            }
        }
        parts.join(" · ")
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

    /// Takes the hero to another map when it went through a portal (see
    /// [`Session::follow_map_change`]); the camera jumps to it there.
    pub fn follow_map_change(
        &mut self,
        project: &c3_project::Project,
        maps: &crate::map_list::MapList,
        renderer: &mut c3_render::Renderer,
    ) -> Option<Result<String, String>> {
        let moved = self.session.follow_map_change(project, maps, renderer)?;
        if moved.is_ok()
            && let Some(hero) = self.session.hero_position()
        {
            self.camera.center = hero;
        }
        Some(moved)
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
}

/// Whether a stick leaned past the line `past` this frame (and not last frame).
fn leaned(now: Vec2, before: Vec2, past: impl Fn(Vec2) -> bool) -> bool {
    past(now) && !past(before)
}
