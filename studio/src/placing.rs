//! Placing assets on the map: what an asset becomes when it is dropped on the Scene view or
//! the Hierarchy (or placed from the Project panel's selection), and where.

use crate::app::Studio;
use crate::browser::{AssetKey, AssetKind};
use crate::data_panel::DraggedRow;
use crate::maps::MapRef;
use c3_edit::SceneCommand;
use c3_scene::{CellPos, Entity, EntityKind, Placement};

/// How far a placed sound carries, in world pixels, and how loud it is (percent).
const SOUND_RANGE: i32 = 300;
const SOUND_VOLUME: i32 = 100;

impl Studio {
    /// The asset `key` as a new object on `at`: a sprite (an image or sprite set, its first
    /// animation), a terrain object, a sound or an effect. `None` for other kinds.
    pub(crate) fn asset_entity(&self, key: &AssetKey, at: CellPos) -> Option<Entity> {
        let rel = key.rel.as_str();
        let kind = match AssetKind::of(rel) {
            AssetKind::Sprites | AssetKind::Texture => {
                let assets = self.game.as_ref()?.assets();
                let set = assets.sprite_set(rel)?;
                let animation = set.animations.keys().next()?.clone();
                // A picture stands on its cell by its pivot (the project's import setting,
                // else the middle of its bottom edge); sprite sets carry their own offsets.
                let offset = match AssetKind::of(rel) {
                    AssetKind::Texture => self.pivot_offset(rel)?,
                    _ => (0, 0),
                };
                EntityKind::Sprite {
                    asset: rel.to_owned(),
                    animation,
                    footprint: (1, 1),
                    offset,
                    frame_ms: 100,
                }
            }
            AssetKind::Terrain => EntityKind::Prefab {
                asset: rel.to_owned(),
            },
            AssetKind::Sound => EntityKind::Sound {
                asset: rel.to_owned(),
                range: SOUND_RANGE,
                volume: SOUND_VOLUME,
            },
            AssetKind::Effect => EntityKind::Effect {
                name: rel.to_owned(),
                transform: None,
            },
            _ => return None,
        };
        Some(Entity {
            placement: Placement::Cell(at),
            kind,
        })
    }

    /// Where a picture's top left goes from its cell so its pivot stands on it.
    fn pivot_offset(&self, path: &str) -> Option<(i32, i32)> {
        let game = self.game.as_ref()?;
        let image = game.assets().image(path)?;
        let import = game
            .native()
            .and_then(|n| n.image_import(path))
            .unwrap_or_default();
        let [_, _, w, h] = import.frame_rect(0, image.width, image.height)?;
        let [px, py] = import.pivot;
        Some((
            -(px * w as f32).round() as i32,
            -(py * h as f32).round() as i32,
        ))
    }

    /// Places table row `row` on cell `at`: a monster type makes a spawn area of it.
    pub(crate) fn place_row(&mut self, row: &DraggedRow, at: CellPos) {
        let monsters = self.game.as_ref().map(|g| g.data_tables().monsters.table);
        if monsters.as_deref() != Some(row.table.as_str()) {
            self.error = Some(format!(
                "Rows of {} can't be placed on a map: drag a monster type (a row of {})",
                row.table,
                monsters.unwrap_or_default()
            ));
            return;
        }
        let Ok(monster) = u32::try_from(row.key) else {
            return;
        };
        let mut spawn = crate::hierarchy::new_spawn(at);
        spawn.monster = monster;
        self.create(SceneCommand::AddSpawn { spawn });
    }

    /// Places asset `key` on cell `at` of the open map (one undo step) and selects it.
    pub(crate) fn place_asset(&mut self, key: &AssetKey, at: CellPos) {
        // A client map (DMap) names sprites by sprite set: a plain picture there would be lost
        // to the classic client.
        let classic = (self.doc.as_ref()).is_some_and(|d| matches!(d.map, MapRef::Client(_)));
        if classic && AssetKind::of(&key.rel) == AssetKind::Texture {
            self.error = Some(
                "A client map (DMap) shows pictures through sprite sets (.ani): place the \
                 picture on an engine map, or use a sprite set"
                    .into(),
            );
            return;
        }
        match self.asset_entity(key, at) {
            Some(entity) => self.create(SceneCommand::AddEntity {
                entity,
                index: None,
            }),
            None => {
                let name = key.rel.rsplit('/').next().unwrap_or(&key.rel);
                self.error = Some(format!(
                    "{name} can't be placed on a map: drop an image, sprite set, terrain \
                     object, sound or effect"
                ));
            }
        }
    }
}
