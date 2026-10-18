//! Conquer character assembly: which models, textures and motions make up a player or
//! monster, from look + equipment ids. Ports of `C3DRole::SetLook/SetArmor/SetArmet/
//! SetRWeapon/SetLWeapon`, `GetWeaponActionData` and `C3DRole::SetAction` (3DBaseCode).
//!
//! The body is the armor model driven by a `3dmotion.ini` motion; hair/helmet and weapons
//! attach to the body's socket parts (`v_armet`, `v_r_weapon`, `v_l_weapon`).

use crate::Content;
use c3_anim::Clip;
use c3_assets::model::Model;

/// Action ids (3DBaseCode/BaseFunc.h).
pub mod action {
    pub const STANDBY: u32 = 100;
    pub const WALK_L: u32 = 110;
    pub const WALK_R: u32 = 111;
    pub const RUN_L: u32 = 120;
    pub const RUN_R: u32 = 121;
    pub const JUMP: u32 = 130;
    pub const SIT_DOWN: u32 = 250;
    pub const ALERT: u32 = 300;
    pub const ATTACK_0: u32 = 401;
}

/// What a role looks like: model look plus equipped item type ids (0 = none).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Look {
    /// Body look (`lookface % 1000`): 1–4 for players, monster looks otherwise.
    pub look: u32,
    /// Hair as sent by the server: style `% 1000`, colour `/ 1000`.
    pub hair: u32,
    pub armor: u32,
    pub armet: u32,
    pub r_weapon: u32,
    pub l_weapon: u32,
}

/// A model with the texture it is drawn with.
#[derive(Debug, Clone)]
pub struct PartModel {
    pub model: Model,
    pub texture: Option<String>,
}

/// A part attached to a body socket.
#[derive(Debug, Clone)]
pub struct Attachment {
    /// Socket part name in the body (e.g. `v_r_weapon`).
    pub socket: &'static str,
    pub part: PartModel,
    /// Weapon item id; weapons have their own motion per action.
    pub weapon: Option<u32>,
}

/// Clips for one model (one per part), from a motion file.
pub type Motion = Vec<Clip>;

/// A character's motions for one action.
#[derive(Debug, Clone, Default)]
pub struct CharacterMotions {
    /// Body motion id (3dmotion.ini), for display.
    pub body_id: Option<u64>,
    /// One per model: body first, then attachments (`None` = no motion of its own).
    pub models: Vec<Option<Motion>>,
}

#[derive(Debug, Clone)]
pub struct Character {
    pub body: PartModel,
    pub attachments: Vec<Attachment>,
    /// Body motion id used (3dmotion.ini), for display.
    pub motion_id: Option<u64>,
}

/// Milliseconds per frame when `Action.dat` has no entry (`GetActionTimeInterval`).
const DEFAULT_FRAME_MS: u32 = 33;
/// `Action.dat` wildcard for look or weapon family.
const ANY: u32 = 999;

const LOOK_UNIT: u64 = 1_000_000;
/// Armet ids in this family are hair styles.
const HAIR_FAMILY: u64 = 119;
const BOW_FAMILY: u32 = 500;

fn is_bow(item: u32) -> bool {
    (item / 1000) % 1000 == BOW_FAMILY
}

fn is_arrow(item: u32) -> bool {
    item / 10_000 == 105
}

/// Weapon action family (`C3DRole::GetWeaponActionData`).
pub fn weapon_family(l_weapon: u32, r_weapon: u32) -> u32 {
    let main = if is_bow(l_weapon) { l_weapon } else { r_weapon };
    let aux = if is_arrow(r_weapon) {
        r_weapon
    } else {
        l_weapon
    };
    if main == 0 {
        return if aux == 0 { 0 } else { 700 };
    }
    if aux == 0 {
        return (main % 1_000_000) / 1000;
    }
    match (aux % 1_000_000) / 100_000 {
        // Shield.
        9 => (main % 1_000_000) / 10_000 + 700,
        // One-handed off-hand weapon: dual wield.
        4 => (aux % 100_000) / 10_000 + ((main % 100_000) / 10_000 + 60) * 10,
        _ if main / 1000 == BOW_FAMILY => BOW_FAMILY,
        _ => 0,
    }
}

/// Body motion candidates in lookup order (`C3DRole::SetAction`; mount family 0).
pub fn body_motion_ids(look: u32, family: u32, act: u32) -> [u64; 3] {
    let base = look as u64 * LOOK_UNIT;
    [
        base + family as u64 * 1000 + act as u64,
        base + act as u64,
        base + action::STANDBY as u64,
    ]
}

/// Weapon motion candidates (`C3DRole` weapon motion lookup in WeaponMotion.ini).
pub fn weapon_motion_ids(look: u32, weapon: u32, act: u32) -> [u64; 4] {
    let family = (weapon as u64 % 10_000_000) * 1_000_000;
    let look = (look as u64 % 1000) * 1000;
    let act = act as u64;
    [
        family + look + act,
        family + look + 999,
        family + 999_000 + act,
        family + 999_999,
    ]
}

impl Content {
    /// Builds a character in action `act`.
    pub fn assemble_character(&self, look: &Look, act: u32) -> Result<Character, String> {
        let unit = look.look as u64 * LOOK_UNIT;
        // Body: armor model for this look (quality digit dropped), else the bare body.
        let armor_key = unit + (look.armor as u64 % LOOK_UNIT) / 10 * 10;
        let mut body = self
            .part_model(&self.tables.armor, armor_key)
            .or_else(|_| self.part_model(&self.tables.armor, unit))?;
        body.model.mark_sockets(
            &self
                .tables
                .sockets
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        );

        let mut attachments = Vec::new();
        // Helmet, or hair when no helmet is worn.
        let style = look.hair % 1000;
        let armet_key = if look.armet != 0 {
            let id = look.armet as u64 % LOOK_UNIT;
            Some(
                unit + if id / 1000 == HAIR_FAMILY {
                    id
                } else {
                    id / 10 * 10
                },
            )
        } else if !style.is_multiple_of(100) {
            Some(unit + HAIR_FAMILY * 1000 + style as u64)
        } else {
            None
        };
        if let Some(part) = armet_key.and_then(|k| self.part_model(&self.tables.armet, k).ok()) {
            attachments.push(Attachment {
                socket: "v_armet",
                part,
                weapon: None,
            });
        }
        // Weapons: bows are held in the left hand socket; arrows are not drawn at rest.
        for (item, default_socket) in [(look.r_weapon, "v_r_weapon"), (look.l_weapon, "v_l_weapon")]
        {
            if item == 0 || is_arrow(item) {
                continue;
            }
            let Ok(part) = self.part_model(&self.tables.weapon, item as u64 % LOOK_UNIT) else {
                continue;
            };
            let socket = if is_bow(item) {
                "v_l_weapon"
            } else {
                default_socket
            };
            attachments.push(Attachment {
                socket,
                part,
                weapon: Some(item),
            });
        }
        let mut character = Character {
            body,
            attachments,
            motion_id: None,
        };
        let motions = self.character_motions(&character, look, act);
        character.apply_motions(&motions);
        Ok(character)
    }

    /// `character`'s motions for action `act` (`C3DRole::SetAction`).
    pub fn character_motions(
        &self,
        character: &Character,
        look: &Look,
        act: u32,
    ) -> CharacterMotions {
        let family = weapon_family(look.l_weapon, look.r_weapon);
        let body_id = body_motion_ids(look.look, family, act)
            .into_iter()
            .find(|id| self.tables.motions.get(*id).is_some());
        let body = body_id
            .and_then(|id| self.tables.motions.get(id))
            .and_then(|p| self.load_motion(p));
        let attachments = character.attachments.iter().map(|a| {
            weapon_motion_ids(look.look, a.weapon?, act)
                .into_iter()
                .find_map(|id| self.tables.weapon_motions.get(id))
                .and_then(|p| self.load_motion(p))
        });
        CharacterMotions {
            body_id,
            models: std::iter::once(body).chain(attachments).collect(),
        }
    }

    /// Milliseconds per animation frame of action `act` (`C3DRoleData::GetActionTimeInterval`:
    /// exact key, then any look, then any weapon, then both; else 33).
    pub fn frame_interval(&self, look: &Look, act: u32) -> u32 {
        let family = weapon_family(look.l_weapon, look.r_weapon);
        let key = |look: u32, family: u32| look * 1_000_000 + family * 1000 + act;
        [
            key(look.look, family),
            key(ANY, family),
            key(look.look, ANY),
            key(ANY, ANY),
        ]
        .into_iter()
        .find_map(|k| self.tables.frame_intervals.get(&k).copied())
        .unwrap_or(DEFAULT_FRAME_MS)
    }

    /// Model + texture of a part-table entry (first part only, as the client does).
    fn part_model(
        &self,
        table: &c3_formats_tq::ini::PartTable,
        key: u64,
    ) -> Result<PartModel, String> {
        let part = table
            .get(key)
            .and_then(|p| p.first())
            .ok_or_else(|| format!("part {key} not found"))?;
        let path = self
            .tables
            .objects
            .get(part.mesh)
            .ok_or_else(|| format!("mesh {} not in 3dobj.ini", part.mesh))?;
        Ok(PartModel {
            model: self.load_model_path(path)?,
            texture: self.tables.textures.get(part.texture).map(str::to_owned),
        })
    }
}

impl Character {
    /// Switches to `motions` (from [`Content::character_motions`]).
    pub fn apply_motions(&mut self, motions: &CharacterMotions) {
        let models = std::iter::once(&mut self.body.model)
            .chain(self.attachments.iter_mut().map(|a| &mut a.part.model));
        for (model, motion) in models.zip(&motions.models) {
            if let Some(clips) = motion {
                model.apply_motion(clips);
            }
        }
        self.motion_id = motions.body_id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weapon_families_match_client_examples() {
        assert_eq!(weapon_family(0, 0), 0);
        assert_eq!(weapon_family(0, 410003), 410);
        assert_eq!(weapon_family(500003, 0), 500);
        // Blade + shield.
        assert_eq!(weapon_family(900003, 410003), 741);
    }

    #[test]
    fn motion_ids_follow_client_formulas() {
        assert_eq!(
            body_motion_ids(3, 0, 100),
            [3_000_100, 3_000_100, 3_000_100]
        );
        assert_eq!(body_motion_ids(3, 410, 100)[0], 3_410_100);
        assert_eq!(weapon_motion_ids(3, 500003, 100)[3], 500_003_999_999);
    }
}
