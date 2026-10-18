//! Which maps the Studio can open: the map files of the project's content, used as they are
//! (DMaps), and scenes made in the engine's own format.

use crate::tq_plugin::Client;
use c3_project::Project;

/// Where a map lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MapRef {
    /// A scene in the engine's own format, in the project's `maps/` folder.
    Project(String),
    /// A map file of the project's TQ content, edited in place (asset path, e.g.
    /// `map/map/newplain.DMap`).
    Client(String),
}

impl MapRef {
    /// Parses a command-line map: a client asset path (`map/map/x.DMap`) or a project map name.
    pub fn parse(arg: &str) -> Self {
        if arg.contains('/') || arg.to_ascii_lowercase().ends_with(".dmap") {
            Self::Client(arg.to_owned())
        } else {
            Self::Project(arg.to_owned())
        }
    }

    /// Name shown in lists and used for the project file.
    pub fn name(&self) -> &str {
        match self {
            Self::Project(name) => name,
            Self::Client(path) => client_map_name(path),
        }
    }

    /// How the game loads it: a project map's name, or the client map's path.
    pub fn key(&self) -> String {
        match self {
            Self::Project(name) | Self::Client(name) => name.clone(),
        }
    }
}

/// `map/map/newplain.DMap` → `newplain`.
pub(crate) fn client_map_name(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    match file.rfind('.') {
        Some(dot) if file[dot..].eq_ignore_ascii_case(".dmap") => &file[..dot],
        _ => file,
    }
}

/// The content's map files, then the engine-format scenes.
pub(crate) fn list_maps(project: &Project, client: Option<&Client>) -> Vec<MapRef> {
    let files = client.map(Client::list_maps).unwrap_or_default();
    let files = files.into_iter().map(MapRef::Client);
    files
        .chain(project.map_names().into_iter().map(MapRef::Project))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_parsing() {
        assert_eq!(client_map_name("map/map/newplain.DMap"), "newplain");
        assert_eq!(MapRef::parse("map/map/x.dmap").name(), "x");
        assert_eq!(
            MapRef::parse("Twin City"),
            MapRef::Project("Twin City".into())
        );
    }
}
