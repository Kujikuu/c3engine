//! The game menu (the Menu action: Esc, Start, the on-screen button) and its Controls page,
//! where players see and change what each action is bound to on the device they use.

use crate::fonts;
use crate::icons;
use crate::style;
use crate::{ControlsView, UiAction};

use egui::{Rect, RichText};

const WIDTH: f32 = 300.0;
const CONTROLS_WIDTH: f32 = 460.0;
/// The Controls page's height besides its list of actions.
const CONTROLS_CHROME: f32 = 130.0;

/// Which page of the menu is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    Main,
    Controls,
}

/// Shows the menu's `page` centred on `rect`.
pub(crate) fn window(
    ui: &mut egui::Ui,
    rect: Rect,
    page: &mut Option<Page>,
    controls: &ControlsView,
    actions: &mut Vec<UiAction>,
) {
    let Some(current) = *page else { return };
    egui::Area::new(egui::Id::unique("game menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.center())
        .pivot(egui::Align2::CENTER_CENTER)
        .constrain_to(rect)
        .show(ui.ctx(), |ui| {
            fonts::use_game_font(ui);
            style::panel().show(ui, |ui| match current {
                Page::Main => main(ui, page, actions),
                Page::Controls => controls_page(ui, rect, page, controls, actions),
            });
        });
}

fn main(ui: &mut egui::Ui, page: &mut Option<Page>, actions: &mut Vec<UiAction>) {
    ui.set_width(WIDTH);
    ui.vertical_centered(|ui| {
        ui.label(style::title(icons::LIST, "Menu"));
    });
    ui.add_space(8.0);
    let button = |ui: &mut egui::Ui, icon: &str, text: &str| {
        ui.add(
            egui::Button::new(RichText::new(format!("{icon}  {text}")).size(14.0))
                .min_size(egui::vec2(WIDTH, 30.0)),
        )
    };
    let resume = button(ui, icons::PLAY, "Back to the game");
    if !ui.ctx().memory(|m| m.focused().is_some()) {
        // Gamepads and keys start on the first button.
        resume.request_focus();
    }
    if resume.clicked() {
        *page = None;
    }
    if button(ui, icons::GAME_CONTROLLER, "Controls").clicked() {
        *page = Some(Page::Controls);
    }
    if button(ui, icons::SIGN_OUT, "Leave the game").clicked() {
        *page = None;
        actions.push(UiAction::Leave);
    }
}

fn controls_page(
    ui: &mut egui::Ui,
    rect: Rect,
    page: &mut Option<Page>,
    controls: &ControlsView,
    actions: &mut Vec<UiAction>,
) {
    ui.set_width(CONTROLS_WIDTH);
    ui.horizontal(|ui| {
        ui.label(style::title(icons::GAME_CONTROLLER, "Controls"));
        ui.label(RichText::new(controls.device.name()).color(style::DIM));
    });
    ui.label(
        RichText::new(match controls.listening {
            Some(action) => format!("Press what should do \"{}\" · Esc: cancel", action.label()),
            None => "Click an action, then press its new key or button.".to_owned(),
        })
        .color(if controls.listening.is_some() {
            style::GOLD
        } else {
            style::DIM
        })
        .small(),
    );
    ui.add_space(6.0);
    egui::ScrollArea::vertical()
        // The title, the hint and the buttons stay in the game view.
        .max_height((rect.height() - CONTROLS_CHROME).max(80.0))
        .show(ui, |ui| {
            let mut group = None;
            egui::Grid::new("game controls")
                .num_columns(2)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    for row in controls.rows {
                        let g = row.action.group();
                        if group != Some(g) {
                            group = Some(g);
                            ui.label(RichText::new(g.name()).color(style::GOLD).small());
                            ui.end_row();
                        }
                        ui.label(row.action.label());
                        let text = match controls.listening {
                            Some(a) if a == row.action => "…".to_owned(),
                            _ if row.bindings.is_empty() => "none".to_owned(),
                            _ => row.bindings.clone(),
                        };
                        let rebindable = row.rebindable;
                        let button = egui::Button::new(RichText::new(text).monospace())
                            .min_size(egui::vec2(170.0, 22.0));
                        let resp = ui
                            .add_enabled(rebindable, button)
                            .on_disabled_hover_text("Change it in the game's settings");
                        if resp.clicked() {
                            actions.push(UiAction::Rebind(row.action));
                        }
                        ui.end_row();
                    }
                });
        });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button(format!("{}  Back", icons::ARROW_LEFT)).clicked() {
            *page = Some(Page::Main);
        }
        if ui
            .button(format!("{}  Defaults", icons::ARROW_COUNTER_CLOCKWISE))
            .on_hover_text("Undo every change you made to the controls")
            .clicked()
        {
            actions.push(UiAction::ResetControls);
        }
    });
}
