//! Recently opened projects, most recent first, kept in the user's settings folder.

use crate::{Error, Project, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Projects remembered.
const MAX_RECENT: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentProject {
    pub name: String,
    pub path: PathBuf,
    /// Seconds since the Unix epoch.
    pub opened: u64,
}

impl RecentProject {
    /// Whether the project folder still has a manifest.
    pub fn exists(&self) -> bool {
        Project::is_project(&self.path)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentProjects {
    #[serde(default, rename = "project")]
    pub entries: Vec<RecentProject>,
    /// File the list is saved to; `None` keeps it in memory only.
    #[serde(skip)]
    file: Option<PathBuf>,
}

impl RecentProjects {
    /// The user's list (`<config dir>/C3Engine/recent.toml`); empty if missing or unreadable.
    pub fn load_default() -> Self {
        match dirs::config_dir() {
            Some(dir) => Self::load(dir.join("C3Engine").join("recent.toml")),
            None => Self::default(),
        }
    }

    /// The list saved in `file`; empty if missing or unreadable.
    pub fn load(file: impl Into<PathBuf>) -> Self {
        let file = file.into();
        let mut list: Self = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default();
        list.file = Some(file);
        list
    }

    /// Moves `project` to the top of the list.
    pub fn touch(&mut self, project: &Project) {
        self.remove(&project.root);
        self.entries.insert(
            0,
            RecentProject {
                name: project.name().to_owned(),
                path: project.root.clone(),
                opened: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs()),
            },
        );
        self.entries.truncate(MAX_RECENT);
    }

    pub fn remove(&mut self, path: &Path) {
        self.entries.retain(|e| e.path != path);
    }

    /// Writes the list to its file.
    pub fn save(&self) -> Result<()> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::Io(dir.to_path_buf(), e))?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| Error::Manifest(file.clone(), e.to_string()))?;
        std::fs::write(file, text).map_err(|e| Error::Io(file.clone(), e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Manifest;

    #[test]
    fn keeps_most_recent_first_without_duplicates() {
        let dir = std::env::temp_dir().join(format!("c3-recent-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("recent.toml");
        let project = |name: &str| Project {
            root: dir.join(name),
            manifest: Manifest::new(name),
        };
        let mut recent = RecentProjects::load(&file);
        recent.touch(&project("a"));
        recent.touch(&project("b"));
        recent.touch(&project("a"));
        recent.save().unwrap();
        let names: Vec<_> = RecentProjects::load(&file)
            .entries
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["a", "b"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
