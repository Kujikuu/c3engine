//! Which maps the Studio can open: the project's own (native scenes) and, until they are
//! imported, the maps of the project's TQ client source.

use crate::tq_plugin::Client;
use c3_project::Project;

/// Where a map lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MapRef {
    /// A native scene in the project's `maps/` folder.
    Project(String),
    /// A map in the project's TQ client that isn't in the project yet (asset path,
    /// e.g. `map/map/newplain.DMap`).
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

    pub fn is_project(&self) -> bool {
        matches!(self, Self::Project(_))
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

/// The project's maps, then the client's maps not imported yet (same name = imported).
pub(crate) fn list_maps(project: &Project, client: Option<&Client>) -> Vec<MapRef> {
    let names = project.map_names();
    let imported = |path: &String| {
        let name = client_map_name(path);
        names.iter().any(|n| n.eq_ignore_ascii_case(name))
    };
    let client_maps = client.map(Client::list_maps).unwrap_or_default();
    let from_client: Vec<MapRef> = client_maps
        .into_iter()
        .filter(|p| !imported(p))
        .map(MapRef::Client)
        .collect();
    names
        .into_iter()
        .map(MapRef::Project)
        .chain(from_client)
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
