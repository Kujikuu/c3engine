//! On-screen controls for touch screens: a joystick bottom left (actions bound to `Touch
//! Joystick`), round buttons bottom right (actions bound to `Touch Button`), and two-finger
//! pinch. They show once the screen is touched (or always, by the settings); the Studio's
//! Play mode can show them to try a touch game with the mouse.

use crate::input::TouchState;
use c3_core::input::Action;
use c3_ui::{icons, style};
use egui::{Color32, Pos2, Rect, Stroke, Vec2};

const STICK_RADIUS: f32 = 56.0;
const KNOB_RADIUS: f32 = 24.0;
const BUTTON_RADIUS: f32 = 28.0;
/// Space kept clear above the bottom of the game (the HUD bar).
const BOTTOM: f32 = 150.0;

/// The joystick's drag, kept between frames.
#[derive(Default)]
pub struct TouchOverlay {
    knob: Option<Vec2>,
}

/// The icon of an action's on-screen button.
fn icon(action: Action) -> &'static str {
    match action {
        Action::Interact => icons::HAND_TAP,
        Action::AttackNearest => icons::SWORD,
        Action::ToggleRun => icons::PERSON_SIMPLE_RUN,
        Action::Bag => icons::BACKPACK,
        Action::Menu => icons::LIST,
        Action::Quests => icons::SCROLL,
        Action::Chat => icons::CHAT_TEXT,
        _ => icons::STAR,
    }
}

impl TouchOverlay {
    /// Draws the controls over `rect` (the `buttons`' actions as round buttons) and reads them.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        joystick: bool,
        buttons: &[Action],
    ) -> TouchState {
        let mut state = TouchState {
            pinch: ctx.multi_touch().map_or(0.0, |t| t.zoom_delta.ln()),
            ..TouchState::default()
        };
        if joystick {
            let center = Pos2::new(rect.left() + STICK_RADIUS + 28.0, rect.bottom() - BOTTOM);
            state.joystick = self.joystick(ctx, center);
        }
        // Buttons in two staggered columns, bottom right.
        let gap = BUTTON_RADIUS * 2.0 + 12.0;
        for (i, action) in buttons.iter().enumerate() {
            let (col, row) = ((i % 2) as f32, (i / 2) as f32);
            let at = Pos2::new(
                rect.right() - BUTTON_RADIUS - 24.0 - col * gap,
                rect.bottom() - BOTTOM - row * gap + col * gap / 2.0,
            );
            if button(ctx, *action, at) {
                state.tapped.insert(*action);
            }
        }
        state
    }

    /// The joystick's lean (screen axes, at most 1 long).
    fn joystick(&mut self, ctx: &egui::Context, center: Pos2) -> glam::Vec2 {
        let area = egui::Area::new(egui::Id::unique("touch joystick"))
            .order(egui::Order::Foreground)
            .fixed_pos(center - Vec2::splat(STICK_RADIUS));
        let mut lean = glam::Vec2::ZERO;
        area.show(ctx, |ui| {
            let (rect, resp) =
                ui.allocate_exact_size(Vec2::splat(STICK_RADIUS * 2.0), egui::Sense::drag());
            self.knob = resp
                .dragged()
                .then(|| resp.interact_pointer_pos())
                .flatten()
                .map(|p| {
                    let d = p - rect.center();
                    d * (STICK_RADIUS / d.length().max(STICK_RADIUS))
                });
            let painter = ui.painter();
            painter.circle(
                rect.center(),
                STICK_RADIUS,
                Color32::from_black_alpha(90),
                Stroke::new(1.5, style::EDGE.gamma_multiply(0.8)),
            );
            let knob = self.knob.unwrap_or_default();
            painter.circle_filled(
                rect.center() + knob,
                KNOB_RADIUS,
                style::EDGE.gamma_multiply(0.85),
            );
            lean = glam::Vec2::new(knob.x, knob.y) / STICK_RADIUS;
        });
        lean
    }
}

/// One round button; whether it was tapped.
fn button(ctx: &egui::Context, action: Action, center: Pos2) -> bool {
    let mut tapped = false;
    egui::Area::new(egui::Id::unique(("touch button", action as u8)))
        .order(egui::Order::Foreground)
        .fixed_pos(center - Vec2::splat(BUTTON_RADIUS))
        .show(ctx, |ui| {
            let (rect, resp) =
                ui.allocate_exact_size(Vec2::splat(BUTTON_RADIUS * 2.0), egui::Sense::click());
            let fill = if resp.is_pointer_button_down_on() {
                style::EDGE
            } else {
                Color32::from_black_alpha(120)
            };
            let painter = ui.painter();
            painter.circle(
                rect.center(),
                BUTTON_RADIUS,
                fill,
                Stroke::new(1.5, style::EDGE),
            );
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                icon(action),
                egui::FontId::proportional(22.0),
                style::TEXT,
            );
            tapped = resp.clicked();
            resp.on_hover_text(action.label());
        });
    tapped
}
