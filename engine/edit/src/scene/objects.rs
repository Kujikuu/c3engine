//! Placed objects (sprites, terrain prefabs, sounds, effects), addressed by their index in
//! the scene's entity list.

use super::{Change, Edit, SceneDoc};
use crate::EditError;
use c3_scene::{CellPos, Entity, EntityKind, Placement};

/// A short description of an entity for labels: `tree01`, `sound ball1`, `effect fire`.
pub fn entity_label(e: &Entity) -> String {
    match &e.kind {
        // A sheet's animation by its sheet (`hero walk`); a TQ sprite set by its animation.
        EntityKind::Sprite {
            asset, animation, ..
        } if asset.contains('.') => format!("{} {animation}", file_stem(asset)),
        EntityKind::Sprite { animation, .. } => animation.clone(),
        EntityKind::Prefab { asset } => file_stem(asset).to_owned(),
        EntityKind::Sound { asset, .. } => format!("sound {}", file_stem(asset)),
        EntityKind::Effect { name, .. } => format!("effect {name}"),
    }
}

fn file_stem(path: &str) -> &str {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

impl SceneDoc {
    fn entity(&self, index: usize) -> Result<&Entity, EditError> {
        self.scene
            .entities
            .get(index)
            .ok_or(EditError::NoEntity(index))
    }

    fn check_placement(&self, placement: Placement) -> Result<(), EditError> {
        match placement {
            Placement::Cell(cell) => self.check_cell(cell),
            Placement::World(_) => Ok(()),
        }
    }

    pub(super) fn add_entity(&mut self, entity: &Entity, index: Option<usize>) -> Edit {
        self.check_placement(entity.placement)?;
        let len = self.scene.entities.len();
        let index = index.unwrap_or(len);
        if index > len {
            return Err(EditError::NoEntity(index));
        }
        self.scene.entities.insert(index, entity.clone());
        Ok((
            format!("Added {}", entity_label(entity)),
            Some(Change::Entity {
                index,
                before: None,
                after: Some(entity.clone()),
            }),
        ))
    }

    pub(super) fn move_entity(&mut self, index: usize, to: CellPos) -> Edit {
        let before = self.entity(index)?.clone();
        self.check_cell(to)?;
        let placement = match before.placement {
            Placement::Cell(_) => Placement::Cell(to),
            Placement::World(_) => {
                let (x, y) = self.scene.cell_to_world(to);
                Placement::World((x.round() as i32, y.round() as i32))
            }
        };
        let label = entity_label(&before);
        if placement == before.placement {
            return Ok((format!("{label} is already at {}, {}", to.0, to.1), None));
        }
        let after = Entity {
            placement,
            ..before.clone()
        };
        self.scene.entities[index] = after.clone();
        Ok((
            format!("Moved {label} to {}, {}", to.0, to.1),
            Some(Change::Entity {
                index,
                before: Some(before),
                after: Some(after),
            }),
        ))
    }

    pub(super) fn set_entity(&mut self, index: usize, entity: &Entity) -> Edit {
        let before = self.entity(index)?.clone();
        self.check_placement(entity.placement)?;
        if &before == entity {
            return Ok(("Nothing changed".into(), None));
        }
        self.scene.entities[index] = entity.clone();
        Ok((
            format!("Changed {}", entity_label(entity)),
            Some(Change::Entity {
                index,
                before: Some(before),
                after: Some(entity.clone()),
            }),
        ))
    }

    pub(super) fn remove_entity(&mut self, index: usize) -> Edit {
        let before = self.entity(index)?.clone();
        self.scene.entities.remove(index);
        Ok((
            format!("Deleted {}", entity_label(&before)),
            Some(Change::Entity {
                index,
                before: Some(before),
                after: None,
            }),
        ))
    }

    /// Applies an entity change forwards (`before` → `after`) or backwards.
    pub(super) fn apply_entity(
        &mut self,
        index: usize,
        from: &Option<Entity>,
        to: &Option<Entity>,
    ) {
        let entities = &mut self.scene.entities;
        match (from, to) {
            (None, Some(e)) => entities.insert(index, e.clone()),
            (Some(_), None) => {
                entities.remove(index);
            }
            (Some(_), Some(e)) => entities[index] = e.clone(),
            (None, None) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::doc;
    use super::*;
    use crate::SceneCommand;

    fn effect(cell: CellPos, name: &str) -> Entity {
        Entity {
            placement: Placement::Cell(cell),
            kind: EntityKind::Effect {
                name: name.into(),
                transform: None,
            },
        }
    }

    #[test]
    fn entities_add_move_set_remove_with_exact_undo() {
        let mut d = doc();
        let add = |entity, index| SceneCommand::AddEntity { entity, index };
        d.execute(&add(effect((1, 1), "fire"), None)).unwrap();
        d.execute(&add(effect((2, 2), "glow"), Some(0))).unwrap();
        assert_eq!(entity_label(&d.scene().entities[0]), "effect glow");
        assert_eq!(
            d.execute(&add(effect((99, 1), "x"), None)),
            Err(EditError::OutOfBounds((99, 1)))
        );
        d.execute(&SceneCommand::MoveEntity {
            index: 1,
            to: (5, 5),
        })
        .unwrap();
        assert_eq!(d.scene().entities[1].placement, Placement::Cell((5, 5)));
        d.execute(&SceneCommand::SetEntity {
            index: 1,
            entity: effect((5, 5), "smoke"),
        })
        .unwrap();
        d.execute(&SceneCommand::RemoveEntity { index: 0 }).unwrap();
        assert_eq!(
            d.execute(&SceneCommand::RemoveEntity { index: 7 }),
            Err(EditError::NoEntity(7))
        );
        assert_eq!(d.scene().entities, [effect((5, 5), "smoke")]);
        while d.undo().is_some() {}
        assert!(d.scene().entities.is_empty());
        while d.redo().is_some() {}
        assert_eq!(d.scene().entities, [effect((5, 5), "smoke")]);
        let labels: Vec<_> = d.history().into_iter().map(|e| e.label).collect();
        assert_eq!(
            labels,
            [
                "Added effect fire",
                "Added effect glow",
                "Moved effect fire to 5, 5",
                "Changed effect smoke",
                "Deleted effect glow",
            ]
        );
    }

    #[test]
    fn entity_commands_read_from_json() {
        let commands = crate::parse_scene_commands(
            r#"[{"op":"add_entity","entity":{"placement":{"Cell":[1,2]},
                  "kind":{"Effect":{"name":"fire","transform":null}}}},
                {"op":"move_entity","index":0,"to":[3,4]}]"#,
        )
        .unwrap();
        assert_eq!(
            commands,
            [
                SceneCommand::AddEntity {
                    entity: effect((1, 2), "fire"),
                    index: None
                },
                SceneCommand::MoveEntity {
                    index: 0,
                    to: (3, 4)
                }
            ]
        );
    }
}
