//! The project's tables as scripts read them: each loaded on first use and kept.

use crate::api::to_dynamic;
use c3_data::Table;
use rhai::{Dynamic, EvalAltResult};
use std::collections::HashMap;
use std::path::PathBuf;

pub(crate) struct Tables {
    /// The project's `tables/` folder; `None` when the host has no tables.
    dir: Option<PathBuf>,
    loaded: HashMap<String, Table>,
}

impl Tables {
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self {
            dir,
            loaded: HashMap::new(),
        }
    }

    /// Forgets loaded tables, so the next read sees the files as saved now.
    pub fn clear(&mut self) {
        self.loaded.clear();
    }

    pub fn table(&mut self, name: &str) -> Result<&Table, Box<EvalAltResult>> {
        if !self.loaded.contains_key(name) {
            let dir = self.dir.as_ref().ok_or("this game has no tables")?;
            if name.contains(['/', '\\', '.']) {
                return Err(format!("\"{name}\" is not a table name").into());
            }
            let file = dir.join(format!("{name}{}", c3_data::TABLE_SUFFIX));
            let table = c3_data::load_table(&file)
                .map_err(|e| format!("table \"{name}\" could not be read: {e}"))?;
            self.loaded.insert(name.to_owned(), table);
        }
        Ok(&self.loaded[name])
    }

    /// The value in `column` of row `key`, or `()` when the row doesn't exist.
    pub fn lookup(
        &mut self,
        name: &str,
        key: i64,
        column: &str,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        let table = self.table(name)?;
        let c = table
            .column(column)
            .ok_or_else(|| format!("table \"{name}\" has no column \"{column}\""))?;
        Ok(table
            .find(key)
            .map_or(Dynamic::UNIT, |row| to_dynamic(&table.rows[row][c])))
    }
}
