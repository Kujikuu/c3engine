//! What NPCs look like (`C3DTaskNpc::SetLook`): an NPC's look ÷ 10 is its type in `Npc.ini`,
//! which names a simple object (`3DSimpleObj.ini`: mesh and texture ids) and its stand-by
//! motion (`3dmotion.ini`); look mod 10 is the facing.

use crate::Content;
use crate::character::{Character, CharacterMotions, PartModel};
use c3_formats_tq::ini::Ini;
use std::collections::HashMap;

/// One `[NpcTypeN]` of `Npc.ini`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpcType {
    pub name: String,
    /// Section of `3DSimpleObj.ini` (`[ObjIDTypeN]`).
    pub simple_obj: u32,
    /// `3dmotion.ini` id played while idle.
    pub standby_motion: u64,
    /// Effect played on the NPC, if any.
    pub effect: Option<String>,
    /// Always faces the same way.
    pub fixed_direction: bool,
}

/// The mesh and texture of a `3DSimpleObj.ini` entry (5065 objects have one part).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SimplePart {
    mesh: u64,
    texture: u64,
}

/// `Npc.ini` and `3DSimpleObj.ini`, by number.
#[derive(Debug, Default)]
pub(crate) struct NpcTables {
    types: HashMap<u32, NpcType>,
    simple_objects: HashMap<u32, SimplePart>,
}

impl NpcTables {
    pub(crate) fn parse(npc_ini: &[u8], simple_obj_ini: &[u8]) -> Self {
        let number = |title: &str, prefix: &str| title.strip_prefix(prefix)?.parse::<u32>().ok();
        let npcs = Ini::parse(npc_ini);
        let types = npcs
            .sections
            .iter()
            .filter_map(|s| {
                let id = number(&s.name, "NpcType")?;
                let get = |k: &str| s.get(k).unwrap_or_default();
                let effect = get("Effect");
                Some((
                    id,
                    NpcType {
                        name: get("Name").to_owned(),
                        simple_obj: get("SimpleObjID").trim().parse().ok()?,
                        standby_motion: get("StandByMotion").trim().parse().unwrap_or(0),
                        effect: (!effect.is_empty() && effect != "none").then(|| effect.to_owned()),
                        fixed_direction: get("FixDir").trim() == "1",
                    },
                ))
            })
            .collect();
        let objects = Ini::parse(simple_obj_ini);
        let simple_objects = objects
            .sections
            .iter()
            .filter_map(|s| {
                let id = number(&s.name, "ObjIDType")?;
                let part = SimplePart {
                    mesh: s.get("Part0")?.trim().parse().ok()?,
                    texture: s.get("Texture0")?.trim().parse().ok()?,
                };
                Some((id, part))
            })
            .collect();
        Self {
            types,
            simple_objects,
        }
    }
}

impl Content {
    /// The NPC type of `look` (the NPC's look ÷ 10).
    pub fn npc_type(&self, look: u32) -> Option<&NpcType> {
        self.npc_tables.types.get(&look)
    }

    /// An NPC of type `look` as a character playing its stand-by motion.
    pub fn npc_character(&self, look: u32) -> Result<Character, String> {
        let kind = self
            .npc_type(look)
            .ok_or_else(|| format!("NPC type {look} is not in Npc.ini"))?;
        let part = self
            .npc_tables
            .simple_objects
            .get(&kind.simple_obj)
            .ok_or_else(|| {
                format!(
                    "simple object {} is not in 3DSimpleObj.ini",
                    kind.simple_obj
                )
            })?;
        let path = self
            .tables
            .objects
            .get(part.mesh)
            .ok_or_else(|| format!("mesh {} is not in 3dobj.ini", part.mesh))?;
        let mut character = Character {
            body: PartModel {
                model: self.load_model_path(path)?,
                texture: self.tables.textures.get(part.texture).map(str::to_owned),
                type_id: None,
            },
            attachments: Vec::new(),
            motion_id: None,
        };
        let motion =
            (self.tables.motions.get(kind.standby_motion)).and_then(|path| self.load_motion(path));
        if motion.is_some() {
            character.motion_id = Some(kind.standby_motion);
            character.apply_motions(&CharacterMotions {
                body_id: Some(kind.standby_motion),
                models: vec![motion],
            });
        }
        Ok(character)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_npc_types_and_simple_objects() {
        let tables = NpcTables::parse(
            b"[NpcType1]\nName=Storekeeper\nSimpleObjID=0211\nStandByMotion=999001100\nBlazeMotion=1\nRestMotion=2\nEffect=none\nASB=5\nADB=6\nFixDir=0\n",
            b"[ObjIDType211]\nPartAmount=1\nPart0=9990010\nTexture0=9990211\n",
        );
        let kind = &tables.types[&1];
        assert_eq!(
            (
                kind.name.as_str(),
                kind.simple_obj,
                kind.standby_motion,
                &kind.effect
            ),
            ("Storekeeper", 211, 999_001_100, &None)
        );
        assert_eq!(
            tables.simple_objects[&211],
            SimplePart {
                mesh: 9_990_010,
                texture: 9_990_211
            }
        );
    }
}
