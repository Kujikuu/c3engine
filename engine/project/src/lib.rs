//! c3-project: game projects on disk.
//!
//! A project is a folder holding one game: its manifest (`c3project.toml`, readable TOML)
//! and the folders its content lives in. Projects may also read content from external
//! sources, such as an original TQ client folder, until that content is imported.
//!
//! ```text
//! MyGame/
//!   c3project.toml   name, engine version, content sources, settings
//!   assets/          models, effects, sprites, textures, sounds
//!   maps/            scenes
//!   tables/          items, skills, monsters, …
//!   scripts/         game logic
//! ```

mod copy;
mod recent;

pub use copy::{CopyProgress, copy_entries};
pub use recent::{RecentProject, RecentProjects};

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

/// Manifest file name inside a project folder.
pub const MANIFEST: &str = "c3project.toml";

/// Folders every project has, created with it.
pub const FOLDERS: [&str; 4] = ["assets", "maps", "tables", "scripts"];

/// File name ending of a map (the engine's native scene format).
pub const MAP_SUFFIX: &str = c3_scene::native::SCENE_SUFFIX;

/// Engine version written into new manifests.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
pub enum Error {
    Io(PathBuf, std::io::Error),
    Manifest(PathBuf, String),
    /// Creating a project where one, or other files, already exist.
    NotEmpty(PathBuf),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path, e) => write!(f, "{}: {e}", path.display()),
            Self::Manifest(path, e) => write!(f, "{}: {e}", path.display()),
            Self::NotEmpty(path) => write!(
                f,
                "{} is not empty; choose an empty or new folder",
                path.display()
            ),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// External content a project reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Source {
    /// An original TQ client's content (maps, models, effects, tables), used as-is through
    /// the TQ format plugins. `path` is relative to the project (`assets` when the content
    /// was copied into the project) or absolute (a client folder used in place).
    /// `game` names the client, e.g. `conquer-5065`.
    TqClient { game: String, path: PathBuf },
}

/// `c3project.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    /// Engine version the project was last saved with.
    pub engine: String,
    #[serde(default, rename = "source", skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<Source>,
}

impl Manifest {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            engine: ENGINE_VERSION.to_owned(),
            sources: Vec::new(),
        }
    }
}

/// An open project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// The project folder.
    pub root: PathBuf,
    pub manifest: Manifest,
}

impl Project {
    /// Creates a project in folder `root` (made if missing; must be empty otherwise).
    pub fn create(root: impl Into<PathBuf>, manifest: Manifest) -> Result<Self> {
        let root = root.into();
        let io = |e| Error::Io(root.clone(), e);
        if root.exists() && std::fs::read_dir(&root).map_err(io)?.next().is_some() {
            return Err(Error::NotEmpty(root));
        }
        for folder in FOLDERS {
            std::fs::create_dir_all(root.join(folder)).map_err(io)?;
        }
        let project = Self { root, manifest };
        project.save()?;
        Ok(project)
    }

    /// Opens the project at `path`: its folder or its manifest file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let root = if path.is_file() {
            path.parent().unwrap_or(Path::new(".")).to_path_buf()
        } else {
            path.to_path_buf()
        };
        let file = root.join(MANIFEST);
        let text = std::fs::read_to_string(&file).map_err(|e| Error::Io(file.clone(), e))?;
        let manifest = toml::from_str(&text).map_err(|e| Error::Manifest(file, e.to_string()))?;
        Ok(Self { root, manifest })
    }

    /// Whether `path` is a project folder (has a manifest).
    pub fn is_project(path: impl AsRef<Path>) -> bool {
        path.as_ref().join(MANIFEST).is_file()
    }

    /// Writes the manifest.
    pub fn save(&self) -> Result<()> {
        let file = self.root.join(MANIFEST);
        let text = toml::to_string_pretty(&self.manifest)
            .map_err(|e| Error::Manifest(file.clone(), e.to_string()))?;
        std::fs::write(&file, text).map_err(|e| Error::Io(file, e))
    }

    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    /// Folder holding the project's maps.
    pub fn maps_dir(&self) -> PathBuf {
        self.root.join("maps")
    }

    /// File of map `name` (`maps/<name>.scene.ron`).
    pub fn map_file(&self, name: &str) -> PathBuf {
        self.maps_dir().join(format!("{name}{MAP_SUFFIX}"))
    }

    /// Names of the project's maps, sorted case-insensitively.
    pub fn map_names(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.maps_dir())
            .map(|dir| {
                dir.flatten()
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter_map(|n| n.strip_suffix(MAP_SUFFIX).map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        names.sort_by_key(|n| n.to_lowercase());
        names
    }

    /// The first TQ client the project reads, as `(game, folder)`.
    pub fn tq_client(&self) -> Option<(&str, PathBuf)> {
        self.manifest
            .sources
            .iter()
            .map(|s| match s {
                Source::TqClient { game, path } => (game.as_str(), self.root.join(path)),
            })
            .next()
    }

    /// Whether the TQ client content lives inside the project folder.
    pub fn content_in_project(&self) -> bool {
        self.tq_client()
            .is_some_and(|(_, dir)| dir.starts_with(&self.root))
    }

    /// Folder holding the project's assets (and copied client content).
    pub fn assets_dir(&self) -> PathBuf {
        self.root.join("assets")
    }
}

/// Where new projects go by default: `Documents/C3 Projects`.
pub fn default_projects_dir() -> PathBuf {
    dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("C3 Projects")
}

/// A folder name for project `name` (keeps letters, digits, spaces, `-` and `_`).
pub fn folder_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "New Game".to_owned()
    } else {
        cleaned.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("c3-project-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn creates_opens_and_round_trips_a_project() {
        let dir = temp_dir("create");
        let mut manifest = Manifest::new("Conquer");
        manifest.sources.push(Source::TqClient {
            game: "conquer-5065".into(),
            path: PathBuf::from(r"D:\client5065"),
        });
        let created = Project::create(&dir, manifest).unwrap();
        for folder in FOLDERS {
            assert!(dir.join(folder).is_dir(), "{folder}");
        }
        let text = std::fs::read_to_string(dir.join(MANIFEST)).unwrap();
        assert!(
            text.contains("name = \"Conquer\"") && text.contains("kind = \"tq-client\""),
            "{text}"
        );

        let opened = Project::open(dir.join(MANIFEST)).unwrap();
        assert_eq!(opened, created);
        assert_eq!(
            opened.tq_client(),
            Some(("conquer-5065", PathBuf::from(r"D:\client5065")))
        );
        assert!(!opened.content_in_project());
        let mut copied = opened.clone();
        copied.manifest.sources = vec![Source::TqClient {
            game: "conquer-5065".into(),
            path: PathBuf::from("assets"),
        }];
        assert_eq!(copied.tq_client().unwrap().1, dir.join("assets"));
        assert!(copied.content_in_project());
        assert!(Project::is_project(&dir));
        std::fs::write(opened.map_file("Twin City"), "").unwrap();
        std::fs::write(opened.maps_dir().join("notes.txt"), "").unwrap();
        assert_eq!(opened.map_names(), ["Twin City"]);
        assert!(matches!(
            Project::create(&dir, Manifest::new("x")),
            Err(Error::NotEmpty(_))
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn folder_names_are_safe() {
        assert_eq!(folder_name("My: Game?"), "My Game");
        assert_eq!(folder_name("  "), "New Game");
    }
}
