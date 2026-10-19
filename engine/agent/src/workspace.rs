//! What an agent works on: a project, the game that plays it, and its documents open for
//! editing. Documents open on first use and stay open, with their undo history, until saved.

use c3_edit::{AssetDoc, DialogueDoc, SceneDoc, ScriptDoc, SettingsDoc, TableDoc, ValueDoc};
use c3_play::GameModule;
use c3_project::Project;
use c3_script::blueprint::Blueprint;

/// A project open for an agent: the Studio, or a headless one.
pub trait Workspace {
    fn project(&self) -> &Project;
    fn game(&self) -> &dyn GameModule;
    /// Map `map` (a project scene's name or a content map path).
    fn scene(&mut self, map: &str) -> Result<&mut SceneDoc, String>;
    fn table(&mut self, name: &str) -> Result<&mut TableDoc, String>;
    fn settings(&mut self) -> Result<&mut SettingsDoc, String>;
    fn dialogue(&mut self, name: &str) -> Result<&mut DialogueDoc, String>;
    fn script(&mut self, name: &str) -> Result<&mut ScriptDoc, String>;
    fn blueprint(&mut self, name: &str) -> Result<&mut ValueDoc<Blueprint>, String>;
    fn assets(&mut self) -> Result<&mut AssetDoc, String>;
    /// Writes every document with changes (scripts made from dialogues and blueprints
    /// too); what was saved, as [`DocRef`] names.
    fn save(&mut self) -> Result<Vec<String>, String>;
    /// The open documents with unsaved changes, as [`DocRef`] names.
    fn unsaved(&self) -> Vec<String>;
    /// A document changed through a tool (the Studio shows it anew).
    fn changed(&mut self, doc: &DocRef) {
        let _ = doc;
    }
    /// Files were made behind the documents' backs (a new table, map or script): lists are
    /// read anew.
    fn refresh(&mut self) {}
}

/// A document, named as tools name it: `map:<key>`, `table:<name>`, `settings`,
/// `dialogue:<name>`, `script:<name>`, `blueprint:<name>` or `assets`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocRef {
    Map(String),
    Table(String),
    Settings,
    Dialogue(String),
    Script(String),
    Blueprint(String),
    Assets,
}

impl DocRef {
    pub fn parse(text: &str) -> Result<Self, String> {
        let (kind, name) = text.split_once(':').unwrap_or((text, ""));
        let name = name.to_owned();
        let named = |doc: fn(String) -> Self| {
            if name.is_empty() {
                Err(format!("\"{kind}\" needs a name: {kind}:<name>"))
            } else {
                Ok(doc(name.clone()))
            }
        };
        match kind {
            "map" => named(Self::Map),
            "table" => named(Self::Table),
            "dialogue" => named(Self::Dialogue),
            "script" => named(Self::Script),
            "blueprint" => named(Self::Blueprint),
            "settings" => Ok(Self::Settings),
            "assets" => Ok(Self::Assets),
            _ => Err(format!(
                "unknown document \"{text}\": map:<key>, table:<name>, settings, \
                 dialogue:<name>, script:<name>, blueprint:<name> or assets"
            )),
        }
    }

    pub fn name(&self) -> String {
        match self {
            Self::Map(n) => format!("map:{n}"),
            Self::Table(n) => format!("table:{n}"),
            Self::Dialogue(n) => format!("dialogue:{n}"),
            Self::Script(n) => format!("script:{n}"),
            Self::Blueprint(n) => format!("blueprint:{n}"),
            Self::Settings => "settings".into(),
            Self::Assets => "assets".into(),
        }
    }
}

/// Undoes (or with `redo`, redoes) the last change of `doc`; what it was.
pub fn step(ws: &mut dyn Workspace, doc: &DocRef, redo: bool) -> Result<Option<String>, String> {
    macro_rules! step {
        ($doc:expr) => {{
            let d = $doc;
            if redo { d.redo() } else { d.undo() }
        }};
    }
    let label = match doc {
        DocRef::Map(n) => step!(ws.scene(n)?),
        DocRef::Table(n) => step!(ws.table(n)?),
        DocRef::Settings => step!(ws.settings()?),
        DocRef::Dialogue(n) => step!(ws.dialogue(n)?),
        DocRef::Script(n) => step!(ws.script(n)?),
        DocRef::Blueprint(n) => step!(ws.blueprint(n)?),
        DocRef::Assets => {
            let assets = ws.assets()?;
            let done = if redo { assets.redo() } else { assets.undo() };
            done.map_err(|e| e.to_string())?
        }
    };
    ws.changed(doc);
    Ok(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_parse_and_name_back() {
        for name in [
            "map:Meadow",
            "table:items",
            "settings",
            "blueprint:quests",
            "assets",
        ] {
            assert_eq!(DocRef::parse(name).unwrap().name(), name);
        }
        assert!(DocRef::parse("table").is_err());
        assert!(DocRef::parse("thing:x").is_err());
    }
}
