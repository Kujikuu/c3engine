//! Project Settings > Input: each action's bindings per device (keyboard and mouse, gamepad,
//! touch), added from the controls that fit the action or by pressing a key, removed, or put
//! back to the engine's defaults; plus the gamepad dead zone, the repeat time and when the
//! on-screen touch controls show.

use super::{Fields, grid, heading};
use crate::icons;
use crate::theme;
use c3_core::input::{
    Action, ActionGroup, Binding, Device, InputSettings, PadControl, TouchControls,
};
use c3_play::input::short_name;
use eframe::egui::{self, RichText};
use serde_json::json;

/// Keyboard and mouse bindings offered besides pressing a key.
const KEYBOARD_MOUSE: [&str; 8] = [
    "Mouse Left",
    "Mouse Right",
    "Mouse Middle",
    "Mouse Wheel",
    "Keys W A S D",
    "Keys ArrowUp ArrowLeft ArrowDown ArrowRight",
    "Keys PageUp PageDown",
    "Keys Plus Minus",
];

const TOUCH: [&str; 4] = ["Touch Tap", "Touch Pinch", "Touch Joystick", "Touch Button"];

/// The bindings `action` may get on `device` (pressing a key aside).
fn choices(action: Action, device: Device) -> Vec<Binding> {
    let texts: Vec<String> = match device {
        Device::KeyboardMouse => KEYBOARD_MOUSE.iter().map(|t| (*t).to_owned()).collect(),
        Device::Gamepad => PadControl::all()
            .map(|c| format!("Pad {}", c.name()))
            .collect(),
        Device::Touch => TOUCH.iter().map(|t| (*t).to_owned()).collect(),
    };
    texts
        .iter()
        .filter_map(|t| t.parse::<Binding>().ok())
        .filter(|b| b.fits(action.kind()))
        .collect()
}

/// The setting path of `action`'s bindings.
fn path(action: Action) -> String {
    let name = serde_json::to_value(action)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    format!("input.bindings.{name}")
}

impl Fields<'_> {
    /// Binds `action` to a key: the one pressed while the Studio listened.
    pub(super) fn bind_key(&mut self, input: &InputSettings, action: Action, key: egui::Key) {
        let mut list = input.bindings(action);
        let binding = Binding::Key(key.name().to_owned());
        if !list.contains(&binding) {
            list.push(binding);
        }
        self.set_bindings(action, &list);
    }

    fn set_bindings(&mut self, action: Action, list: &[Binding]) {
        let texts: Vec<String> = list.iter().map(ToString::to_string).collect();
        self.set(&path(action), json!(texts));
    }

    /// The Input page.
    pub(super) fn input(
        &mut self,
        ui: &mut egui::Ui,
        input: &InputSettings,
        listening: &mut Option<Action>,
    ) {
        heading(ui, "Actions");
        ui.label(
            RichText::new(
                "The game reads actions, never keys. Click a cell to add or remove a binding; \
                 players can rebind in the game's menu too.",
            )
            .color(theme::TEXT_DIM),
        );
        ui.add_space(4.0);
        egui::Grid::new("input actions")
            .num_columns(5)
            .spacing([12.0, 6.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label("");
                for device in Device::ALL {
                    ui.label(RichText::new(device.name()).strong());
                }
                ui.label("");
                ui.end_row();
                let mut group: Option<ActionGroup> = None;
                for action in Action::ALL {
                    if group != Some(action.group()) {
                        group = Some(action.group());
                        ui.label(
                            RichText::new(action.group().name())
                                .color(theme::ACCENT)
                                .small(),
                        );
                        ui.end_row();
                    }
                    ui.label(action.label());
                    let list = input.bindings(action);
                    for device in Device::ALL {
                        self.binding_cell(ui, action, device, &list, listening);
                    }
                    self.reset(ui, &path(action));
                    ui.end_row();
                }
            });
        heading(ui, "Gamepads and touch");
        grid(ui, "input options", |ui| {
            self.number(
                ui,
                (
                    "Stick dead zone",
                    "How far a stick must lean before it counts",
                ),
                "input.dead_zone",
                input.dead_zone,
                0.0..=0.9,
                "",
            );
            self.number(
                ui,
                (
                    "Repeat",
                    "How often a held direction steps the hero or the menu focus",
                ),
                "input.repeat_seconds",
                input.repeat_seconds,
                0.05..=2.0,
                " s",
            );
            Self::label(
                ui,
                "Touch controls",
                "When the on-screen joystick and buttons show",
            );
            ui.horizontal(|ui| {
                for (mode, name) in [
                    (TouchControls::Auto, "Once the screen is touched"),
                    (TouchControls::Always, "Always"),
                    (TouchControls::Never, "Never"),
                ] {
                    if ui.radio(input.touch_controls == mode, name).clicked() {
                        self.set("input.touch_controls", json!(mode));
                    }
                }
            });
            ui.end_row();
        });
    }

    /// One action's bindings on one device, as a button opening their editor.
    fn binding_cell(
        &mut self,
        ui: &mut egui::Ui,
        action: Action,
        device: Device,
        list: &[Binding],
        listening: &mut Option<Action>,
    ) {
        let mine: Vec<&Binding> = list.iter().filter(|b| b.device() == device).collect();
        let text = if *listening == Some(action) && device == Device::KeyboardMouse {
            "Press a key… (Esc: cancel)".to_owned()
        } else if mine.is_empty() {
            "—".to_owned()
        } else {
            mine.iter()
                .map(|b| short_name(b))
                .collect::<Vec<_>>()
                .join(" / ")
        };
        ui.menu_button(RichText::new(text).monospace(), |ui| {
            for b in &mine {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(b.to_string()).monospace());
                    if ui
                        .button(icons::TRASH)
                        .on_hover_text("Remove this binding")
                        .clicked()
                    {
                        let rest: Vec<Binding> = list.iter().filter(|x| x != b).cloned().collect();
                        self.set_bindings(action, &rest);
                        ui.close();
                    }
                });
            }
            if !mine.is_empty() {
                ui.separator();
            }
            if device == Device::KeyboardMouse
                && action.kind() == c3_core::input::ActionKind::Button
                && ui
                    .button(theme::icon_text(icons::KEY, "Press a key…"))
                    .clicked()
            {
                *listening = Some(action);
                ui.close();
            }
            for choice in choices(action, device) {
                if list.contains(&choice) {
                    continue;
                }
                if ui
                    .button(theme::icon_text(icons::PLUS, &choice.to_string()))
                    .clicked()
                {
                    let mut next = list.to_vec();
                    next.push(choice);
                    self.set_bindings(action, &next);
                    ui.close();
                }
            }
        });
    }
}
