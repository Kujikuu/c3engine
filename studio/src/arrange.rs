//! Arranging the selection on the map: nudging it by cells or pixels, drawing it over or
//! under the rest, lining it up, hiding and locking it. Each change is one undo step.

use crate::app::Studio;
use crate::icons;
use crate::selection::{self, Line, Selection};
use crate::theme;
use c3_edit::SceneCommand;
use c3_scene::CellPos;
use eframe::egui;

/// Cell steps that move one step on screen, by arrow key: right, left, up, down.
pub(crate) const SCREEN_STEPS: [(egui::Key, CellPos, (i32, i32)); 4] = [
    (egui::Key::ArrowRight, (1, -1), (1, 0)),
    (egui::Key::ArrowLeft, (-1, 1), (-1, 0)),
    (egui::Key::ArrowUp, (-1, -1), (0, -1)),
    (egui::Key::ArrowDown, (1, 1), (0, 1)),
];

impl Studio {
    /// Moves the selection by `delta` cells.
    pub(crate) fn nudge_selection(&mut self, delta: CellPos) {
        let Some(doc) = &mut self.doc else { return };
        if let Some(message) = self.editor.nudge(doc, delta) {
            self.notify(message);
        }
    }

    /// Shifts the selected cover objects' images by `delta` pixels.
    pub(crate) fn nudge_pixels(&mut self, delta: (i32, i32)) {
        let Some(doc) = &self.doc else { return };
        let commands = selection::nudge_pixels(doc.scene(), self.editor.selection(), delta);
        self.run_arrange(
            "Nudge by a pixel",
            commands,
            "Only cover objects move by pixels",
        );
    }

    /// Draws the selected objects over (`front`) or under the others on their spot.
    pub(crate) fn reorder_selection(&mut self, front: bool) {
        let Some(doc) = &self.doc else { return };
        let (commands, after) = selection::reorder(doc.scene(), self.editor.selection(), front);
        let label = if front {
            "Bring to front"
        } else {
            "Send to back"
        };
        if self.run_arrange(label, commands, "Only objects have a drawing order") {
            self.editor.forget_object_marks();
            self.editor.select_all(after);
        }
    }

    /// Lines the selection up on `line` through its first one.
    pub(crate) fn align_selection(&mut self, line: Line) {
        let Some(doc) = &self.doc else { return };
        let commands = selection::align(doc.scene(), self.editor.selection(), line);
        let label = match line {
            Line::Row => "Line up in a row",
            Line::Column => "Line up in a column",
        };
        self.run_arrange(label, commands, "Already lined up");
    }

    pub(crate) fn toggle_hidden(&mut self) {
        if !self.editor.selection().is_empty() {
            let message = self.editor.toggle_hidden();
            self.notify(message);
        }
    }

    pub(crate) fn toggle_locked(&mut self) {
        if !self.editor.selection().is_empty() {
            let message = self.editor.toggle_locked();
            self.notify(message);
        }
    }

    pub(crate) fn show_all(&mut self) {
        let n = self.editor.show_all();
        self.notify(format!("Showed {n} hidden"));
    }

    pub(crate) fn unlock_all(&mut self) {
        let n = self.editor.unlock_all();
        self.notify(format!("Unlocked {n}"));
    }

    /// Runs `commands` as one step called `label`; `nothing` says why when there are none.
    /// Returns whether they all ran.
    fn run_arrange(&mut self, label: &str, commands: Vec<SceneCommand>, nothing: &str) -> bool {
        let Some(doc) = &mut self.doc else {
            return false;
        };
        if commands.is_empty() {
            self.notify(nothing.to_owned());
            return false;
        }
        let results = doc.execute_batch(label, &commands);
        match results.into_iter().find_map(Result::err) {
            Some(e) => {
                self.error = Some(format!("{label}: {e}"));
                false
            }
            None => {
                self.notify(label.to_owned());
                true
            }
        }
    }

    /// The arrange buttons over the selection's properties.
    pub(crate) fn arrange_bar(&mut self, ui: &mut egui::Ui) {
        let objects = (self.editor.selection().iter()).any(|s| matches!(s, Selection::Entity(_)));
        let several = self.editor.selection().len() > 1;
        let mut act = None;
        ui.horizontal_wrapped(|ui| {
            let buttons = [
                (
                    icons::ARROW_LINE_UP,
                    "Bring to front: draw over the rest on its spot",
                    objects,
                    0,
                ),
                (
                    icons::ARROW_LINE_DOWN,
                    "Send to back: draw under the rest on its spot",
                    objects,
                    1,
                ),
                (
                    icons::ALIGN_CENTER_HORIZONTAL,
                    "Line up in a row with the first selected",
                    several,
                    2,
                ),
                (
                    icons::ALIGN_CENTER_VERTICAL,
                    "Line up in a column with the first selected",
                    several,
                    3,
                ),
                (
                    icons::EYE_SLASH,
                    "Hide (H) · Shift+H shows everything",
                    true,
                    4,
                ),
                (
                    icons::LOCK,
                    "Lock: can't be picked (L) · Shift+L unlocks everything",
                    true,
                    5,
                ),
            ];
            for (icon, tip, enabled, id) in buttons {
                if ui
                    .add_enabled(enabled, egui::Button::new(icon))
                    .on_hover_text(tip)
                    .on_disabled_hover_text(tip)
                    .clicked()
                {
                    act = Some(id);
                }
            }
        });
        ui.label(
            egui::RichText::new("Arrows move it · Alt+arrows nudge a pixel")
                .color(theme::TEXT_DIM)
                .small(),
        );
        match act {
            Some(0) => self.reorder_selection(true),
            Some(1) => self.reorder_selection(false),
            Some(2) => self.align_selection(Line::Row),
            Some(3) => self.align_selection(Line::Column),
            Some(4) => self.toggle_hidden(),
            Some(5) => self.toggle_locked(),
            _ => {}
        }
    }
}
