//! A player's own controls: what they rebound in the game's menu, kept on their machine
//! (`<config>/C3 Games/<game>/controls.toml`) on top of the project's bindings. The Studio's
//! Play mode rebinds for the session only, so the project's bindings stay what it tests.

use c3_core::input::{Action, Binding, InputSettings};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct ControlsFile {
    bindings: BTreeMap<Action, Vec<Binding>>,
}

/// The player's rebinds and where they are kept.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerControls {
    /// `None`: kept for this session only.
    file: Option<PathBuf>,
    pub bindings: BTreeMap<Action, Vec<Binding>>,
}

impl PlayerControls {
    /// Rebinds kept for this session only.
    pub fn session_only() -> Self {
        Self::default()
    }

    /// The player's controls of game `name` on this machine (none yet: none).
    pub fn load(name: &str) -> Self {
        let file = dirs::config_dir().map(|dir| {
            dir.join("C3 Games")
                .join(c3_project::folder_name(name))
                .join("controls.toml")
        });
        let bindings = file
            .as_ref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|text| toml::from_str::<ControlsFile>(&text).ok())
            .map(|f| f.bindings)
            .unwrap_or_default();
        Self { file, bindings }
    }

    /// Keeps the controls on the player's machine (session-only controls aren't kept).
    pub fn save(&self) -> Result<(), String> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = toml::to_string_pretty(&ControlsFile {
            bindings: self.bindings.clone(),
        })
        .map_err(|e| e.to_string())?;
        std::fs::write(file, text).map_err(|e| e.to_string())
    }

    /// Binds `action` to `binding` on its device, in place of what it had there.
    pub fn rebind(&mut self, project: &InputSettings, action: Action, binding: Binding) {
        let current = self
            .bindings
            .get(&action)
            .cloned()
            .unwrap_or_else(|| project.bindings(action));
        let device = binding.device();
        let mut next: Vec<Binding> = current
            .into_iter()
            .filter(|b| b.device() != device)
            .collect();
        next.push(binding);
        self.bindings.insert(action, next);
    }

    /// Back to the game's own controls.
    pub fn reset(&mut self) {
        self.bindings.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebinding_replaces_only_that_device() {
        let project = InputSettings::default();
        let mut controls = PlayerControls::session_only();
        controls.rebind(&project, Action::Bag, "Key I".parse().unwrap());
        let bag = &controls.bindings[&Action::Bag];
        assert!(bag.contains(&"Key I".parse().unwrap()));
        assert!(!bag.contains(&"Key B".parse().unwrap()));
        assert!(bag.contains(&"Pad Select".parse().unwrap()));
        assert_eq!(controls.save(), Ok(()));
        controls.reset();
        assert!(controls.bindings.is_empty());
    }
}
