//! Table (game data) editing: commands on one table, applied by [`TableDoc`] with undo.

use crate::history::{History, HistoryEntry};
use crate::{EditError, Outcome};
use c3_data::{Table, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A change to a table. Serialized as `{"op": "set_cell", ...}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TableCommand {
    /// Sets one cell of the row with `key`.
    SetCell {
        key: i64,
        column: String,
        value: Value,
    },
    /// Adds a row: `values` in column order, or else an empty row with `key` (or the next
    /// free key).
    AddRow {
        #[serde(default)]
        values: Option<Vec<Value>>,
        #[serde(default)]
        key: Option<i64>,
    },
    /// Deletes the row with `key`.
    RemoveRow { key: i64 },
}

/// A recorded change, with what it replaced.
enum Change {
    Cell {
        row: usize,
        column: usize,
        before: Value,
        after: Value,
    },
    /// Row `index` added (`before` none) or removed (`after` none).
    Row {
        index: usize,
        before: Option<Vec<Value>>,
        after: Option<Vec<Value>>,
    },
}

type Edit = Result<(String, Option<Change>), EditError>;

/// An open table: every change goes through [`TableDoc::execute`] and can be undone.
pub struct TableDoc {
    table: Table,
    history: History<Change>,
}

impl TableDoc {
    pub fn new(table: Table) -> Self {
        Self {
            table,
            history: History::default(),
        }
    }

    pub fn table(&self) -> &Table {
        &self.table
    }

    /// Applies `command`, recording it for undo. Refused commands change nothing.
    pub fn execute(&mut self, command: &TableCommand) -> Result<Outcome, EditError> {
        let (summary, change) = match command {
            TableCommand::SetCell { key, column, value } => self.set_cell(*key, column, value)?,
            TableCommand::AddRow { values, key } => self.add_row(values.as_deref(), *key)?,
            TableCommand::RemoveRow { key } => self.remove_row(*key)?,
        };
        let changed = change.is_some();
        if let Some(change) = change {
            self.history.record(summary.clone(), change);
        }
        Ok(Outcome { summary, changed })
    }

    fn row(&self, key: i64) -> Result<usize, EditError> {
        self.table.find(key).ok_or(EditError::NoRow(key))
    }

    fn set_cell(&mut self, key: i64, column: &str, value: &Value) -> Edit {
        let row = self.row(key)?;
        let c = self
            .table
            .column(column)
            .ok_or_else(|| EditError::NoColumn(column.to_owned()))?;
        let kind = self.table.columns[c].kind;
        if !value.fits(kind) {
            return Err(EditError::WrongKind(column.to_owned()));
        }
        // A whole number typed into a decimal column is stored as a decimal.
        let value = match (value, kind) {
            (Value::Int(v), c3_data::ColumnKind::Float) => Value::Float(*v as f64),
            _ => value.clone(),
        };
        if c == 0 {
            let new_key = value.as_int().unwrap_or(key);
            if new_key != key && self.table.find(new_key).is_some() {
                return Err(EditError::KeyTaken(new_key));
            }
        }
        let before = self.table.rows[row][c].clone();
        if before == value {
            return Ok(("Nothing changed".into(), None));
        }
        self.table.rows[row][c] = value.clone();
        Ok((
            format!("Set {column} of {key} to {value}"),
            Some(Change::Cell {
                row,
                column: c,
                before,
                after: value,
            }),
        ))
    }

    fn add_row(&mut self, values: Option<&[Value]>, key: Option<i64>) -> Edit {
        let row = match values {
            Some(values) => {
                if values.len() != self.table.columns.len() {
                    return Err(EditError::WrongWidth(values.len()));
                }
                for (v, c) in values.iter().zip(&self.table.columns) {
                    if !v.fits(c.kind) {
                        return Err(EditError::WrongKind(c.name.clone()));
                    }
                }
                values.to_vec()
            }
            None => self
                .table
                .empty_row(key.unwrap_or_else(|| self.table.next_key())),
        };
        let key = row.first().and_then(Value::as_int).unwrap_or_default();
        if self.table.find(key).is_some() {
            return Err(EditError::KeyTaken(key));
        }
        let index = self.table.rows.len();
        self.table.rows.push(row.clone());
        Ok((
            format!("Added row {key}"),
            Some(Change::Row {
                index,
                before: None,
                after: Some(row),
            }),
        ))
    }

    fn remove_row(&mut self, key: i64) -> Edit {
        let index = self.row(key)?;
        let row = self.table.rows.remove(index);
        Ok((
            format!("Deleted row {key}"),
            Some(Change::Row {
                index,
                before: Some(row),
                after: None,
            }),
        ))
    }

    fn apply(&mut self, change: &Change, forward: bool) {
        match change {
            Change::Cell {
                row,
                column,
                before,
                after,
            } => {
                self.table.rows[*row][*column] = if forward { after } else { before }.clone();
            }
            Change::Row {
                index,
                before,
                after,
            } => {
                let (from, to) = if forward {
                    (before, after)
                } else {
                    (after, before)
                };
                match (from, to) {
                    (None, Some(row)) => self.table.rows.insert(*index, row.clone()),
                    (Some(_), None) => {
                        self.table.rows.remove(*index);
                    }
                    _ => {}
                }
            }
        }
    }

    // ---- history ----

    /// Merges the following commands into one undo step called `label`.
    pub fn begin_group(&mut self, label: impl Into<String>) {
        self.history.begin_group(label.into());
    }

    pub fn end_group(&mut self) {
        self.history.end_group();
    }

    /// Reverts the last step; returns its label.
    pub fn undo(&mut self) -> Option<String> {
        let step = self.history.take_undo()?;
        for change in step.changes.iter().rev() {
            self.apply(change, false);
        }
        let label = step.label.clone();
        self.history.undone(step);
        Some(label)
    }

    /// Reapplies the last undone step; returns its label.
    pub fn redo(&mut self) -> Option<String> {
        let step = self.history.take_redo()?;
        for change in &step.changes {
            self.apply(change, true);
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
    use c3_data::{Column, ColumnKind};

    fn doc() -> TableDoc {
        let mut t = Table::new(
            "items",
            vec![
                Column::new("id", ColumnKind::Int),
                Column::new("name", ColumnKind::Text),
                Column::new("weight", ColumnKind::Float),
            ],
        );
        t.rows.push(vec![
            Value::Int(1),
            Value::Text("Blade".into()),
            Value::Float(1.5),
        ]);
        TableDoc::new(t)
    }

    #[test]
    fn cells_and_rows_with_checks_and_undo() {
        let mut d = doc();
        let set = |key, column: &str, value| TableCommand::SetCell {
            key,
            column: column.into(),
            value,
        };
        d.execute(&set(1, "name", Value::Text("Sword".into())))
            .unwrap();
        assert_eq!(
            d.execute(&set(1, "name", Value::Int(3))),
            Err(EditError::WrongKind("name".into()))
        );
        d.execute(&set(1, "weight", Value::Int(2))).unwrap();
        assert_eq!(d.table().get(1, "weight"), Some(&Value::Float(2.0)));
        d.execute(&TableCommand::AddRow {
            values: None,
            key: None,
        })
        .unwrap();
        assert_eq!(d.table().key(1), Some(2));
        assert_eq!(
            d.execute(&set(2, "id", Value::Int(1))),
            Err(EditError::KeyTaken(1))
        );
        d.execute(&TableCommand::RemoveRow { key: 1 }).unwrap();
        assert_eq!(d.table().rows.len(), 1);
        while d.undo().is_some() {}
        assert_eq!(d.table().get(1, "name"), Some(&Value::Text("Blade".into())));
        assert_eq!(d.table().rows.len(), 1);
        while d.redo().is_some() {}
        assert_eq!(d.table().key(0), Some(2));
    }
}
