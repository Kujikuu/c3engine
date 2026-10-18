//! The game's maps, as the engine knows them: each map's key (how the game loads it), its ids
//! (what portals, scripts and saves name it by), name, music and rules. The project's map list
//! (Project Settings > Maps) comes first; maps of the game it doesn't list follow, with the
//! game's own ids (Conquer's catalog) or ids after the highest listed.

use crate::module::GameModule;
use c3_core::ProjectSettings;
use c3_project::Project;

/// One map of the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapInfo {
    pub key: String,
    /// Its ids; the first is the one portals made in the editor use.
    pub ids: Vec<u32>,
    pub name: String,
    /// A sound asset; empty for none.
    pub music: String,
    pub pk: bool,
}

impl MapInfo {
    pub fn id(&self) -> u32 {
        self.ids.first().copied().unwrap_or(0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct MapList {
    maps: Vec<MapInfo>,
}

/// The file name of `key` without folders or extensions.
fn short_name(key: &str) -> &str {
    let name = key.rsplit(['/', '\\']).next().unwrap_or(key);
    name.split('.').next().unwrap_or(name)
}

impl MapList {
    /// The maps of `project` played by `game`, as `settings` list them.
    pub fn new(project: &Project, game: &dyn GameModule, settings: &ProjectSettings) -> Self {
        Self::from_parts(
            &settings.maps,
            game.maps(project)
                .into_iter()
                .map(|key| {
                    let ids = game.map_ids(&key);
                    (key, ids)
                })
                .collect(),
        )
    }

    /// The `listed` maps, then the `game`'s other maps with their ids (none: the next free).
    pub fn from_parts(
        listed: &[c3_core::settings::MapEntry],
        game: Vec<(String, Vec<u32>)>,
    ) -> Self {
        let mut maps: Vec<MapInfo> = listed
            .iter()
            .map(|m| MapInfo {
                key: m.key.clone(),
                ids: vec![m.id],
                name: if m.name.is_empty() {
                    short_name(&m.key).to_owned()
                } else {
                    m.name.clone()
                },
                music: m.music.clone(),
                pk: m.pk,
            })
            .collect();
        let mut next = maps.iter().map(MapInfo::id).max().unwrap_or(0) + 1;
        for (key, ids) in game {
            if let Some(listed) = maps.iter_mut().find(|m| m.key == key) {
                // A listed map answers to the game's ids too.
                for id in ids {
                    if !listed.ids.contains(&id) {
                        listed.ids.push(id);
                    }
                }
                continue;
            }
            let ids = if ids.is_empty() {
                next += 1;
                vec![next - 1]
            } else {
                ids
            };
            maps.push(MapInfo {
                name: short_name(&key).to_owned(),
                key,
                ids,
                music: String::new(),
                pk: false,
            });
        }
        Self { maps }
    }

    pub fn iter(&self) -> impl Iterator<Item = &MapInfo> {
        self.maps.iter()
    }

    /// The map with id `id`.
    pub fn by_id(&self, id: u32) -> Option<&MapInfo> {
        self.maps.iter().find(|m| m.ids.contains(&id))
    }

    pub fn by_key(&self, key: &str) -> Option<&MapInfo> {
        self.maps.iter().find(|m| m.key == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_core::settings::MapEntry;

    #[test]
    fn listed_maps_come_first_and_others_get_free_ids() {
        let listed = [MapEntry {
            key: "Meadow".into(),
            id: 5,
            name: "Green Meadow".into(),
            music: "sounds/theme.ogg".into(),
            pk: false,
        }];
        let list = MapList::from_parts(
            &listed,
            vec![
                ("Meadow".into(), vec![]),
                ("Grove".into(), vec![]),
                ("map/map/newplain.DMap".into(), vec![1002, 1015]),
            ],
        );
        assert_eq!(list.by_id(5).unwrap().name, "Green Meadow");
        assert_eq!(list.by_key("Grove").unwrap().id(), 6);
        assert_eq!(list.by_id(1015).unwrap().key, "map/map/newplain.DMap");
        assert_eq!(list.by_id(1015).unwrap().name, "newplain");
        assert!(list.by_id(7).is_none());
    }
}
