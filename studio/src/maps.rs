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
    pub fn name(&self) -> std::borrow::Cow<'_, str> {
        match self {
            Self::Project(name) => name.into(),
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

/// `map/map/newplain.DMap` → `newplain`; a copy `map/map/mine.DMap@2002` → `mine@2002`.
pub(crate) fn client_map_name(path: &str) -> std::borrow::Cow<'_, str> {
    c3_play::map_list::short_name(path)
}

/// The content's map files, each followed by the copies of it the map list names
/// (`file@id`), then the engine-format scenes.
pub(crate) fn list_maps(
    project: &Project,
    client: Option<&Client>,
    listed: &[c3_core::settings::MapEntry],
) -> Vec<MapRef> {
    let files = client.map(Client::list_maps).unwrap_or_default();
    let copies_of = |file: &String| -> Vec<MapRef> {
        (listed.iter())
            .filter(|m| {
                let (of, copy) = c3_play::map_list::split_instance(&m.key);
                copy.is_some() && of.eq_ignore_ascii_case(file)
            })
            .map(|m| MapRef::Client(m.key.clone()))
            .collect()
    };
    let files =
        (files.iter()).flat_map(|f| std::iter::once(MapRef::Client(f.clone())).chain(copies_of(f)));
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
