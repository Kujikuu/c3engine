//! Documents edited as one whole value (an effect file, an animation set): each change sets
//! the value anew and is one undo step, named by the editor. A drag or a text field's run of
//! typing merges into one step between [`ValueDoc::begin_group`] and [`ValueDoc::end_group`].

use crate::Outcome;
use crate::history::{History, HistoryEntry};

struct Change<T> {
    before: T,
    after: T,
}

/// A value open for editing, with undo.
pub struct ValueDoc<T> {
    value: T,
    history: History<Change<T>>,
}

impl<T: Clone + PartialEq> ValueDoc<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            history: History::default(),
        }
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    /// Makes the value `value`, as a step called `label`.
    pub fn set(&mut self, value: T, label: &str) -> Outcome {
        if value == self.value {
            return Outcome {
                summary: "Nothing changed".into(),
                changed: false,
            };
        }
        let before = std::mem::replace(&mut self.value, value.clone());
        self.history.record(
            label.to_owned(),
            Change {
                before,
                after: value,
            },
        );
        Outcome {
            summary: label.to_owned(),
            changed: true,
        }
    }

    /// Merges the following changes into one step called `label`, until [`Self::end_group`].
    pub fn begin_group(&mut self, label: &str) {
        self.history.begin_group(label.to_owned());
    }

    pub fn end_group(&mut self) {
        self.history.end_group();
    }

    /// Reverts the last step; returns its label.
    pub fn undo(&mut self) -> Option<String> {
        let step = self.history.take_undo()?;
        if let Some(first) = step.changes.first() {
            self.value = first.before.clone();
        }
        let label = step.label.clone();
        self.history.undone(step);
        Some(label)
    }

    /// Reapplies the last undone step; returns its label.
    pub fn redo(&mut self) -> Option<String> {
        let step = self.history.take_redo()?;
        if let Some(last) = step.changes.last() {
            self.value = last.after.clone();
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
        self.history.mark_saved();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_set_group_and_undo() {
        let mut doc = ValueDoc::new(1);
        doc.set(2, "Two");
        doc.begin_group("Drag");
        doc.set(3, "x");
        doc.set(4, "x");
        doc.end_group();
        assert!(!doc.set(4, "same").changed);
        assert_eq!(doc.undo().as_deref(), Some("Drag"));
        assert_eq!(*doc.value(), 2);
        doc.undo();
        assert_eq!(*doc.value(), 1);
        doc.redo();
        doc.redo();
        assert_eq!(*doc.value(), 4);
    }
}
