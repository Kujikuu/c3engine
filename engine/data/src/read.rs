//! Reading a [`Table`] with its schema: each cell is read as its column's kind. Generic
//! "any value" reading is very slow in RON on large tables (minutes), typed reading is fast.

use crate::{Column, ColumnKind, Table, Value};
use serde::Deserialize;
use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use std::fmt;

impl<'de> Deserialize<'de> for Table {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_struct("Table", &["name", "doc", "columns", "rows"], TableVisitor)
    }
}

struct TableVisitor;

impl<'de> Visitor<'de> for TableVisitor {
    type Value = Table;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a table (name, columns, rows)")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Table, A::Error> {
        let mut name = None;
        let mut doc = String::new();
        let mut columns: Option<Vec<Column>> = None;
        let mut rows = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "name" => name = Some(map.next_value()?),
                "doc" => doc = map.next_value()?,
                "columns" => columns = Some(map.next_value()?),
                "rows" => {
                    let columns = columns
                        .as_deref()
                        .ok_or_else(|| de::Error::custom("columns must come before rows"))?;
                    rows = Some(map.next_value_seed(Rows(columns))?);
                }
                _ => {
                    map.next_value::<de::IgnoredAny>()?;
                }
            }
        }
        Ok(Table {
            name: name.ok_or_else(|| de::Error::missing_field("name"))?,
            doc,
            columns: columns.ok_or_else(|| de::Error::missing_field("columns"))?,
            rows: rows.unwrap_or_default(),
        })
    }
}

/// Every row, typed by `columns`.
struct Rows<'a>(&'a [Column]);

impl<'de> DeserializeSeed<'de> for Rows<'_> {
    type Value = Vec<Vec<Value>>;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for Rows<'_> {
    type Value = Vec<Vec<Value>>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a list of rows")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut rows = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(row) = seq.next_element_seed(Row(self.0))? {
            rows.push(row);
        }
        Ok(rows)
    }
}

/// One row: one value per column, each read as its column's kind.
struct Row<'a>(&'a [Column]);

impl<'de> DeserializeSeed<'de> for Row<'_> {
    type Value = Vec<Value>;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for Row<'_> {
    type Value = Vec<Value>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a row of {} values", self.0.len())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut row = Vec::with_capacity(self.0.len());
        for column in self.0 {
            let value = seq
                .next_element_seed(Cell(column.kind))?
                .ok_or_else(|| de::Error::invalid_length(row.len(), &self))?;
            row.push(value);
        }
        if seq.next_element::<de::IgnoredAny>()?.is_some() {
            return Err(de::Error::custom(format!(
                "a row has more than {} values",
                self.0.len()
            )));
        }
        Ok(row)
    }
}

/// One cell, read as `kind`.
struct Cell(ColumnKind);

impl<'de> DeserializeSeed<'de> for Cell {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        Ok(match self.0 {
            ColumnKind::Int => Value::Int(i64::deserialize(d)?),
            ColumnKind::Float => Value::Float(f64::deserialize(d)?),
            ColumnKind::Text => Value::Text(String::deserialize(d)?),
            ColumnKind::Bool => Value::Bool(bool::deserialize(d)?),
        })
    }
}
