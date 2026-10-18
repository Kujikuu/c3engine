//! Conquer character assembly: which models, textures and motions make up a player or
//! monster, from look + equipment ids. Ports of `C3DRole::SetLook/SetArmor/SetArmet/
//! SetRWeapon/SetLWeapon`, `GetWeaponActionData` and `C3DRole::SetAction` (3DBaseCode).
//!
//! The body is the armor model driven by a `3dmotion.ini` motion; hair/helmet and weapons
//! attach to the body's socket parts (`v_armet`, `v_r_weapon`, `v_l_weapon`).

use crate::Content;
use crate::rules::slot;
use c3_game::Player;
use c3_play::{Attachment, Character, CharacterMotions, Look, LookKind, PartModel};
use std::collections::HashMap;

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

    /// Common actions with readable names (3DBaseCode/BaseFunc.h), for pickers.
    pub const NAMED: &[(u32, &str)] = &[
        (100, "Stand"),
        (101, "Rest"),
        (110, "Walk"),
        (120, "Run"),
        (130, "Jump"),
        (150, "Sad"),
        (160, "Fury"),
        (170, "Genuflect"),
        (180, "Excitement"),
        (190, "Say hello"),
        (200, "Salute"),
        (210, "Laugh"),
        (220, "Kneel"),
        (230, "Cool"),
        (250, "Sit"),
        (260, "Meditate"),
        (270, "Faint"),
        (271, "Lie"),
        (280, "Pick up"),
        (290, "Mine"),
        (300, "Alert"),
        (310, "Dodge"),
        (320, "Hurt"),
        (330, "Die"),
        (390, "Cast"),
        (400, "Revive"),
        (401, "Attack 1"),
        (402, "Attack 2"),
        (403, "Attack 3"),
        (404, "Attack 4"),
        (451, "Puncture"),
        (452, "Air strike"),
        (453, "Tornado"),
    ];

    /// The readable name of action `id`, if it has one.
    pub fn name(id: u32) -> Option<&'static str> {
        NAMED.iter().find(|(a, _)| *a == id).map(|(_, n)| *n)
    }
}

/// What a Conquer role looks like: model look plus equipped item type ids (0 = none). The
/// engine's [`Look`] carries the same as a body, hair and the items worn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RoleLook {
    /// Body look (`lookface % 1000`): 1–4 for players, monster looks otherwise.
    pub look: u32,
    /// Hair as sent by the server: style `% 1000`, colour `/ 1000`.
    pub hair: u32,
    pub armor: u32,
    pub armet: u32,
    pub r_weapon: u32,
    pub l_weapon: u32,
}

/// The hero when none is chosen: a newbie with hair, armor and a blade.
pub const DEFAULT_HERO: RoleLook = RoleLook {
    look: 3,
    hair: 310,
    armor: 130003,
    armet: 0,
    r_weapon: 410003,
    l_weapon: 0,
};

impl RoleLook {
    /// The engine's look of this role.
    pub fn to_look(&self, kind: LookKind) -> Look {
        Look {
            kind,
            body: self.look,
            hair: self.hair,
            worn: [self.armor, self.armet, self.r_weapon, self.l_weapon]
                .into_iter()
                .filter(|&k| k != 0)
                .collect(),
            ..Look::default()
        }
    }

    /// The role of the engine's `look`: its worn items go to the slots Conquer wears them in.
    pub fn from_look(look: &Look) -> Self {
        let worn = |s: u8| {
            look.worn
                .iter()
                .copied()
                .find(|&k| slot(k) == Some(s))
                .unwrap_or(0)
        };
        Self {
            look: look.body,
            hair: look.hair,
            armor: worn(slot::ARMOR),
            armet: worn(slot::HELMET),
            r_weapon: worn(slot::RIGHT_HAND),
            l_weapon: worn(slot::LEFT_HAND),
        }
    }
}

/// What a player looks like in what it wears (the default hero's body and hair).
pub fn player_look(p: &Player) -> RoleLook {
    let worn = |s: u8| {
        p.equipment
            .iter()
            .map(|e| e.kind)
            .find(|&k| slot(k) == Some(s))
            .unwrap_or(0)
    };
    let chosen = |v: u32, default: u32| if v == 0 { default } else { v };
    RoleLook {
        look: chosen(p.look.body, DEFAULT_HERO.look),
        hair: chosen(p.look.hair, DEFAULT_HERO.hair),
        armor: worn(slot::ARMOR),
        armet: worn(slot::HELMET),
        r_weapon: worn(slot::RIGHT_HAND),
        l_weapon: worn(slot::LEFT_HAND),
    }
}

/// `Action3DEffect.ini` wildcards for look, action, part and variant.
const ANY_LOOK: u64 = 999;
const ANY_ACTION: u64 = 999;
const ANY_PART: u64 = 999;
const ANY_VARIANT: u64 = 999;

/// Parses an `Action3DEffect*.ini`: lines `look.action.part.variant=effect`.
pub(crate) fn parse_action_effects(data: &[u8]) -> HashMap<u64, String> {
    let text = String::from_utf8_lossy(data);
    text.lines()
        .filter_map(|line| {
            let (key, effect) = line.trim().split_once('=')?;
            let mut n = key.split('.').map(|v| v.trim().parse::<u64>().ok());
            let (look, act, part, variant) = (n.next()??, n.next()??, n.next()??, n.next()??);
            let id = look * 1_000_000_000 + act * 1_000_000 + part * 1000 + variant;
            Some((id, effect.trim().to_owned()))
        })
        .collect()
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
    pub fn assemble_character(&self, look: &RoleLook, act: u32) -> Result<Character, String> {
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
                socket: "v_armet".into(),
                part,
                item: None,
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
                socket: socket.into(),
                part,
                item: Some(item),
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
        look: &RoleLook,
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
            weapon_motion_ids(look.look, a.item?, act)
                .into_iter()
                .find_map(|id| self.tables.weapon_motions.get(id))
                .and_then(|p| self.load_motion(p))
        });
        CharacterMotions {
            body_id,
            models: std::iter::once(body).chain(attachments).collect(),
        }
    }

    /// Effects a part plays when its role starts action `act` (`C3DRolePart::Set3DEffect`):
    /// up to one per effect slot, stopping at the first empty slot. `"none"` means stop the
    /// part's effects. Lookup order per slot as `C3DRoleData::GetRolePart3DEffect`.
    pub fn part_action_effects(&self, type_id: u64, act: u32) -> Vec<String> {
        let look = (type_id / 1_000_000) % 1000;
        let part = (type_id / 1000) % 1000;
        let variant = type_id % 1000;
        let act = u64::from(act);
        let keys = [
            (look, act, part, variant),
            (ANY_LOOK, act, part, variant),
            (ANY_LOOK, ANY_ACTION, part, variant),
            (ANY_LOOK, act, ANY_PART, variant),
            (ANY_LOOK, act, part, ANY_VARIANT),
            (look, ANY_ACTION, part, variant),
            (look, act, ANY_PART, variant),
            (look, act, part, ANY_VARIANT),
        ]
        .map(|(l, a, p, v)| l * 1_000_000_000 + a * 1_000_000 + p * 1000 + v);
        self.tables
            .action_effects
            .iter()
            .map_while(|slot| keys.iter().find_map(|k| slot.get(k)).cloned())
            .collect()
    }

    /// Milliseconds per animation frame of action `act` (`C3DRoleData::GetActionTimeInterval`:
    /// exact key, then any look, then any weapon, then both; else 33).
    pub fn frame_interval(&self, look: &RoleLook, act: u32) -> u32 {
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
            effect_key: Some(key),
            tint: [1.0; 4],
        })
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
