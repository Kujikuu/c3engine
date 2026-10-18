//! c3-data: a game's data as typed tables (items, monsters, skills, levels, shop goods…).
//!
//! A [`Table`] is a list of named, typed columns and rows of values; the first column is the
//! row's key (a unique integer id), as in a database. Tables live in the project as readable
//! RON (`tables/<name>.table.ron`), are edited through commands (`c3-edit`) and can be
//! imported from SQL dumps ([`sql`]). Game code reads them by column name; a game module
//! decides which tables and columns its rules need.

mod read;
pub mod sql;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::io::{Error, ErrorKind, Result};
use std::path::Path;

/// File name ending of a table.
pub const TABLE_SUFFIX: &str = ".table.ron";
const FORMAT_VERSION: u32 = 1;

/// One cell's value. Written as a plain RON/JSON value (`5`, `1.5`, `"text"`, `true`).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl Value {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(v) => Some(*v),
            Self::Bool(b) => Some(i64::from(*b)),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(v) => Some(*v),
            Self::Int(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(t) => Some(t),
            _ => None,
        }
    }

    /// Whether this value fits a column of `kind`.
    pub fn fits(&self, kind: ColumnKind) -> bool {
        matches!(
            (self, kind),
            (Self::Bool(_), ColumnKind::Bool)
                | (Self::Int(_), ColumnKind::Int)
                | (Self::Float(_) | Self::Int(_), ColumnKind::Float)
                | (Self::Text(_), ColumnKind::Text)
        )
    }
}

// Read straight from the value's own type: serde's derived untagged reader buffers every
// value and takes tens of seconds on a large table.
impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = Value;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a number, text or true/false")
            }

            fn visit_bool<E>(self, v: bool) -> std::result::Result<Value, E> {
                Ok(Value::Bool(v))
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Value, E> {
                Ok(Value::Int(v))
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Value, E> {
                i64::try_from(v)
                    .map(Value::Int)
                    .map_err(|_| E::custom("number too large"))
            }

            fn visit_f64<E>(self, v: f64) -> std::result::Result<Value, E> {
                Ok(Value::Float(v))
            }

            fn visit_str<E>(self, v: &str) -> std::result::Result<Value, E> {
                Ok(Value::Text(v.to_owned()))
            }

            fn visit_string<E>(self, v: String) -> std::result::Result<Value, E> {
                Ok(Value::Text(v))
            }
        }
        d.deserialize_any(Visitor)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{b}"),
            Self::Int(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v}"),
            Self::Text(t) => f.write_str(t),
        }
    }
}

/// What a column holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ColumnKind {
    Int,
    Float,
    Text,
    Bool,
}

impl ColumnKind {
    /// The empty value of this kind (new rows, missing cells).
    pub fn default_value(self) -> Value {
        match self {
            Self::Int => Value::Int(0),
            Self::Float => Value::Float(0.0),
            Self::Text => Value::Text(String::new()),
            Self::Bool => Value::Bool(false),
        }
    }

    /// Reads `text` as a value of this kind (how people type cells).
    pub fn parse(self, text: &str) -> Option<Value> {
        let t = text.trim();
        match self {
            Self::Int => t.parse().ok().map(Value::Int),
            Self::Float => t.parse().ok().map(Value::Float),
            Self::Text => Some(Value::Text(text.to_owned())),
            Self::Bool => match t.to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" => Some(Value::Bool(true)),
                "false" | "0" | "no" => Some(Value::Bool(false)),
                _ => None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Column {
    pub name: String,
    pub kind: ColumnKind,
    /// What the column means, for editors and AI tools.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub doc: String,
}

impl Column {
    pub fn new(name: impl Into<String>, kind: ColumnKind) -> Self {
        Self {
            name: name.into(),
            kind,
            doc: String::new(),
        }
    }
}

/// Typed rows; the first column is each row's unique integer key. (Read by `read.rs`, which
/// reads each cell as its column's kind.)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Table {
    pub name: String,
    /// What the table is for.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub doc: String,
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Value>>,
}

/// Something wrong with a table, found by [`Table::problems`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    NoKeyColumn,
    WrongWidth { row: usize, cells: usize },
    WrongKind { row: usize, column: String },
    DuplicateKey(i64),
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoKeyColumn => write!(f, "the first column must be an integer key"),
            Self::WrongWidth { row, cells } => write!(f, "row {row} has {cells} cells"),
            Self::WrongKind { row, column } => {
                write!(f, "row {row}: the {column} value has the wrong kind")
            }
            Self::DuplicateKey(k) => write!(f, "key {k} is used by more than one row"),
        }
    }
}

impl Table {
    pub fn new(name: impl Into<String>, columns: Vec<Column>) -> Self {
        Self {
            name: name.into(),
            doc: String::new(),
            columns,
            rows: Vec::new(),
        }
    }

    pub fn column(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }

    /// The key of row `index`.
    pub fn key(&self, index: usize) -> Option<i64> {
        self.rows.get(index)?.first()?.as_int()
    }

    /// Index of the row with `key`.
    pub fn find(&self, key: i64) -> Option<usize> {
        (0..self.rows.len()).find(|&i| self.key(i) == Some(key))
    }

    /// The value in column `column` of the row with `key`.
    pub fn get(&self, key: i64, column: &str) -> Option<&Value> {
        let c = self.column(column)?;
        self.rows.get(self.find(key)?)?.get(c)
    }

    /// The lowest key above every key in use (for new rows).
    pub fn next_key(&self) -> i64 {
        (0..self.rows.len())
            .filter_map(|i| self.key(i))
            .max()
            .map_or(1, |k| k + 1)
    }

    /// A new row with `key` and empty values.
    pub fn empty_row(&self, key: i64) -> Vec<Value> {
        let mut row: Vec<Value> = self
            .columns
            .iter()
            .map(|c| c.kind.default_value())
            .collect();
        if let Some(first) = row.first_mut() {
            *first = Value::Int(key);
        }
        row
    }

    /// Everything wrong with the table (empty when it is sound).
    pub fn problems(&self) -> Vec<Problem> {
        let mut out = Vec::new();
        if self.columns.first().map(|c| c.kind) != Some(ColumnKind::Int) {
            out.push(Problem::NoKeyColumn);
        }
        let mut keys = HashSet::new();
        for (r, row) in self.rows.iter().enumerate() {
            if row.len() != self.columns.len() {
                out.push(Problem::WrongWidth {
                    row: r,
                    cells: row.len(),
                });
                continue;
            }
            for (value, column) in row.iter().zip(&self.columns) {
                if !value.fits(column.kind) {
                    out.push(Problem::WrongKind {
                        row: r,
                        column: column.name.clone(),
                    });
                }
            }
            if let Some(k) = self.key(r)
                && !keys.insert(k)
            {
                out.push(Problem::DuplicateKey(k));
            }
        }
        out
    }
}

/// The file form of a table (no `flatten`: it buffers the whole table and is very slow).
#[derive(Serialize, Deserialize)]
struct TableFile {
    format: u32,
    table: Table,
}

fn invalid(file: &Path, why: impl fmt::Display) -> Error {
    Error::new(ErrorKind::InvalidData, format!("{}: {why}", file.display()))
}

/// Writes `table` to `file` (`<name>.table.ron`), through a temporary file.
pub fn save_table(table: &Table, file: &Path) -> Result<()> {
    let text = TableFile {
        format: FORMAT_VERSION,
        table: table.clone(),
    };
    // Rows on one line each: readable and diff-able, not one value per line.
    let config = ron::ser::PrettyConfig::default().compact_arrays(true);
    let ron = ron::ser::to_string_pretty(&text, config).map_err(|e| invalid(file, e))?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension("tmp");
    std::fs::write(&tmp, ron)?;
    std::fs::rename(&tmp, file)
}

/// Reads the table at `file`.
pub fn load_table(file: &Path) -> Result<Table> {
    let text = std::fs::read_to_string(file)?;
    let parsed: TableFile = ron::from_str(&text).map_err(|e| invalid(file, e))?;
    if parsed.format > FORMAT_VERSION {
        return Err(invalid(
            file,
            format!(
                "format {} is newer than this engine supports",
                parsed.format
            ),
        ));
    }
    Ok(parsed.table)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Table {
        let mut t = Table::new(
            "items",
            vec![
                Column::new("id", ColumnKind::Int),
                Column::new("name", ColumnKind::Text),
                Column::new("price", ColumnKind::Int),
                Column::new("weight", ColumnKind::Float),
                Column::new("tradable", ColumnKind::Bool),
            ],
        );
        t.rows.push(vec![
            Value::Int(410003),
            Value::Text("Blade".into()),
            Value::Int(30),
            Value::Float(1.5),
            Value::Bool(true),
        ]);
        t.rows.push(t.empty_row(7));
        t
    }

    #[test]
    fn finds_rows_by_key_and_checks_itself() {
        let mut t = items();
        assert_eq!(t.get(410003, "name"), Some(&Value::Text("Blade".into())));
        assert_eq!(t.next_key(), 410004);
        assert!(t.problems().is_empty());
        t.rows.push(t.empty_row(7));
        t.rows[0][3] = Value::Text("heavy".into());
        assert_eq!(
            t.problems(),
            [
                Problem::WrongKind {
                    row: 0,
                    column: "weight".into()
                },
                Problem::DuplicateKey(7)
            ]
        );
        assert_eq!(ColumnKind::Bool.parse("Yes"), Some(Value::Bool(true)));
        assert_eq!(ColumnKind::Int.parse("x"), None);
    }

    #[test]
    fn tables_round_trip_through_ron() {
        let t = items();
        let dir = std::env::temp_dir().join(format!("c3-data-{}", std::process::id()));
        let file = dir.join(format!("items{TABLE_SUFFIX}"));
        save_table(&t, &file).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            text.contains("\"Blade\"") && text.contains("410003"),
            "{text}"
        );
        assert_eq!(load_table(&file).unwrap(), t);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
