//! Script editing: a command that sets a script's text, applied by [`ScriptDoc`] with undo.
//! Typing in an editor arrives as many small settings of the text; [`ScriptDoc::type_text`]
//! merges a run of them into one undo step, as text editors do.

use crate::history::{History, HistoryEntry};
use crate::{EditError, Outcome};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A change to a script. Serialized as `{"op": "set_text", "text": "fn talk() { ... }"}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ScriptCommand {
    /// Replaces the script's whole text.
    SetText { text: String },
}

struct Change {
    before: String,
    after: String,
}

/// A script open for editing.
pub struct ScriptDoc {
    text: String,
    history: History<Change>,
    /// Typing goes on: the next typed text joins the last step.
    typing: bool,
}

impl ScriptDoc {
    pub fn new(text: String) -> Self {
        Self {
            text,
            history: History::default(),
            typing: false,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Applies `command`, recording it for undo.
    pub fn execute(&mut self, command: &ScriptCommand) -> Result<Outcome, EditError> {
        let ScriptCommand::SetText { text } = command;
        Ok(self.set_text(text.clone(), "Set the script's text"))
    }

    /// Sets the text as one undo step called `label` (an editor's insert, a paste).
    pub fn set_text(&mut self, text: String, label: &str) -> Outcome {
        self.end_typing();
        self.set(text, label)
    }

    /// The editor's text after typing; a run of typing is one undo step until
    /// [`Self::end_typing`].
    pub fn type_text(&mut self, text: String) {
        if text == self.text {
            return;
        }
        if !self.typing {
            self.history.begin_group("Typing".into());
            self.typing = true;
        }
        self.set(text, "Typing");
    }

    /// Ends the run of typing (focus left the editor, or a pause).
    pub fn end_typing(&mut self) {
        if self.typing {
            self.history.end_group();
            self.typing = false;
        }
    }

    fn set(&mut self, text: String, label: &str) -> Outcome {
        if text == self.text {
            return Outcome {
                summary: "Nothing changed".into(),
                changed: false,
            };
        }
        let before = std::mem::replace(&mut self.text, text.clone());
        self.history.record(
            label.to_owned(),
            Change {
                before,
                after: text,
            },
        );
        Outcome {
            summary: label.to_owned(),
            changed: true,
        }
    }

    // ---- history ----

    /// Reverts the last step; returns its label.
    pub fn undo(&mut self) -> Option<String> {
        self.end_typing();
        let step = self.history.take_undo()?;
        if let Some(first) = step.changes.first() {
            self.text = first.before.clone();
        }
        let label = step.label.clone();
        self.history.undone(step);
        Some(label)
    }

    /// Reapplies the last undone step; returns its label.
    pub fn redo(&mut self) -> Option<String> {
        self.end_typing();
        let step = self.history.take_redo()?;
        if let Some(last) = step.changes.last() {
            self.text = last.after.clone();
        }
        let label = step.label.clone();
        self.history.redone(step);
        Some(label)
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.history.undo_label()
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.history.redo_label()
    }

    pub fn history(&self) -> Vec<HistoryEntry> {
        self.history.entries()
    }

    pub fn is_dirty(&self) -> bool {
        self.history.is_dirty()
    }

    pub fn mark_saved(&mut self) {
        self.end_typing();
        self.history.mark_saved();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_runs_undo_as_one_step() {
        let mut doc = ScriptDoc::new("fn talk() {}".into());
        doc.type_text("fn talk() { }".into());
        doc.type_text("fn talk() { t }".into());
        doc.end_typing();
        doc.execute(&ScriptCommand::SetText {
            text: "// new".into(),
        })
        .unwrap();
        assert_eq!(doc.undo().as_deref(), Some("Set the script's text"));
        assert_eq!(doc.text(), "fn talk() { t }");
        assert_eq!(doc.undo().as_deref(), Some("Typing"));
        assert_eq!(doc.text(), "fn talk() {}");
        assert!(!doc.is_dirty());
        doc.redo();
        assert!(doc.is_dirty());
    }
}
