//! Undo history: labelled steps of recorded changes, groups that merge many commands into
//! one step, and a saved marker for "unsaved changes".

use serde::Serialize;

/// One undo step: the changes it made, oldest first.
pub(crate) struct Step<C> {
    pub label: String,
    pub changes: Vec<C>,
}

/// A step as listed in a history panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistoryEntry {
    pub label: String,
    /// False for undone steps (redo would bring them back).
    pub done: bool,
}

pub(crate) struct History<C> {
    undo: Vec<Step<C>>,
    redo: Vec<Step<C>>,
    /// Group being recorded; becomes one step when it ends.
    group: Option<Step<C>>,
    /// `undo.len()` when last saved; `None` once that state can't be reached again.
    saved_at: Option<usize>,
}

impl<C> Default for History<C> {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            group: None,
            saved_at: Some(0),
        }
    }
}

impl<C> History<C> {
    /// Starts merging recorded changes into one step called `label`.
    pub fn begin_group(&mut self, label: String) {
        self.end_group();
        self.group = Some(Step {
            label,
            changes: Vec::new(),
        });
    }

    /// Closes the group; an empty group leaves no step.
    pub fn end_group(&mut self) {
        if let Some(step) = self.group.take()
            && !step.changes.is_empty()
        {
            self.push(step);
        }
    }

    pub fn record(&mut self, label: String, change: C) {
        match &mut self.group {
            Some(group) => group.changes.push(change),
            None => self.push(Step {
                label,
                changes: vec![change],
            }),
        }
    }

    fn push(&mut self, step: Step<C>) {
        if self.saved_at.is_some_and(|s| s > self.undo.len()) {
            self.saved_at = None; // the saved state was in the redo stack being dropped
        }
        self.redo.clear();
        self.undo.push(step);
    }

    /// Removes the step to undo; hand it back with [`History::undone`] once reverted.
    pub fn take_undo(&mut self) -> Option<Step<C>> {
        self.end_group();
        self.undo.pop()
    }

    pub fn undone(&mut self, step: Step<C>) {
        self.redo.push(step);
    }

    /// Removes the step to redo; hand it back with [`History::redone`] once reapplied.
    pub fn take_redo(&mut self) -> Option<Step<C>> {
        self.end_group();
        self.redo.pop()
    }

    pub fn redone(&mut self, step: Step<C>) {
        self.undo.push(step);
    }

    pub fn undo_label(&self) -> Option<&str> {
        match &self.group {
            Some(g) if !g.changes.is_empty() => Some(&g.label),
            _ => self.undo.last().map(|s| s.label.as_str()),
        }
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.label.as_str())
    }

    pub fn is_dirty(&self) -> bool {
        self.saved_at != Some(self.undo.len())
            || self.group.as_ref().is_some_and(|g| !g.changes.is_empty())
    }

    pub fn mark_saved(&mut self) {
        self.end_group();
        self.saved_at = Some(self.undo.len());
    }

    /// Every step, oldest first: done ones, the group in progress, then undone ones.
    pub fn entries(&self) -> Vec<HistoryEntry> {
        let done = self
            .undo
            .iter()
            .chain(self.group.as_ref().filter(|g| !g.changes.is_empty()));
        done.map(|s| HistoryEntry {
            label: s.label.clone(),
            done: true,
        })
        .chain(self.redo.iter().rev().map(|s| HistoryEntry {
            label: s.label.clone(),
            done: false,
        }))
        .collect()
    }
}
