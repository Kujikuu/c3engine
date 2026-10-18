//! c3-edit: the one way projects change.
//!
//! Every edit is a typed, serializable **command** applied to a document; applying it records
//! the exact change so it can be undone. The Studio's tools, the AI assistant and external
//! agents (MCP) all issue the same commands, and one **history** records them with readable
//! labels. **Queries** read documents without changing them. Commands and queries describe
//! themselves with JSON schemas, so an AI model can be handed them as tools.
//!
//! ```text
//! Studio tool ─┐
//! Assistant ───┼─► SceneCommand ─► SceneDoc::execute ─► Scene + History (undo/redo)
//! MCP agent ───┘        SceneQuery ─► SceneDoc::query ─► QueryResult
//! ```

mod error;
mod history;
mod query;
mod scene;

pub use error::EditError;
pub use history::HistoryEntry;
pub use query::{
    CellInfo, MAX_OBJECTS, MAX_WALKABILITY_CELLS, ObjectInfo, PortalInfo, QueryResult, SceneQuery,
    SceneSummary, WalkabilityMap, entity_kind_name,
};
pub use scene::{Area, MAX_AREA_CELLS, SceneCommand, SceneDoc, entity_label};

use serde::Serialize;

/// What a successful command did, in words (shown to people and returned to AI tools).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Outcome {
    pub summary: String,
    /// False when the command found nothing to change (nothing was recorded).
    pub changed: bool,
}

/// Parses one command or a list of commands from JSON.
pub fn parse_scene_commands(json: &str) -> Result<Vec<SceneCommand>, String> {
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        Many(Vec<SceneCommand>),
        One(SceneCommand),
    }
    match serde_json::from_str(json) {
        Ok(OneOrMany::Many(commands)) => Ok(commands),
        Ok(OneOrMany::One(command)) => Ok(vec![command]),
        // Report the error as a list of commands would, the documented form.
        Err(_) => serde_json::from_str::<Vec<SceneCommand>>(json).map_err(|e| e.to_string()),
    }
}

/// JSON schema of [`SceneCommand`].
pub fn scene_command_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(SceneCommand)).unwrap_or_default()
}

/// JSON schema of [`SceneQuery`].
pub fn scene_query_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(SceneQuery)).unwrap_or_default()
}
