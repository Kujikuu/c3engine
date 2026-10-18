//! Reading tables out of MySQL/MariaDB dumps (`CREATE TABLE` + `INSERT INTO … VALUES`), as
//! exported by Navicat or mysqldump. Column kinds come from the SQL types; `NULL` becomes the
//! column's empty value.

use crate::{Column, ColumnKind, Table, Value};
use std::collections::HashMap;

/// One SQL value: `None` for `NULL`.
pub type SqlValue = Option<String>;

/// A table found in a dump, with how many rows it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DumpTable {
    pub name: String,
    pub columns: usize,
    pub rows: usize,
}

/// The tables in `sql`, in the order they are created.
pub fn list_tables(sql: &str) -> Vec<DumpTable> {
    let mut tables: Vec<DumpTable> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut lines = sql.lines();
    while let Some(line) = lines.next() {
        if let Some(name) = create_table_name(line) {
            let columns = column_lines(&mut lines).len();
            index.insert(name.clone(), tables.len());
            tables.push(DumpTable {
                name,
                columns,
                rows: 0,
            });
        } else if let Some((name, rest)) = insert_target(line)
            && let Some(&i) = index.get(name)
        {
            tables[i].rows += count_tuples(rest);
        }
    }
    tables
}

/// The tables `names` of `sql` as engine tables (missing ones are left out).
pub fn import_tables(sql: &str, names: &[&str]) -> Vec<Table> {
    let mut tables: Vec<Table> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut lines = sql.lines();
    while let Some(line) = lines.next() {
        if let Some(name) = create_table_name(line) {
            if !names.contains(&name.as_str()) {
                continue;
            }
            let columns = column_lines(&mut lines)
                .into_iter()
                .filter_map(|l| column_from_line(&l))
                .collect();
            index.insert(name.clone(), tables.len());
            tables.push(Table::new(name, columns));
        } else if let Some((name, rest)) = insert_target(line)
            && let Some(&i) = index.get(name)
        {
            let table = &mut tables[i];
            for tuple in tuples(rest) {
                if tuple.len() == table.columns.len() {
                    let row = tuple
                        .into_iter()
                        .zip(&table.columns)
                        .map(|(v, c)| typed(v, c.kind))
                        .collect();
                    table.rows.push(row);
                }
            }
        }
    }
    tables
}

/// `CREATE TABLE `name` (` → `name`.
fn create_table_name(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("CREATE TABLE ")?;
    let rest = rest.trim_start_matches("IF NOT EXISTS ");
    let name = rest.trim_start_matches('`').split(['`', ' ', '(']).next()?;
    Some(name.to_owned())
}

/// The column-definition lines after a `CREATE TABLE` line, up to its closing `)`.
fn column_lines<'a>(lines: &mut impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines {
        let t = line.trim();
        if t.starts_with(')') {
            break;
        }
        if t.starts_with('`') {
            out.push(t.to_owned());
        }
    }
    out
}

/// `` `price` int(4) unsigned default '0', `` → a column.
fn column_from_line(line: &str) -> Option<Column> {
    let rest = line.strip_prefix('`')?;
    let (name, rest) = rest.split_once('`')?;
    let sql_type = rest.trim_start().to_ascii_lowercase();
    let kind = if sql_type.starts_with("float")
        || sql_type.starts_with("double")
        || sql_type.starts_with("decimal")
        || sql_type.starts_with("real")
    {
        ColumnKind::Float
    } else if ["tinyint", "smallint", "mediumint", "bigint", "int", "bit"]
        .iter()
        .any(|t| sql_type.starts_with(t))
    {
        ColumnKind::Int
    } else {
        ColumnKind::Text
    };
    Some(Column::new(name, kind))
}

/// `INSERT INTO `name` VALUES (...)` → (`name`, `(...)...`).
fn insert_target(line: &str) -> Option<(&str, &str)> {
    let rest = line.trim_start().strip_prefix("INSERT INTO `")?;
    let (name, rest) = rest.split_once('`')?;
    // Skip an optional column list: `INSERT INTO `t` (`a`, `b`) VALUES`.
    let rest = rest.split_once("VALUES")?.1.trim_start();
    Some((name, rest))
}

fn count_tuples(rest: &str) -> usize {
    tuples(rest).len()
}

/// Every `(…)` tuple in `rest`, e.g. `(1, 'a'), (2, 'b');`.
fn tuples(rest: &str) -> Vec<Vec<SqlValue>> {
    let mut out = Vec::new();
    let mut s = rest;
    while let Some(start) = s.find('(') {
        match parse_values(&s[start + 1..]) {
            Some((values, used)) => {
                out.push(values);
                s = &s[start + 1 + used..];
            }
            None => break,
        }
    }
    out
}

/// Parses `'a', 12, null, 'it\'s')` up to and including the closing parenthesis; returns
/// the values and the bytes used.
pub fn parse_values(s: &str) -> Option<(Vec<SqlValue>, usize)> {
    let mut values = Vec::new();
    let mut chars = s.char_indices().peekable();
    let skip_spaces = |chars: &mut std::iter::Peekable<std::str::CharIndices>| {
        while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
            chars.next();
        }
    };
    loop {
        skip_spaces(&mut chars);
        match chars.peek()?.1 {
            '\'' => {
                chars.next();
                let mut text = String::new();
                loop {
                    match chars.next()?.1 {
                        '\\' => text.push(unescape(chars.next()?.1)),
                        '\'' if chars.peek().is_some_and(|(_, c)| *c == '\'') => {
                            chars.next();
                            text.push('\'');
                        }
                        '\'' => break,
                        c => text.push(c),
                    }
                }
                values.push(Some(text));
            }
            _ => {
                let mut word = String::new();
                while let Some(&(_, c)) = chars.peek() {
                    if c == ',' || c == ')' {
                        break;
                    }
                    word.push(c);
                    chars.next();
                }
                let word = word.trim();
                values.push((!word.eq_ignore_ascii_case("null")).then(|| word.to_owned()));
            }
        }
        skip_spaces(&mut chars);
        match chars.next()? {
            (_, ',') => {}
            (i, ')') => return Some((values, i + 1)),
            _ => return None,
        }
    }
}

/// MySQL escapes: `\n`, `\t`, `\0`, others stand for themselves.
fn unescape(c: char) -> char {
    match c {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '0' => '\0',
        other => other,
    }
}

fn typed(value: SqlValue, kind: ColumnKind) -> Value {
    value
        .and_then(|v| kind.parse(&v))
        .unwrap_or_else(|| kind.default_value())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMP: &str = r#"
-- comment
CREATE TABLE `cq_itemtype` (
  `id` int(4) unsigned zerofill NOT NULL default '0000',
  `name` varchar(15) NOT NULL default '',
  `weight` float NOT NULL default '0',
  PRIMARY KEY  (`id`)
) ENGINE=MyISAM;
INSERT INTO `cq_itemtype` VALUES ('0410003', 'Blade', '1.5');
INSERT INTO `cq_itemtype` VALUES ('0410013', 'It\'s, a (test)', null), ('7', 'x', '2');
CREATE TABLE `cq_other` (
  `id` int(4) NOT NULL
) ENGINE=MyISAM;
INSERT INTO `cq_other` VALUES ('1');
"#;

    #[test]
    fn lists_and_imports_tables() {
        assert_eq!(
            list_tables(DUMP),
            [
                DumpTable {
                    name: "cq_itemtype".into(),
                    columns: 3,
                    rows: 3
                },
                DumpTable {
                    name: "cq_other".into(),
                    columns: 1,
                    rows: 1
                }
            ]
        );
        let tables = import_tables(DUMP, &["cq_itemtype"]);
        assert_eq!(tables.len(), 1);
        let t = &tables[0];
        let kinds: Vec<_> = t.columns.iter().map(|c| c.kind).collect();
        assert_eq!(
            kinds,
            [ColumnKind::Int, ColumnKind::Text, ColumnKind::Float]
        );
        assert_eq!(t.rows.len(), 3);
        assert_eq!(t.get(410003, "weight"), Some(&Value::Float(1.5)));
        assert_eq!(
            t.get(410013, "name"),
            Some(&Value::Text("It's, a (test)".into()))
        );
        assert_eq!(t.get(410013, "weight"), Some(&Value::Float(0.0)));
        assert!(t.problems().is_empty());
    }
}
