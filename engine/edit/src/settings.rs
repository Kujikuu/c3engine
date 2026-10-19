//! Project Settings editing: commands that set or reset one setting by its path
//! (`camera.max_zoom`), applied by [`SettingsDoc`] with undo. Values are JSON, checked
//! against the setting's type and the settings' rules ([`ProjectSettings::problems`]).

use crate::history::{History, HistoryEntry};
use crate::{EditError, Outcome};
use c3_core::ProjectSettings;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A change to the settings. Serialized as `{"op": "set", "path": "camera.max_zoom",
/// "value": 3}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SettingsCommand {
    /// Sets the setting at `path` (sections and fields joined by dots) to `value`; `null`
    /// clears an optional setting.
    Set { path: String, value: Value },
    /// Puts the setting at `path` (a field, a whole section, or an entry of a list keyed by
    /// name such as `input.bindings.bag`) back to its default.
    Reset { path: String },
}

/// The settings before and after a command.
struct Change {
    before: ProjectSettings,
    after: ProjectSettings,
}

/// A project's settings open for editing: every change goes through
/// [`SettingsDoc::execute`] and can be undone.
pub struct SettingsDoc {
    settings: ProjectSettings,
    /// What Reset goes back to (the game's defaults).
    defaults: ProjectSettings,
    history: History<Change>,
}

impl SettingsDoc {
    pub fn new(settings: ProjectSettings, defaults: ProjectSettings) -> Self {
        Self {
            settings,
            defaults,
            history: History::default(),
        }
    }

    pub fn settings(&self) -> &ProjectSettings {
        &self.settings
    }

    pub fn defaults(&self) -> &ProjectSettings {
        &self.defaults
    }

    /// Applies `command`, recording it for undo. Refused commands change nothing.
    pub fn execute(&mut self, command: &SettingsCommand) -> Result<Outcome, EditError> {
        let (path, value) = match command {
            SettingsCommand::Set { path, value } => (path, Some(value.clone())),
            // An entry the defaults don't have goes away.
            SettingsCommand::Reset { path } => (path, read(&self.defaults, path).ok()),
        };
        let after = match &value {
            Some(value) => with_value(&self.settings, path, value)?,
            None => without(&self.settings, path)?,
        };
        if after == self.settings {
            return Ok(Outcome {
                summary: "Nothing changed".into(),
                changed: false,
            });
        }
        let problems = after.problems();
        if let Some(problem) = problems.first() {
            return Err(EditError::InvalidSetting(problem.clone()));
        }
        let summary = match command {
            SettingsCommand::Set { .. } => {
                format!(
                    "Set {path} to {}",
                    value.as_ref().map_or_else(String::new, short)
                )
            }
            SettingsCommand::Reset { .. } => format!("Reset {path}"),
        };
        let before = std::mem::replace(&mut self.settings, after.clone());
        self.history
            .record(summary.clone(), Change { before, after });
        Ok(Outcome {
            summary,
            changed: true,
        })
    }

    /// The value of the setting at `path`, as JSON.
    pub fn get(&self, path: &str) -> Result<Value, EditError> {
        read(&self.settings, path)
    }

    /// Whether the setting at `path` has its default value.
    pub fn is_default(&self, path: &str) -> bool {
        match (read(&self.settings, path), read(&self.defaults, path)) {
            (Ok(a), Ok(b)) => same(&a, &b),
            // An entry the defaults don't have.
            (Ok(_), Err(_)) => false,
            _ => true,
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
        if let Some(first) = step.changes.first() {
            self.settings = first.before.clone();
        }
        let label = step.label.clone();
        self.history.undone(step);
        Some(label)
    }

    /// Reapplies the last undone step; returns its label.
    pub fn redo(&mut self) -> Option<String> {
        let step = self.history.take_redo()?;
        if let Some(last) = step.changes.last() {
            self.settings = last.after.clone();
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

/// `settings` as JSON with every field present (optional ones as `null`).
fn to_json(settings: &ProjectSettings) -> Value {
    let mut value = serde_json::to_value(settings).unwrap_or_default();
    // Optional fields are left out when empty; name them so paths to them resolve.
    let defaults_with_options = serde_json::json!({
        "cursors": {
            "normal": {"hotspot": null}, "attack": {"hotspot": null}, "talk": {"hotspot": null},
            "pick_up": {"hotspot": null}, "portal": {"hotspot": null}
        },
        "camera": {"tilt_deg": null},
        "gameplay": {
            "start_cell": null, "currencies": [], "equipment_slots": [],
            "new_character": {"skills": []}
        },
        "maps": [],
        "network": {"front": [], "server": [], "group": [], "local_icon": ""},
        "ui": {"skins": []},
        "characters": {"class": []}
    });
    fill_missing(&mut value, &defaults_with_options);
    value
}

fn fill_missing(value: &mut Value, with: &Value) {
    if let (Value::Object(map), Value::Object(extra)) = (value, with) {
        for (key, v) in extra {
            match map.get_mut(key) {
                Some(existing) => fill_missing(existing, v),
                None => {
                    map.insert(key.clone(), v.clone());
                }
            }
        }
    }
}

fn pointer(path: &str) -> String {
    format!("/{}", path.replace('.', "/"))
}

fn read(settings: &ProjectSettings, path: &str) -> Result<Value, EditError> {
    to_json(settings)
        .pointer(&pointer(path))
        .cloned()
        .ok_or_else(|| EditError::NoSetting(path.to_owned()))
}

/// `settings` with the setting at `path` set to `value`, if it is one and the value fits.
fn with_value(
    settings: &ProjectSettings,
    path: &str,
    value: &Value,
) -> Result<ProjectSettings, EditError> {
    let mut json = to_json(settings);
    match json.pointer_mut(&pointer(path)) {
        Some(slot) => *slot = value.clone(),
        // A new entry of a list keyed by name; a misspelt field is caught below.
        None => {
            let (parent, key) = path
                .rsplit_once('.')
                .ok_or_else(|| EditError::NoSetting(path.to_owned()))?;
            let Some(Value::Object(map)) = json.pointer_mut(&pointer(parent)) else {
                return Err(EditError::NoSetting(path.to_owned()));
            };
            map.insert(key.to_owned(), value.clone());
        }
    }
    let wrong = || EditError::InvalidSetting(format!("{} doesn't fit {path}", short(value)));
    let after: ProjectSettings = serde_json::from_value(json).map_err(|_| wrong())?;
    // A value the type took but changed (a fraction into a whole number) doesn't fit.
    if !same(&read(&after, path)?, value) {
        return Err(wrong());
    }
    Ok(after)
}

/// `settings` without the entry at `path` (of a list keyed by name).
fn without(settings: &ProjectSettings, path: &str) -> Result<ProjectSettings, EditError> {
    let no = || EditError::NoSetting(path.to_owned());
    let (parent, key) = path.rsplit_once('.').ok_or_else(no)?;
    let mut json = to_json(settings);
    let Some(Value::Object(map)) = json.pointer_mut(&pointer(parent)) else {
        return Err(no());
    };
    map.remove(key);
    serde_json::from_value(json).map_err(|_| no())
}

/// Whether two JSON values are equal, numbers by their value (`2` is `2.0`).
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

/// A value in a few words, for history labels.
fn short(value: &Value) -> String {
    let text = match value {
        Value::String(s) if s.is_empty() => "nothing".to_owned(),
        Value::String(s) => format!("\"{s}\""),
        Value::Null => "none".to_owned(),
        v => v.to_string(),
    };
    match text.char_indices().nth(40) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc() -> SettingsDoc {
        SettingsDoc::new(ProjectSettings::default(), ProjectSettings::default())
    }

    fn set(path: &str, value: Value) -> SettingsCommand {
        SettingsCommand::Set {
            path: path.into(),
            value,
        }
    }

    #[test]
    fn sets_undoes_and_redoes() {
        let mut d = doc();
        let out = d.execute(&set("camera.max_zoom", json!(3))).unwrap();
        assert_eq!(out.summary, "Set camera.max_zoom to 3");
        assert_eq!(d.settings().camera.max_zoom, 3.0);
        d.execute(&set("gameplay.currency", json!("Silver")))
            .unwrap();
        assert_eq!(
            d.undo().as_deref(),
            Some("Set gameplay.currency to \"Silver\"")
        );
        assert_eq!(d.settings().gameplay.currency, "Gold");
        d.redo();
        assert_eq!(d.settings().gameplay.currency, "Silver");
        assert!(d.is_dirty());
    }

    #[test]
    fn optional_settings_and_sections() {
        let mut d = doc();
        d.execute(&set("camera.tilt_deg", json!(30.5))).unwrap();
        assert_eq!(d.settings().camera.tilt_deg, Some(30.5));
        d.execute(&set("camera.tilt_deg", Value::Null)).unwrap();
        assert_eq!(d.settings().camera.tilt_deg, None);
        d.execute(&set(
            "cursors.attack",
            json!({"image": "a.png", "hotspot": [3, 4]}),
        ))
        .unwrap();
        assert_eq!(d.settings().cursors.attack.hotspot, Some([3, 4]));
        d.execute(&SettingsCommand::Reset {
            path: "cursors".into(),
        })
        .unwrap();
        assert_eq!(d.settings().cursors, Default::default());
    }

    #[test]
    fn refuses_unknown_paths_wrong_types_and_broken_rules() {
        let mut d = doc();
        assert_eq!(
            d.execute(&set("camera.max_zom", json!(3))),
            Err(EditError::NoSetting("camera.max_zom".into()))
        );
        assert!(matches!(
            d.execute(&set("gameplay.bag_size", json!("many"))),
            Err(EditError::InvalidSetting(_))
        ));
        assert!(matches!(
            d.execute(&set("gameplay.bag_size", json!(2.5))),
            Err(EditError::InvalidSetting(_))
        ));
        assert!(matches!(
            d.execute(&set("camera.min_zoom", json!(9))),
            Err(EditError::InvalidSetting(_))
        ));
        assert_eq!(d.settings(), &ProjectSettings::default());
        assert!(!d.is_dirty());
    }

    #[test]
    fn entries_of_keyed_lists_are_added_and_reset() {
        let mut d = doc();
        d.execute(&set("input.bindings.bag", json!(["Key I", "Pad Select"])))
            .unwrap();
        assert_eq!(
            d.settings()
                .input
                .bindings(c3_core::input::Action::Bag)
                .len(),
            2
        );
        assert!(!d.is_default("input.bindings.bag"));
        assert!(matches!(
            d.execute(&set("input.bindings.bag", json!(["Mouse Wheel"]))),
            Err(EditError::InvalidSetting(_))
        ));
        assert!(matches!(
            d.execute(&set("input.bindings.fly", json!(["Key F"]))),
            Err(EditError::InvalidSetting(_) | EditError::NoSetting(_))
        ));
        d.execute(&SettingsCommand::Reset {
            path: "input.bindings.bag".into(),
        })
        .unwrap();
        assert!(d.settings().input.bindings.is_empty());
    }

    #[test]
    fn a_drag_is_one_step() {
        let mut d = doc();
        d.begin_group("Change pan speed");
        for v in [910, 920, 930] {
            d.execute(&set("camera.pan_speed", json!(v))).unwrap();
        }
        d.end_group();
        assert_eq!(d.history().len(), 1);
        d.undo();
        assert_eq!(d.settings().camera.pan_speed, 900.0);
        d.redo();
        assert_eq!(d.settings().camera.pan_speed, 930.0);
    }
}
