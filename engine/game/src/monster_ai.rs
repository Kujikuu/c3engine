//! What monster types do beyond chasing and hitting, as their data says: call allies of
//! their kind when hit (`assist`), run when hurt (`flee_percent`), cast a skill now and then
//! (`skill`, `skill_percent`), and bosses that never give up a chase and shrug off stuns.

use crate::EntityId;
use crate::distance;
use crate::event::Event;
use crate::rules::{SkillKind, SkillType};
use crate::sim::Sim;
use c3_scene::CellPos;

impl Sim {
    /// Monster `id` was hurt by player `by`: it turns on them, and so do allies of its kind
    /// around that are idle, when its type calls them.
    pub(crate) fn rouse(&mut self, id: EntityId, by: EntityId) {
        let Some(m) = self.monsters.get_mut(&id) else {
            return;
        };
        m.target = Some(by);
        let (kind, cell) = (m.kind, m.cell);
        let Some(t) = self.rules.monster(kind).filter(|t| t.assist) else {
            return;
        };
        let view = t.view_range as i32;
        let allies: Vec<EntityId> = (self.index.near(cell, view))
            .map(|(e, _)| e)
            .filter(|e| {
                self.monsters
                    .get(e)
                    .is_some_and(|a| a.kind == kind && a.is_alive() && a.target.is_none())
            })
            .collect();
        for ally in allies {
            self.monsters.get_mut(&ally).expect("found above").target = Some(by);
        }
    }

    /// Whether monster `id` is hurt enough that its type runs.
    pub(crate) fn wants_to_flee(&self, id: EntityId) -> bool {
        let Some(m) = self.monsters.get(&id) else {
            return false;
        };
        let percent = self.rules.monster(m.kind).map_or(0, |t| t.flee_percent);
        u64::from(m.life) * 100 < u64::from(m.stats.max_life) * u64::from(percent)
    }

    /// The cell straight away from `from`, one step past `cell`.
    pub(crate) fn away(cell: CellPos, from: CellPos) -> CellPos {
        let d = ((cell.0 - from.0).signum(), (cell.1 - from.1).signum());
        let d = if d == (0, 0) { (1, 0) } else { d };
        (cell.0 + d.0 * 4, cell.1 + d.1 * 4)
    }

    /// Whether monster `id` is a boss (it ignores the leash and stuns).
    pub(crate) fn is_boss(&self, id: EntityId) -> bool {
        (self.monsters.get(&id)).is_some_and(|m| self.rules.monster(m.kind).is_some_and(|t| t.boss))
    }

    /// The skill monster `id` casts this time instead of a plain hit, if any.
    pub(crate) fn monster_skill_now(&mut self, id: EntityId) -> Option<SkillType> {
        let kind = self.monsters.get(&id)?.kind;
        let t = self.rules.monster(kind)?;
        let (skill, percent) = (t.skill, t.skill_percent);
        if skill == 0 || !self.rng.chance(percent, 100) {
            return None;
        }
        self.rules.skill(skill).cloned()
    }

    /// Monster `id` casts `t` at player `target`: damage on it (and players around it, with
    /// an area), a heal on itself, or its status on them.
    pub(crate) fn monster_casts(&mut self, id: EntityId, t: &SkillType, target: EntityId) {
        let Some(goal) = self.players.get(&target).map(|p| p.cell) else {
            return;
        };
        let area = t.area as i32;
        let targets: Vec<EntityId> = match t.kind {
            SkillKind::Heal => vec![id],
            _ if area > 0 => (self.players.values())
                .filter(|p| p.is_alive() && distance(p.cell, goal) <= area)
                .map(|p| p.id)
                .collect(),
            _ => vec![target],
        };
        self.events.push(Event::Cast {
            caster: id,
            skill: t.id,
            cell: goal,
            targets: targets.clone(),
        });
        let caster = self.monsters[&id].fighter();
        for victim in targets {
            match t.kind {
                SkillKind::Heal => {
                    let amount = self.rules.skill_heal(&caster, t);
                    let m = self.monsters.get_mut(&id).expect("casting");
                    let healed = amount.min(m.stats.max_life.saturating_sub(m.life));
                    m.life += healed;
                    self.events.push(Event::Healed {
                        target: id,
                        amount: healed,
                    });
                }
                SkillKind::Damage => {
                    if !self.players.contains_key(&victim) {
                        continue;
                    }
                    let defender = self.player_fighter(victim);
                    let damage = self.rules.skill_hit(&caster, &defender, t, &mut self.rng);
                    self.monster_hurts(id, victim, damage);
                }
                SkillKind::Status => {}
            }
            if t.status != 0 && victim != id {
                self.add_status(victim, t.status, Some(id));
            }
        }
    }
}
