//! Asset tools: the project's own files (images, sounds, models, effects) and changes to them.

use crate::tools::{Tool, arg, opt_arg, outcomes, tool};
use crate::workspace::{DocRef, Workspace};
use c3_edit::AssetCommand;
use serde_json::{Value, json};

/// Assets `list_assets` gives at most.
const MAX_ASSETS: usize = 500;

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        tool(
            "list_assets",
            "Lists the project's own assets (under assets/) with how each is imported: those \
             in `folder`, or whose path contains `search`; at most 500. Content of a game \
             client used in place is not listed (its files are read by path).",
            json!({
                "folder": { "type": "string" },
                "search": { "type": "string" },
            }),
            &[],
            list_assets,
        ),
        tool(
            "edit_assets",
            "Changes the project's assets with AssetCommands (see command_schemas kind \
             assets): set_import, create_folder, rename (references follow), delete (to the \
             trash), import (copy a file from disk), copy_map, rename_map, delete_map. Each \
             happens at once on disk and can be undone.",
            json!({ "commands": { "type": "array", "items": { "type": "object" } } }),
            &["commands"],
            edit_assets,
        ),
    ]
}

fn list_assets(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let folder =
        opt_arg::<String>(args, "folder")?.map(|f| format!("{}/", f.trim_end_matches('/')));
    let search = opt_arg::<String>(args, "search")?.map(|s| s.to_lowercase());
    let db = ws.assets()?.db();
    let mut found: Vec<Value> = (db.iter())
        .filter(|(path, _)| folder.as_ref().is_none_or(|f| path.starts_with(f.as_str())))
        .filter(|(path, _)| {
            search
                .as_ref()
                .is_none_or(|s| path.to_lowercase().contains(s))
        })
        .map(|(path, meta)| json!({ "path": path, "import": meta.import }))
        .collect();
    let total = found.len();
    found.truncate(MAX_ASSETS);
    Ok(json!({ "assets": found, "matching": total }))
}

fn edit_assets(ws: &mut dyn Workspace, args: &Value) -> Result<Value, String> {
    let commands: Vec<AssetCommand> = arg(args, "commands")?;
    let doc = ws.assets()?;
    doc.begin_group("Agent edit");
    let results: Vec<_> = commands.iter().map(|c| doc.execute(c)).collect();
    doc.end_group();
    ws.changed(&DocRef::Assets);
    ws.refresh();
    outcomes(results)
}
