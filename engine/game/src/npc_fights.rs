//! NPCs that can be hit: an NPC whose extra value named by [`NpcCombatSettings::max_life`]
//! is above 0 takes hits like a monster. Its life is the world flag `npc<id>.life` (every map
//! sees it; scripts set it back), starting at its extra `life` value; at 0 it can't be hit,
//! and the custom event `npc_destroyed_<id>` runs for its destroyer with the NPC as the
//! dialogue's NPC. Training goals, guild-war poles and gates are such NPCs.

use crate::sim::Sim;
use crate::{CellPos, EntityId, Event, Fighter};
use c3_core::settings::NpcCombatSettings;

/// The world flag holding NPC `id`'s life.
pub fn npc_life_flag(id: u32) -> String {
    format!("npc{id}.life")
}

/// The custom event run for the player who destroyed an NPC: `npc_destroyed_<key>`, the key
/// being its id or the extra value the settings name.
pub fn npc_destroyed_event(key: u32) -> String {
    format!("npc_destroyed_{key}")
}

/// NPC `npc`'s extra value `key` as a number (0 when missing or not one).
fn extra_number(npc: &c3_scene::Npc, key: &str) -> i64 {
    (!key.is_empty())
        .then(|| npc.extra.get(key)?.trim().parse::<i64>().ok())
        .flatten()
        .unwrap_or(0)
}

impl Sim {
    fn npc_combat(&self) -> &NpcCombatSettings {
        &self.settings.npc_combat
    }

    /// NPC `id` when it can be hit: it has a max life.
    fn hittable_npc(&self, id: EntityId) -> Option<&c3_scene::Npc> {
        let key = &self.npc_combat().max_life;
        (self.npcs.iter()).find(|n| n.id == id.0 && extra_number(n, key) > 0)
    }

    /// NPC `npc`'s most life and life now when it can be hit (for hosts to show).
    pub fn npc_hit_points(&self, npc: &c3_scene::Npc) -> Option<(u32, u32)> {
        let max = u32::try_from(extra_number(npc, &self.npc_combat().max_life)).ok()?;
        let life = u32::try_from(self.npc_life(npc).clamp(0, i64::from(max))).unwrap_or(0);
        (max > 0).then_some((max, life))
    }

    /// NPC `npc`'s life now: its world flag, else its extra `life` value.
    pub(crate) fn npc_life(&self, npc: &c3_scene::Npc) -> i64 {
        (self.world.flags.get(&npc_life_flag(npc.id)).copied())
            .unwrap_or_else(|| extra_number(npc, &self.npc_combat().life))
    }

    /// Where NPC `id` stands when it can be hit and has life left.
    pub(crate) fn hittable_npc_cell(&self, id: EntityId) -> Option<CellPos> {
        let npc = self.hittable_npc(id)?;
        (self.npc_life(npc) > 0).then_some(npc.cell)
    }

    /// NPC `npc` as a fighter: its level, life and defence by the settings' keys; its facts are
    /// `npc` (1), `monster` (its kind, so formulas treat it as no player) and every numeric
    /// extra value.
    fn npc_fighter(&self, npc: &c3_scene::Npc) -> Fighter {
        let keys = self.npc_combat();
        let mut facts: std::collections::BTreeMap<String, i64> = (npc.extra.iter())
            .filter_map(|(k, v)| Some((k.clone(), v.trim().parse::<i64>().ok()?)))
            .collect();
        facts.insert("npc".into(), 1);
        facts.insert("monster".into(), i64::from(npc.kind.max(1)));
        let number = |key: &str| u32::try_from(extra_number(npc, key)).unwrap_or(0);
        Fighter {
            level: number(&keys.level),
            stats: crate::Stats {
                max_life: number(&keys.max_life),
                defence: number(&keys.defence),
                ..Default::default()
            },
            facts,
        }
    }

    /// Player `id` hits NPC `target` (by a blow, or a skill with `skill`); the life it took.
    pub(crate) fn player_hits_npc(
        &mut self,
        id: EntityId,
        target: EntityId,
        skill: Option<&crate::SkillType>,
    ) -> u32 {
        let Some(npc) = self.hittable_npc(target).cloned() else {
            return 0;
        };
        let life = self.npc_life(&npc);
        let attacker = self.player_fighter(id);
        let defender = self.npc_fighter(&npc);
        let roll = match skill {
            Some(t) => self.rules.skill_hit(&attacker, &defender, t, &mut self.rng),
            None => self.rules.hit(&attacker, &defender, &mut self.rng),
        };
        let damage = roll.min(u32::try_from(life.max(0)).unwrap_or(u32::MAX));
        let left = life - i64::from(damage);
        self.change_world(c3_script::WorldChange::Flag {
            name: npc_life_flag(npc.id),
            value: left,
        });
        self.events.push(Event::Attacked {
            attacker: id,
            target,
            damage,
        });
        let exp = self.rules.hit_exp(&attacker, &defender, damage);
        self.gain_exp(id, exp);
        if skill.is_none() {
            self.wear_items(id, |t| t.bonus.attack_max > 0);
        }
        if left <= 0 && damage > 0 {
            let chain = extra_number(&npc, &self.npc_combat().destroyed_event);
            let key = u32::try_from(chain)
                .ok()
                .filter(|&k| k != 0)
                .unwrap_or(npc.id);
            self.npc_event(id, &npc, &npc_destroyed_event(key));
        }
        damage
    }
}
