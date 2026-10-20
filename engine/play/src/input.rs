//! The Input System at play: each frame, what the devices did becomes what the player asked
//! for, by the game's [`Action`]s and their bindings (Project Settings > Input, then the
//! player's own). Keyboard, mouse and touch come through egui; gamepads through `gilrs`
//! (Xbox, PlayStation, Switch Pro and generic pads on Windows, macOS and Linux). The device
//! used last is the active one: hints and on-screen controls follow it.

use c3_core::input::{
    Action, ActionKind, Binding, Device, InputSettings, Modifier, MouseControl, PadControl,
    TouchControl, TouchControls,
};
use egui::Key;
use glam::Vec2;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Wheel zoom (log of the factor) per scrolled point.
const WHEEL_ZOOM: f32 = 0.002;
/// Zoom (log of the factor) per second while a zoom key or trigger is held fully.
const HELD_ZOOM: f32 = 1.5;

/// A gamepad's buttons and sticks at one moment.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PadState {
    pub buttons: HashSet<PadButton>,
    /// Right and down are positive, as on screen.
    pub left_stick: Vec2,
    pub right_stick: Vec2,
    pub left_trigger: f32,
    pub right_trigger: f32,
}

/// A gamepad button (the pressable [`PadControl`]s).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PadButton {
    South,
    East,
    West,
    North,
    LeftBumper,
    RightBumper,
    LeftTrigger,
    RightTrigger,
    Select,
    Start,
    LeftStickPress,
    RightStickPress,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

impl PadButton {
    const ALL: [Self; 16] = [
        Self::South,
        Self::East,
        Self::West,
        Self::North,
        Self::LeftBumper,
        Self::RightBumper,
        Self::LeftTrigger,
        Self::RightTrigger,
        Self::Select,
        Self::Start,
        Self::LeftStickPress,
        Self::RightStickPress,
        Self::DPadUp,
        Self::DPadDown,
        Self::DPadLeft,
        Self::DPadRight,
    ];

    /// The button `control` names (`None` for sticks, the D-pad as a whole and triggers).
    pub fn of(control: PadControl) -> Option<Self> {
        Some(match control {
            PadControl::South => Self::South,
            PadControl::East => Self::East,
            PadControl::West => Self::West,
            PadControl::North => Self::North,
            PadControl::LeftBumper => Self::LeftBumper,
            PadControl::RightBumper => Self::RightBumper,
            PadControl::LeftTrigger => Self::LeftTrigger,
            PadControl::RightTrigger => Self::RightTrigger,
            PadControl::Select => Self::Select,
            PadControl::Start => Self::Start,
            PadControl::LeftStickPress => Self::LeftStickPress,
            PadControl::RightStickPress => Self::RightStickPress,
            PadControl::DPadUp => Self::DPadUp,
            PadControl::DPadDown => Self::DPadDown,
            PadControl::DPadLeft => Self::DPadLeft,
            PadControl::DPadRight => Self::DPadRight,
            PadControl::DPad
            | PadControl::LeftStick
            | PadControl::RightStick
            | PadControl::Triggers => {
                return None;
            }
        })
    }

    /// The binding this button makes when a player rebinds by pressing it.
    pub fn control(self) -> PadControl {
        match self {
            Self::South => PadControl::South,
            Self::East => PadControl::East,
            Self::West => PadControl::West,
            Self::North => PadControl::North,
            Self::LeftBumper => PadControl::LeftBumper,
            Self::RightBumper => PadControl::RightBumper,
            Self::LeftTrigger => PadControl::LeftTrigger,
            Self::RightTrigger => PadControl::RightTrigger,
            Self::Select => PadControl::Select,
            Self::Start => PadControl::Start,
            Self::LeftStickPress => PadControl::LeftStickPress,
            Self::RightStickPress => PadControl::RightStickPress,
            Self::DPadUp => PadControl::DPadUp,
            Self::DPadDown => PadControl::DPadDown,
            Self::DPadLeft => PadControl::DPadLeft,
            Self::DPadRight => PadControl::DPadRight,
        }
    }
}

/// Reads gamepads. Without a gamepad system it reads nothing.
pub struct Gamepads {
    gilrs: Option<gilrs::Gilrs>,
}

impl Default for Gamepads {
    fn default() -> Self {
        Self {
            gilrs: gilrs::Gilrs::new().ok(),
        }
    }
}

impl Gamepads {
    /// The first connected gamepad's state now.
    pub fn read(&mut self) -> Option<PadState> {
        use gilrs::{Axis, Button};
        let gilrs = self.gilrs.as_mut()?;
        while gilrs.next_event().is_some() {}
        let (_, pad) = gilrs.gamepads().find(|(_, p)| p.is_connected())?;
        let mut state = PadState::default();
        for (button, b) in [
            (Button::South, PadButton::South),
            (Button::East, PadButton::East),
            (Button::West, PadButton::West),
            (Button::North, PadButton::North),
            (Button::LeftTrigger, PadButton::LeftBumper),
            (Button::RightTrigger, PadButton::RightBumper),
            (Button::LeftTrigger2, PadButton::LeftTrigger),
            (Button::RightTrigger2, PadButton::RightTrigger),
            (Button::Select, PadButton::Select),
            (Button::Start, PadButton::Start),
            (Button::LeftThumb, PadButton::LeftStickPress),
            (Button::RightThumb, PadButton::RightStickPress),
            (Button::DPadUp, PadButton::DPadUp),
            (Button::DPadDown, PadButton::DPadDown),
            (Button::DPadLeft, PadButton::DPadLeft),
            (Button::DPadRight, PadButton::DPadRight),
        ] {
            if pad.is_pressed(button) {
                state.buttons.insert(b);
            }
        }
        let axis = |a: Axis| pad.value(a);
        state.left_stick = Vec2::new(axis(Axis::LeftStickX), -axis(Axis::LeftStickY));
        state.right_stick = Vec2::new(axis(Axis::RightStickX), -axis(Axis::RightStickY));
        let trigger = |b: Button| pad.button_data(b).map_or(0.0, |d| d.value());
        state.left_trigger = trigger(Button::LeftTrigger2);
        state.right_trigger = trigger(Button::RightTrigger2);
        Some(state)
    }
}

/// What the on-screen touch controls did this frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TouchState {
    /// The joystick's lean, 0–1 long.
    pub joystick: Vec2,
    /// On-screen buttons tapped this frame.
    pub tapped: HashSet<Action>,
    /// Two-finger zoom this frame (log of the factor).
    pub pinch: f32,
}

/// What the keyboard and mouse did this frame (read from egui).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeyState {
    pub pressed: HashSet<Key>,
    pub down: HashSet<Key>,
    /// Buttons pressed this frame over the game.
    pub mouse_pressed: HashSet<MouseControl>,
    pub mouse_down: HashSet<MouseControl>,
    /// Keys held that chords use.
    pub modifiers: HashSet<Modifier>,
    /// Wheel scroll this frame (points, up positive).
    pub wheel: f32,
    /// The keyboard is busy (a text field has focus): keys count for nothing.
    pub typing: bool,
}

/// Everything the devices did in one frame.
#[derive(Debug, Clone, Default)]
pub struct DeviceFrame {
    pub keys: KeyState,
    pub pad: Option<PadState>,
    pub touch: TouchState,
    /// Seconds since the last frame.
    pub dt: f32,
}

/// What the player asked for in one frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ActionFrame {
    pressed: HashSet<Action>,
    held: HashSet<Action>,
    vectors: HashMap<Action, Vec2>,
    axes: HashMap<Action, f32>,
}

impl ActionFrame {
    /// The action started this frame.
    pub fn pressed(&self, action: Action) -> bool {
        self.pressed.contains(&action)
    }

    pub fn held(&self, action: Action) -> bool {
        self.held.contains(&action)
    }

    /// A direction action's lean (screen axes, at most 1 long).
    pub fn vector(&self, action: Action) -> Vec2 {
        self.vectors.get(&action).copied().unwrap_or_default()
    }

    /// An amount action's value this frame.
    pub fn axis(&self, action: Action) -> f32 {
        self.axes.get(&action).copied().unwrap_or_default()
    }
}

/// The bindings the game plays by: the project's, then the player's own on top.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bindings {
    pub project: InputSettings,
    /// What the player rebound.
    pub player: BTreeMap<Action, Vec<Binding>>,
}

impl Bindings {
    pub fn of(&self, action: Action) -> Vec<Binding> {
        self.player
            .get(&action)
            .cloned()
            .unwrap_or_else(|| self.project.bindings(action))
    }

    /// The bindings of `action` on `device`, as players read them (`Tab`, `South`...).
    pub fn label(&self, action: Action, device: Device) -> String {
        let names: Vec<String> = self
            .of(action)
            .iter()
            .filter(|b| b.device() == device)
            .map(short_name)
            .collect();
        names.join(" / ")
    }
}

/// A binding as a short name for hints: `Tab`, `WASD`, `Wheel`, `A`.
pub fn short_name(binding: &Binding) -> String {
    match binding {
        Binding::Key(k) => k.clone(),
        Binding::Keys(keys) if keys.iter().all(|k| k.len() == 1) => keys.concat(),
        Binding::Keys(keys) if keys.len() == 4 && keys[0] == "ArrowUp" => "Arrows".into(),
        Binding::Keys(keys) => keys.join("/"),
        Binding::Mouse(MouseControl::Wheel) => "Wheel".into(),
        Binding::Mouse(m) => format!("{m:?} click"),
        Binding::Chord(k, m) => format!("{k:?}+{m:?} click"),
        Binding::Pad(c) => pad_name(*c).into(),
        Binding::Touch(TouchControl::Tap) => "Tap".into(),
        Binding::Touch(TouchControl::Pinch) => "Pinch".into(),
        Binding::Touch(TouchControl::Joystick) => "Joystick".into(),
        Binding::Touch(TouchControl::Button) => "Button".into(),
    }
}

/// A gamepad control as most pads print it (Xbox letters).
fn pad_name(c: PadControl) -> &'static str {
    match c {
        PadControl::South => "A",
        PadControl::East => "B",
        PadControl::West => "X",
        PadControl::North => "Y",
        PadControl::LeftBumper => "LB",
        PadControl::RightBumper => "RB",
        PadControl::LeftTrigger => "LT",
        PadControl::RightTrigger => "RT",
        PadControl::Select => "View",
        PadControl::Start => "Menu",
        PadControl::LeftStickPress => "L3",
        PadControl::RightStickPress => "R3",
        PadControl::DPadUp => "D-pad up",
        PadControl::DPadDown => "D-pad down",
        PadControl::DPadLeft => "D-pad left",
        PadControl::DPadRight => "D-pad right",
        PadControl::DPad => "D-pad",
        PadControl::LeftStick => "Left stick",
        PadControl::RightStick => "Right stick",
        PadControl::Triggers => "LT/RT",
    }
}

/// Turns device frames into action frames, remembering the gamepad's last state and the
/// active device.
#[derive(Default)]
pub struct InputMapper {
    last_pad: PadState,
    active: Option<Device>,
}

impl InputMapper {
    /// The device used last (keyboard and mouse until another is used).
    pub fn active(&self) -> Device {
        self.active.unwrap_or(Device::KeyboardMouse)
    }

    /// Whether the on-screen touch controls show.
    pub fn touch_controls(&self, settings: &InputSettings) -> bool {
        match settings.touch_controls {
            TouchControls::Always => true,
            TouchControls::Never => false,
            TouchControls::Auto => self.active == Some(Device::Touch),
        }
    }

    /// Marks touch as used (the screen was touched).
    pub fn touched(&mut self) {
        self.active = Some(Device::Touch);
    }

    /// What `frame` asks for under `bindings`.
    pub fn map(&mut self, frame: &DeviceFrame, bindings: &Bindings) -> ActionFrame {
        let dead_zone = bindings.project.dead_zone as f32;
        let pad = frame.pad.clone().unwrap_or_default();
        self.note_device(frame, &pad, dead_zone);
        let mut out = ActionFrame::default();
        for action in Action::ALL {
            for binding in bindings.of(action) {
                self.apply(action, &binding, frame, &pad, dead_zone, &mut out);
            }
            if let Some(v) = out.vectors.get_mut(&action) {
                *v = v.clamp_length_max(1.0);
            }
        }
        self.last_pad = pad;
        out
    }

    fn note_device(&mut self, frame: &DeviceFrame, pad: &PadState, dead_zone: f32) {
        let k = &frame.keys;
        if !k.pressed.is_empty() || !k.mouse_pressed.is_empty() || k.wheel != 0.0 {
            self.active = Some(Device::KeyboardMouse);
        }
        let pad_used = pad
            .buttons
            .difference(&self.last_pad.buttons)
            .next()
            .is_some()
            || pad.left_stick.length() > dead_zone
            || pad.right_stick.length() > dead_zone;
        if pad_used {
            self.active = Some(Device::Gamepad);
        }
        let t = &frame.touch;
        if !t.tapped.is_empty() || t.joystick != Vec2::ZERO || t.pinch != 0.0 {
            self.active = Some(Device::Touch);
        }
    }

    fn apply(
        &self,
        action: Action,
        binding: &Binding,
        frame: &DeviceFrame,
        pad: &PadState,
        dead_zone: f32,
        out: &mut ActionFrame,
    ) {
        let keys = &frame.keys;
        let key_down = |name: &str| {
            !keys.typing && Key::from_name(name).is_some_and(|k| keys.down.contains(&k))
        };
        let key_pressed = |name: &str| {
            !keys.typing && Key::from_name(name).is_some_and(|k| keys.pressed.contains(&k))
        };
        let mut press = |pressed: bool, held: bool| {
            if pressed {
                out.pressed.insert(action);
            }
            if held {
                out.held.insert(action);
            }
        };
        match binding {
            Binding::Key(k) => press(key_pressed(k), key_down(k)),
            Binding::Mouse(MouseControl::Wheel) => {
                *out.axes.entry(action).or_default() += keys.wheel * WHEEL_ZOOM;
            }
            Binding::Mouse(m) => press(keys.mouse_pressed.contains(m), keys.mouse_down.contains(m)),
            Binding::Chord(k, m) => {
                let held = !keys.typing && keys.modifiers.contains(k);
                press(
                    held && keys.mouse_pressed.contains(m),
                    held && keys.mouse_down.contains(m),
                );
            }
            Binding::Keys(names) if names.len() == 4 => {
                let v = Vec2::new(
                    f32::from(u8::from(key_down(&names[3])))
                        - f32::from(u8::from(key_down(&names[1]))),
                    f32::from(u8::from(key_down(&names[2])))
                        - f32::from(u8::from(key_down(&names[0]))),
                );
                *out.vectors.entry(action).or_default() += v.normalize_or_zero();
            }
            Binding::Keys(names) => {
                let amount = f32::from(u8::from(key_down(&names[0])))
                    - f32::from(u8::from(key_down(&names[1])));
                *out.axes.entry(action).or_default() += amount * HELD_ZOOM * frame.dt;
            }
            Binding::Pad(control) => match control.kind() {
                ActionKind::Button => {
                    let Some(b) = PadButton::of(*control) else {
                        return;
                    };
                    press(
                        pad.buttons.contains(&b) && !self.last_pad.buttons.contains(&b),
                        pad.buttons.contains(&b),
                    );
                }
                ActionKind::Vector => {
                    let v = match control {
                        PadControl::LeftStick => dead(pad.left_stick, dead_zone),
                        PadControl::RightStick => dead(pad.right_stick, dead_zone),
                        _ => dpad(pad),
                    };
                    *out.vectors.entry(action).or_default() += v;
                }
                ActionKind::Axis => {
                    let amount = pad.right_trigger - pad.left_trigger;
                    *out.axes.entry(action).or_default() += amount * HELD_ZOOM * frame.dt;
                }
            },
            Binding::Touch(TouchControl::Button) => {
                press(frame.touch.tapped.contains(&action), false)
            }
            Binding::Touch(TouchControl::Joystick) => {
                *out.vectors.entry(action).or_default() += frame.touch.joystick;
            }
            Binding::Touch(TouchControl::Pinch) => {
                *out.axes.entry(action).or_default() += frame.touch.pinch;
            }
            // Taps reach the game as pointer clicks.
            Binding::Touch(TouchControl::Tap) => {}
        }
    }
}

/// A stick's lean with the dead zone cut out (0 inside it, rising to 1 at the edge).
fn dead(v: Vec2, dead_zone: f32) -> Vec2 {
    let len = v.length();
    if len <= dead_zone {
        return Vec2::ZERO;
    }
    v / len * ((len - dead_zone) / (1.0 - dead_zone)).min(1.0)
}

fn dpad(pad: &PadState) -> Vec2 {
    let on = |b: PadButton| f32::from(u8::from(pad.buttons.contains(&b)));
    Vec2::new(
        on(PadButton::DPadRight) - on(PadButton::DPadLeft),
        on(PadButton::DPadDown) - on(PadButton::DPadUp),
    )
    .normalize_or_zero()
}

/// The gamepad buttons pressed this frame (for rebinding by pressing one).
pub fn pad_pressed(now: &PadState, before: &PadState) -> Option<PadButton> {
    PadButton::ALL
        .into_iter()
        .find(|b| now.buttons.contains(b) && !before.buttons.contains(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bindings() -> Bindings {
        Bindings::default()
    }

    #[test]
    fn keys_drive_buttons_and_directions() {
        let mut m = InputMapper::default();
        let mut frame = DeviceFrame::default();
        frame.keys.pressed.insert(Key::Tab);
        frame.keys.down.extend([Key::Tab, Key::W, Key::D]);
        let a = m.map(&frame, &bindings());
        assert!(a.pressed(Action::AttackNearest) && a.held(Action::AttackNearest));
        let pan = a.vector(Action::Pan);
        assert!(pan.x > 0.6 && pan.y < -0.6, "{pan}");
        assert_eq!(m.active(), Device::KeyboardMouse);
        // Typing keeps keys out of the game.
        frame.keys.typing = true;
        assert!(!m.map(&frame, &bindings()).pressed(Action::AttackNearest));
    }

    #[test]
    fn gamepads_press_once_and_lean_past_the_dead_zone() {
        let mut m = InputMapper::default();
        let mut frame = DeviceFrame {
            pad: Some(PadState::default()),
            dt: 0.1,
            ..DeviceFrame::default()
        };
        let pad = frame.pad.as_mut().unwrap();
        pad.buttons.insert(PadButton::West);
        pad.left_stick = Vec2::new(0.1, 0.0);
        pad.right_trigger = 1.0;
        let a = m.map(&frame, &bindings());
        assert!(a.pressed(Action::AttackNearest));
        assert_eq!(a.vector(Action::Move), Vec2::ZERO);
        assert!(a.axis(Action::Zoom) > 0.1);
        assert_eq!(m.active(), Device::Gamepad);
        // Held, not pressed again; the stick leans.
        frame.pad.as_mut().unwrap().left_stick = Vec2::new(0.0, 1.0);
        let a = m.map(&frame, &bindings());
        assert!(!a.pressed(Action::AttackNearest) && a.held(Action::AttackNearest));
        assert!(a.vector(Action::Move).y > 0.9);
    }

    #[test]
    fn player_bindings_win_and_hints_name_the_device() {
        let mut b = bindings();
        b.player.insert(
            Action::Bag,
            vec!["Key I".parse().unwrap(), "Pad North".parse().unwrap()],
        );
        assert_eq!(b.label(Action::Bag, Device::KeyboardMouse), "I");
        assert_eq!(b.label(Action::Bag, Device::Gamepad), "Y");
        assert_eq!(b.label(Action::Pan, Device::KeyboardMouse), "WASD / Arrows");
        assert_eq!(b.label(Action::Zoom, Device::Touch), "Pinch");
    }

    #[test]
    fn touch_buttons_and_the_joystick_count() {
        let mut m = InputMapper::default();
        let mut frame = DeviceFrame::default();
        frame.touch.tapped.insert(Action::Bag);
        frame.touch.joystick = Vec2::new(-1.0, 0.0);
        let a = m.map(&frame, &bindings());
        assert!(a.pressed(Action::Bag));
        assert_eq!(a.vector(Action::Move), Vec2::new(-1.0, 0.0));
        assert_eq!(m.active(), Device::Touch);
    }
}
