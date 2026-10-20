//! The Input System's model, like Unity's: the game reads **actions** (attack nearest, open
//! the bag, zoom...), never keys; each action has **bindings**, one or more per device (keyboard
//! and mouse, gamepad, touch). The project sets the defaults (Project Settings > Input);
//! players rebind in game.
//!
//! A binding is written as text: `Key Tab`, `Keys W A S D` (up, left, down, right), `Keys
//! PageUp PageDown` (plus, minus), `Mouse Left`, `Mouse Wheel`, `Pad West`, `Pad LeftStick`,
//! `Pad Triggers`, `Touch Tap`, `Touch Pinch`, `Touch Joystick`, `Touch Button`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// What an action gives: a press, a direction or an amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    /// Pressed, held, released.
    Button,
    /// A 2D direction (sticks, four keys, a touch joystick).
    Vector,
    /// An amount, up or down (the wheel, a pinch, two triggers, two keys).
    Axis,
}

/// Where an action belongs, for lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionGroup {
    World,
    Camera,
    Skills,
    Windows,
    Menus,
}

impl ActionGroup {
    pub fn name(self) -> &'static str {
        match self {
            Self::World => "In the world",
            Self::Skills => "Skills",
            Self::Camera => "Camera",
            Self::Windows => "Windows",
            Self::Menus => "Menus",
        }
    }
}

/// Everything the engine's games do with input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Act on what is under the pointer: walk there, attack, talk, pick up.
    Click,
    /// Walk the hero cell by cell in a direction.
    Move,
    /// Act on the nearest thing: the target, an NPC, a drop.
    Interact,
    AttackNearest,
    NextTarget,
    PreviousTarget,
    ToggleRun,
    TurnLeft,
    TurnRight,
    /// Cast the skill in a slot of the skill bar.
    Skill1,
    Skill2,
    Skill3,
    Skill4,
    Skill5,
    Skill6,
    Skill7,
    Skill8,
    /// Cast the skill picked for it (TQ's skill box) at what is under the pointer.
    CastPicked,
    /// Move the camera off the hero.
    Pan,
    Zoom,
    /// Keep the hero in view again.
    Follow,
    FitMap,
    Bag,
    Quests,
    Chat,
    /// The game menu (controls, leave); closes the open window first.
    Menu,
    /// Menu navigation (gamepads move between buttons).
    Navigate,
    Confirm,
    Back,
}

impl Action {
    pub const ALL: [Self; 29] = [
        Self::Click,
        Self::Move,
        Self::Interact,
        Self::AttackNearest,
        Self::NextTarget,
        Self::PreviousTarget,
        Self::ToggleRun,
        Self::TurnLeft,
        Self::TurnRight,
        Self::Skill1,
        Self::Skill2,
        Self::Skill3,
        Self::Skill4,
        Self::Skill5,
        Self::Skill6,
        Self::Skill7,
        Self::Skill8,
        Self::CastPicked,
        Self::Pan,
        Self::Zoom,
        Self::Follow,
        Self::FitMap,
        Self::Bag,
        Self::Quests,
        Self::Chat,
        Self::Menu,
        Self::Navigate,
        Self::Confirm,
        Self::Back,
    ];

    /// The skill bar's slots, in order.
    pub const SKILLS: [Self; 8] = [
        Self::Skill1,
        Self::Skill2,
        Self::Skill3,
        Self::Skill4,
        Self::Skill5,
        Self::Skill6,
        Self::Skill7,
        Self::Skill8,
    ];

    pub fn kind(self) -> ActionKind {
        match self {
            Self::Move | Self::Pan | Self::Navigate => ActionKind::Vector,
            Self::Zoom => ActionKind::Axis,
            _ => ActionKind::Button,
        }
    }

    pub fn group(self) -> ActionGroup {
        match self {
            Self::Click
            | Self::Move
            | Self::Interact
            | Self::AttackNearest
            | Self::NextTarget
            | Self::PreviousTarget
            | Self::ToggleRun
            | Self::TurnLeft
            | Self::TurnRight => ActionGroup::World,
            Self::Skill1
            | Self::Skill2
            | Self::Skill3
            | Self::Skill4
            | Self::Skill5
            | Self::Skill6
            | Self::Skill7
            | Self::Skill8
            | Self::CastPicked => ActionGroup::Skills,
            Self::Pan | Self::Zoom | Self::Follow | Self::FitMap => ActionGroup::Camera,
            Self::Bag | Self::Quests | Self::Chat | Self::Menu => ActionGroup::Windows,
            Self::Navigate | Self::Confirm | Self::Back => ActionGroup::Menus,
        }
    }

    /// The name players see.
    pub fn label(self) -> &'static str {
        match self {
            Self::Click => "Act on what's pointed at",
            Self::Move => "Move",
            Self::Interact => "Interact with the nearest",
            Self::AttackNearest => "Attack the nearest",
            Self::NextTarget => "Next target",
            Self::PreviousTarget => "Previous target",
            Self::ToggleRun => "Walk / run",
            Self::TurnLeft => "Turn left",
            Self::TurnRight => "Turn right",
            Self::Skill1 => "Skill 1",
            Self::Skill2 => "Skill 2",
            Self::Skill3 => "Skill 3",
            Self::Skill4 => "Skill 4",
            Self::Skill5 => "Skill 5",
            Self::Skill6 => "Skill 6",
            Self::Skill7 => "Skill 7",
            Self::Skill8 => "Skill 8",
            Self::CastPicked => "Cast the picked skill",
            Self::Pan => "Pan the camera",
            Self::Zoom => "Zoom",
            Self::Follow => "Follow the hero",
            Self::FitMap => "See the whole map",
            Self::Bag => "Bag",
            Self::Quests => "Quests",
            Self::Chat => "Chat",
            Self::Menu => "Menu",
            Self::Navigate => "Move between buttons",
            Self::Confirm => "Press the button",
            Self::Back => "Back",
        }
    }

    /// The bindings a new project starts with.
    pub fn default_bindings(self) -> Vec<Binding> {
        let b = |texts: &[&str]| texts.iter().filter_map(|t| t.parse().ok()).collect();
        match self {
            Self::Click => b(&["Mouse Left", "Touch Tap"]),
            Self::Move => b(&["Pad LeftStick", "Pad DPad", "Touch Joystick"]),
            Self::Interact => b(&["Pad South", "Touch Button"]),
            Self::AttackNearest => b(&["Key Tab", "Pad West", "Touch Button"]),
            Self::NextTarget => b(&["Pad RightBumper"]),
            Self::PreviousTarget => b(&["Pad LeftBumper"]),
            Self::ToggleRun => b(&["Key R", "Pad North", "Touch Button"]),
            Self::TurnLeft => b(&["Key Q"]),
            Self::TurnRight => b(&["Key E"]),
            Self::Skill1 => b(&["Key 1", "Touch Button"]),
            Self::Skill2 => b(&["Key 2", "Touch Button"]),
            Self::Skill3 => b(&["Key 3"]),
            Self::Skill4 => b(&["Key 4"]),
            Self::Skill5 => b(&["Key 5"]),
            Self::Skill6 => b(&["Key 6"]),
            Self::Skill7 => b(&["Key 7"]),
            Self::Skill8 => b(&["Key 8"]),
            Self::CastPicked => b(&["Mouse Right"]),
            Self::Pan => b(&[
                "Keys W A S D",
                "Keys ArrowUp ArrowLeft ArrowDown ArrowRight",
                "Pad RightStick",
            ]),
            Self::Zoom => b(&[
                "Mouse Wheel",
                "Pad Triggers",
                "Touch Pinch",
                "Keys PageUp PageDown",
            ]),
            Self::Follow => b(&["Key Space", "Pad RightStickPress"]),
            Self::FitMap => b(&["Key F"]),
            Self::Bag => b(&["Key B", "Pad Select", "Touch Button"]),
            Self::Quests => b(&["Key L"]),
            Self::Chat => b(&["Key Enter"]),
            Self::Menu => b(&["Key Escape", "Pad Start", "Touch Button"]),
            Self::Navigate => b(&["Pad DPad", "Pad LeftStick"]),
            Self::Confirm => b(&["Pad South"]),
            Self::Back => b(&["Pad East"]),
        }
    }
}

/// A device a binding belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Device {
    KeyboardMouse,
    Gamepad,
    Touch,
}

impl Device {
    pub const ALL: [Self; 3] = [Self::KeyboardMouse, Self::Gamepad, Self::Touch];

    pub fn name(self) -> &'static str {
        match self {
            Self::KeyboardMouse => "Keyboard & mouse",
            Self::Gamepad => "Gamepad",
            Self::Touch => "Touch",
        }
    }
}

/// A gamepad control, by its place (south is A on Xbox, Cross on PlayStation, B on Switch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PadControl {
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
    /// The four D-pad buttons as a direction.
    DPad,
    LeftStick,
    RightStick,
    /// Right trigger plus, left trigger minus.
    Triggers,
}

impl PadControl {
    const NAMES: [(Self, &'static str); 20] = [
        (Self::South, "South"),
        (Self::East, "East"),
        (Self::West, "West"),
        (Self::North, "North"),
        (Self::LeftBumper, "LeftBumper"),
        (Self::RightBumper, "RightBumper"),
        (Self::LeftTrigger, "LeftTrigger"),
        (Self::RightTrigger, "RightTrigger"),
        (Self::Select, "Select"),
        (Self::Start, "Start"),
        (Self::LeftStickPress, "LeftStickPress"),
        (Self::RightStickPress, "RightStickPress"),
        (Self::DPadUp, "DPadUp"),
        (Self::DPadDown, "DPadDown"),
        (Self::DPadLeft, "DPadLeft"),
        (Self::DPadRight, "DPadRight"),
        (Self::DPad, "DPad"),
        (Self::LeftStick, "LeftStick"),
        (Self::RightStick, "RightStick"),
        (Self::Triggers, "Triggers"),
    ];

    /// Every control, buttons first.
    pub fn all() -> impl Iterator<Item = Self> {
        Self::NAMES.iter().map(|(c, _)| *c)
    }

    pub fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(c, _)| *c == self)
            .map_or("?", |(_, n)| n)
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case(name))
            .map(|(c, _)| *c)
    }

    /// How the control reads: a press, a direction or an amount.
    pub fn kind(self) -> ActionKind {
        match self {
            Self::DPad | Self::LeftStick | Self::RightStick => ActionKind::Vector,
            Self::Triggers => ActionKind::Axis,
            _ => ActionKind::Button,
        }
    }
}

/// A mouse control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseControl {
    Left,
    Right,
    Middle,
    /// Up plus, down minus.
    Wheel,
}

/// A touch gesture or on-screen control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TouchControl {
    /// A tap where the finger lands (acts as a click).
    Tap,
    /// Two fingers apart plus, together minus.
    Pinch,
    /// The on-screen joystick.
    Joystick,
    /// An on-screen button for the action.
    Button,
}

/// One way to trigger an action. Keys are named as egui names them (`Tab`, `A`, `ArrowUp`,
/// `PageUp`...).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Binding {
    Key(String),
    /// Four keys as a direction (up, left, down, right) or two as an amount (plus, minus).
    Keys(Vec<String>),
    Mouse(MouseControl),
    Pad(PadControl),
    Touch(TouchControl),
}

impl Binding {
    pub fn device(&self) -> Device {
        match self {
            Self::Key(_) | Self::Keys(_) | Self::Mouse(_) => Device::KeyboardMouse,
            Self::Pad(_) => Device::Gamepad,
            Self::Touch(_) => Device::Touch,
        }
    }

    /// Whether it can drive an action of `kind`.
    pub fn fits(&self, kind: ActionKind) -> bool {
        match self {
            Self::Key(_) => kind == ActionKind::Button,
            Self::Keys(keys) => match keys.len() {
                4 => kind == ActionKind::Vector,
                2 => kind == ActionKind::Axis,
                _ => false,
            },
            Self::Mouse(MouseControl::Wheel) => kind == ActionKind::Axis,
            Self::Mouse(_) => kind == ActionKind::Button,
            Self::Pad(c) => c.kind() == kind,
            Self::Touch(TouchControl::Tap | TouchControl::Button) => kind == ActionKind::Button,
            Self::Touch(TouchControl::Pinch) => kind == ActionKind::Axis,
            Self::Touch(TouchControl::Joystick) => kind == ActionKind::Vector,
        }
    }
}

impl fmt::Display for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Key(k) => write!(f, "Key {k}"),
            Self::Keys(keys) => write!(f, "Keys {}", keys.join(" ")),
            Self::Mouse(m) => write!(f, "Mouse {m:?}"),
            Self::Pad(c) => write!(f, "Pad {}", c.name()),
            Self::Touch(t) => write!(f, "Touch {t:?}"),
        }
    }
}

impl std::str::FromStr for Binding {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let mut words = text.split_whitespace();
        let device = words.next().unwrap_or_default();
        let rest: Vec<&str> = words.collect();
        let bad = || format!("\"{text}\" is not a binding");
        let one = || match rest.as_slice() {
            [name] => Ok(*name),
            _ => Err(bad()),
        };
        Ok(match device.to_ascii_lowercase().as_str() {
            "key" => Self::Key(one()?.to_owned()),
            "keys" if rest.len() == 2 || rest.len() == 4 => {
                Self::Keys(rest.iter().map(|k| (*k).to_owned()).collect())
            }
            "mouse" => Self::Mouse(match one()?.to_ascii_lowercase().as_str() {
                "left" => MouseControl::Left,
                "right" => MouseControl::Right,
                "middle" => MouseControl::Middle,
                "wheel" => MouseControl::Wheel,
                _ => return Err(bad()),
            }),
            "pad" => Self::Pad(PadControl::from_name(one()?).ok_or_else(bad)?),
            "touch" => Self::Touch(match one()?.to_ascii_lowercase().as_str() {
                "tap" => TouchControl::Tap,
                "pinch" => TouchControl::Pinch,
                "joystick" => TouchControl::Joystick,
                "button" => TouchControl::Button,
                _ => return Err(bad()),
            }),
            _ => return Err(bad()),
        })
    }
}

impl Serialize for Binding {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Binding {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// When the on-screen touch controls show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchControls {
    /// Once the screen is touched.
    #[default]
    Auto,
    Always,
    Never,
}

/// Project Settings > Input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputSettings {
    /// Bindings of the actions the project changed (the rest keep their defaults).
    pub bindings: BTreeMap<Action, Vec<Binding>>,
    /// How far a stick must lean before it counts (0–1).
    pub dead_zone: f64,
    /// How often a held direction steps the hero or the menu focus (seconds).
    pub repeat_seconds: f64,
    pub touch_controls: TouchControls,
}

impl Default for InputSettings {
    fn default() -> Self {
        Self {
            bindings: BTreeMap::new(),
            dead_zone: 0.25,
            repeat_seconds: 0.18,
            touch_controls: TouchControls::Auto,
        }
    }
}

impl InputSettings {
    /// The bindings of `action`: the project's, else the defaults.
    pub fn bindings(&self, action: Action) -> Vec<Binding> {
        self.bindings
            .get(&action)
            .cloned()
            .unwrap_or_else(|| action.default_bindings())
    }

    /// Why these input settings can't be used, if they can't.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for (action, bindings) in &self.bindings {
            for b in bindings {
                if !b.fits(action.kind()) {
                    problems.push(format!("{b} can't drive \"{}\"", action.label()));
                }
            }
        }
        if !(0.0..0.95).contains(&self.dead_zone) {
            problems.push("The dead zone must be 0 to 0.95".into());
        }
        if !(0.05..=2.0).contains(&self.repeat_seconds) {
            problems.push("The repeat time must be 0.05 to 2 seconds".into());
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_read_and_write_as_text() {
        for text in [
            "Key Tab",
            "Keys W A S D",
            "Keys PageUp PageDown",
            "Mouse Wheel",
            "Pad RightBumper",
            "Pad LeftStick",
            "Touch Pinch",
        ] {
            let b: Binding = text.parse().unwrap();
            assert_eq!(b.to_string(), text);
        }
        assert!("Key".parse::<Binding>().is_err());
        assert!("Keys A B C".parse::<Binding>().is_err());
        assert!("Pad Turbo".parse::<Binding>().is_err());
    }

    #[test]
    fn every_default_binding_fits_its_action() {
        for action in Action::ALL {
            let bindings = action.default_bindings();
            assert!(!bindings.is_empty(), "{action:?}");
            for b in bindings {
                assert!(b.fits(action.kind()), "{action:?}: {b}");
            }
        }
        assert!(InputSettings::default().problems().is_empty());
    }

    #[test]
    fn project_bindings_replace_the_defaults() {
        let mut input = InputSettings::default();
        input
            .bindings
            .insert(Action::Bag, vec!["Key I".parse().unwrap()]);
        assert_eq!(input.bindings(Action::Bag), vec![Binding::Key("I".into())]);
        assert_eq!(
            input.bindings(Action::Quests),
            Action::Quests.default_bindings()
        );
        input
            .bindings
            .insert(Action::Bag, vec!["Mouse Wheel".parse().unwrap()]);
        assert_eq!(input.problems().len(), 1);
    }
}
