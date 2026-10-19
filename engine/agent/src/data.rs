//! Table and settings tools: reading rows and settings, editing them, making tables.

use crate::tools::{Tool, answer, arg, opt_arg, outcomes, tool};
use crate::workspace::{DocRef, Workspace};
use c3_data::{Column, Table};
use c3_edit::{SettingsCommand, TableCommand};
use serde_json::{Map, Value, json};

/// Rows `read_table` gives at most.
const MAX_ROWS: usize = 200;

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        tool(
            "read_table",
            "Reads a table: its columns (name, kind, meaning, which table an id column refers \
             to) and rows as objects, a page at a time (at most 200), or the rows with `keys`, \
             or the rows where some cell contains `search`.",
            json!({
                "table": { "type": "string" },
                "keys": { "type": "array", "items": { "type": "integer" } },
                "search": { "type": "string" },
                "offset": { "type": "integer" },
                "limit": { "type": "integer" },
            }),
            &["table"],
            read_table,
        ),
        tool(
            "edit_table",
            "Changes a table with TableCommands (see command_schemas kind table), one undo \
             step: set_cell, add_row, remove_row, add_column, rename_column, set_column, \
             remove_column.",
            json!({
                "table": { "type": "string" },
                "commands": { "type": "array", "items": { "type": "object" } },
                "label": { "type": "string" },
            }),
            &["table", "commands"],
            edit_table,
        ),
        tool(
            "create_table",
            "Makes a new table. Its first column is the key (an integer); give the others as \
             {name, kind: int|float|text|bool, doc, references}. With `template` true and no \
             columns, the game's own template of that table (with an example row) is used.",
            json!({
                "name": { "type": "string" },
                "doc": { "type": "string" },
                "columns": { "type": "array", "items": { "type": "object" } },
                "template": { "type": "boolean" },
            }),
            &["name"],
            create_table,
        ),
        tool(
            "import_server_data",
            "Imports the game's own server data in one step (Conquer: a TQ SQL dump such as              cq_finalized_mariadb.sql, given by its path on disk): the map list into the              settings, every map's NPCs, spawn areas and portal destinations into its              population (merged with the project's own NPCs and scripts), and the game-data              tables. Writes the files at once (the map list waits for save) and answers the              report, also written to import_report.txt.",
            json!({ "file": { "type": "string" } }),
            &["file"],
            import_server_data,
        ),
        tool(
            "read_settings",
            "Reads Project Settings: the setting at `path` (sections and fields joined by dots, \
             as `network.port` or `characters`), or all of them with what is wrong.",
            json!({ "path": { "type": "string" } }),
            &[],
            read_settings,
        ),
        tool(
            "edit_settings",
            "Changes Project Settings with SettingsCommands, one undo step: \
             {\"op\":\"set\",\"path\":\"network.port\",\"value\":7200} or \
             {\"op\":\"reset\",\"path\":\"camera\"}.",
            json!({ "commands": { "type": "array", "items": { "type": "object" } } }),
            &["commands"],
            edit_settings,
        ),
    ]
}

/// Row `row` of `table` as an object by column.
fn row_object(table: &Table, row: &[c3_data::Value]) -> Value {
    let cells = (table.columns.iter().zip(row))
        .map(|(c, v)| (c.name.clone(), serde_json::to_value(v).unwrap_or_default()));
    Value::Object(cells.collect::<Map<_, _>>())
}

fn read_table(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let name: String = arg(args, "table")?;
    let keys: Option<Vec<i64>> = opt_arg(args, "keys")?;
    let search = opt_arg::<String>(args, "search")?.map(|s| s.to_lowercase());
    let offset = opt_arg::<usize>(args, "offset")?.unwrap_or(0);
    let limit = opt_arg::<usize>(args, "limit")?.unwrap_or(50).min(MAX_ROWS);
    let table = ws.table(&name)?.table();
    let matches = |row: &&Vec<c3_data::Value>| {
        let key_ok = keys.as_ref().is_none_or(|k| {
            row.first()
                .and_then(c3_data::Value::as_int)
                .is_some_and(|v| k.contains(&v))
        });
        let search_ok = search
            .as_ref()
            .is_none_or(|s| row.iter().any(|v| v.to_string().to_lowercase().contains(s)));
        key_ok && search_ok
    };
    let found: Vec<&Vec<c3_data::Value>> = table.rows.iter().filter(matches).collect();
    let rows: Vec<Value> = (found.iter().skip(offset).take(limit))
        .map(|r| row_object(table, r))
        .collect();
    Ok(json!({
        "name": table.name,
        "doc": table.doc,
        "columns": table.columns,
        "rows": rows,
        "matching": found.len(),
        "total": table.rows.len(),
        "problems": table.problems().iter().map(ToString::to_string).collect::<Vec<_>>(),
    }))
}

fn edit_table(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let name: String = arg(args, "table")?;
    let commands: Vec<TableCommand> = arg(args, "commands")?;
    let label = opt_arg::<String>(args, "label")?.unwrap_or_else(|| "Agent edit".into());
    let doc = ws.table(&name)?;
    doc.begin_group(label);
    let results: Vec<_> = commands.iter().map(|c| doc.execute(c)).collect();
    doc.end_group();
    ws.changed(&DocRef::Table(name));
    outcomes(results)
}

fn create_table(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let name: String = arg(args, "name")?;
    let columns: Option<Vec<Column>> = opt_arg(args, "columns")?;
    let template = opt_arg::<bool>(args, "template")?.unwrap_or(false);
    let file = ws.project().table_file(&name);
    if file.exists() {
        return Err(format!("the project already has a table \"{name}\""));
    }
    let table = match columns {
        Some(columns) => {
            let key = Column::new("id", c3_data::ColumnKind::Int);
            let mut table = Table::new(name.clone(), [vec![key], columns].concat());
            table.doc = opt_arg(args, "doc")?.unwrap_or_default();
            table
        }
        None if template => (ws.game().table_templates().into_iter())
            .find(|t| t.name == name)
            .ok_or_else(|| format!("the game has no template of a table \"{name}\""))?,
        None => return Err("give `columns`, or `template` true for the game's own table".into()),
    };
    (file.parent())
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| c3_data::save_table(&table, &file))
        .map_err(|e| e.to_string())?;
    ws.refresh();
    Ok(json!({ "made": format!("table {name}"), "columns": table.columns }))
}

fn import_server_data(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let file: String = arg(args, "file")?;
    let bytes = std::fs::read(&file).map_err(|e| format!("can't read {file}: {e}"))?;
    let data = (ws
        .game()
        .import_server_data(&bytes))
    .ok_or_else(|| format!("{} has no server data to import", ws.game().name()))?;
    let project = ws.project().clone();
    let listed = ws.settings()?.settings().maps.clone();
    let written = c3_play::server_data::write(&project, &data, &listed, &file);
    let command = SettingsCommand::Set {
        path: "maps".into(),
        value: serde_json::to_value(&written.maps).map_err(|e| e.to_string())?,
    };
    let doc = ws.settings()?;
    doc.begin_group("Import server data");
    let listed = doc.execute(&command).map_err(|e| e.to_string());
    doc.end_group();
    listed?;
    ws.changed(&DocRef::Settings);
    ws.refresh();
    Ok(json!({ "report": written.report, "problems": written.problems }))
}

fn read_settings(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let path: Option<String> = opt_arg(args, "path")?;
    let doc = ws.settings()?;
    match path {
        Some(path) => doc.get(&path).map_err(|e| e.to_string()),
        None => Ok(json!({
            "settings": answer(doc.settings())?,
            "problems": doc.settings().problems(),
        })),
    }
}

fn edit_settings(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let commands: Vec<SettingsCommand> = arg(args, "commands")?;
    let doc = ws.settings()?;
    doc.begin_group("Agent edit");
    let results: Vec<_> = commands.iter().map(|c| doc.execute(c)).collect();
    doc.end_group();
    let problems = doc.settings().problems();
    ws.changed(&DocRef::Settings);
    let mut done = outcomes(results)?;
    done["problems"] = json!(problems);
    Ok(done)
}
