//! Skills: what players learn and cast — a strike or a spell at a monster, a blast on an area,
//! a heal. A cast costs mana, then the skill rests for its cooldown; out of range, the
//! caster walks up first. The game's rules say what each skill is and how hard it hits.

use crate::entity::Task;
use crate::event::Event;
use crate::rules::{SkillKind, SkillTarget, SkillType};
use crate::sim::Sim;
use crate::{EntityId, distance};
use c3_scene::CellPos;
use serde::{Deserialize, Serialize};

/// Where a cast is aimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastAt {
    /// The caster itself.
    Me,
    /// A monster or player.
    Entity(EntityId),
    /// A cell of the ground.
    Cell(CellPos),
}

impl Sim {
    /// Teaches player `id` skill `skill`; false if it knew it or there is no such skill.
    pub fn learn_skill(&mut self, id: EntityId, skill: u32) -> bool {
        if self.rules.skill(skill).is_none() {
            return false;
        }
        let Some(p) = self.players.get_mut(&id) else {
            return false;
        };
        if p.skills.contains(&skill) {
            return false;
        }
        p.skills.push(skill);
        true
    }

    /// Player `id` casts `skill` at `at`, walking into range first if it must.
    pub fn cast(&mut self, id: EntityId, skill: u32, at: CastAt) {
        let Some(t) = self.rules.skill(skill).cloned() else {
            return;
        };
        let Some(p) = self.players.get(&id).filter(|p| p.is_alive()) else {
            return;
        };
        if !p.skills.contains(&skill) {
            self.message(id, format!("You don't know {}.", t.name));
            return;
        }
        let at = match t.target {
            SkillTarget::Myself => CastAt::Me,
            _ => at,
        };
        if t.target == SkillTarget::Enemy && !matches!(at, CastAt::Entity(e) if self.is_foe(id, e))
        {
            self.message(id, format!("{} needs a target.", t.name));
            return;
        }
        self.stop_talking(id);
        let p = self.players.get_mut(&id).expect("checked above");
        p.task = Task::Cast { skill, at };
        p.path.clear();
    }

    /// Whether `caster` may aim a damaging skill at `e`: a living monster, or another player
    /// on a PK map.
    fn is_foe(&self, caster: EntityId, e: EntityId) -> bool {
        self.foe_cell(caster, e).is_some()
    }

    /// Where `at` is now, if it is still there.
    fn cast_cell(&self, caster: EntityId, at: CastAt) -> Option<CellPos> {
        match at {
            CastAt::Me => self.players.get(&caster).map(|p| p.cell),
            CastAt::Cell(cell) => Some(cell),
            CastAt::Entity(e) => self
                .monsters
                .get(&e)
                .filter(|m| m.is_alive())
                .map(|m| m.cell)
                .or_else(|| self.living_player_cell(e)),
        }
    }

    fn living_player_cell(&self, id: EntityId) -> Option<CellPos> {
        self.players
            .get(&id)
            .filter(|p| p.is_alive())
            .map(|p| p.cell)
    }

    /// One tick of a cast: closing in, then casting when the skill is ready.
    pub(crate) fn walk_to_cast(&mut self, id: EntityId, skill: u32, at: CastAt) {
        let (Some(t), Some(goal)) = (self.rules.skill(skill).cloned(), self.cast_cell(id, at))
        else {
            self.players.get_mut(&id).expect("ticking").task = Task::Idle;
            return;
        };
        let p = self.players.get_mut(&id).expect("ticking");
        if distance(p.cell, goal) > t.range.max(1) as i32 {
            if p.path
                .back()
                .is_none_or(|&end| distance(end, goal) > t.range.max(1) as i32)
            {
                self.plan_path(id, goal);
                if self.players[&id].path.is_empty() {
                    self.players.get_mut(&id).expect("ticking").task = Task::Idle;
                    self.message(id, "You can't reach it.");
                    return;
                }
            }
            self.step_player(id);
            return;
        }
        p.path.clear();
        p.task = Task::Idle;
        self.release(id, &t, goal, at);
    }

    /// Casts `t` at `goal` (where `at` is) if it is ready and the caster has the mana.
    fn release(&mut self, id: EntityId, t: &SkillType, goal: CellPos, at: CastAt) {
        let now = self.now_ms;
        let p = self.players.get_mut(&id).expect("casting");
        if p.cooldowns.get(&t.id).is_some_and(|&ready| now < ready) {
            let text = format!("{} isn't ready yet.", t.name);
            self.message(id, text);
            return;
        }
        if p.mana < t.mana {
            let text = format!("Not enough mana for {}.", t.name);
            self.message(id, text);
            return;
        }
        // An attribute the cast costs (an XP skill's full bar, stamina).
        if !t.cost_attribute.is_empty() {
            let default = (self.settings.attributes.iter())
                .find(|a| a.name == t.cost_attribute)
                .map_or(0, |a| a.default);
            let p = self.players.get_mut(&id).expect("casting");
            let have = p
                .attributes
                .get(&t.cost_attribute)
                .copied()
                .unwrap_or(default);
            if have < t.cost {
                let text = format!("Not enough {} for {}.", t.cost_attribute, t.name);
                self.message(id, text);
                return;
            }
            p.attributes.insert(t.cost_attribute.clone(), have - t.cost);
        }
        let p = self.players.get_mut(&id).expect("casting");
        p.mana -= t.mana;
        p.cooldowns.insert(t.id, now + u64::from(t.cooldown_ms));
        p.direction = c3_scene::path::facing(p.cell, goal).unwrap_or(p.direction);
        let targets = self.reached(id, t, goal, at);
        self.events.push(Event::Cast {
            caster: id,
            skill: t.id,
            cell: goal,
            targets: targets.clone(),
        });
        let caster = self.player_fighter(id);
        // What the cast did: the life it took or gave back (a status: 1).
        let amount: u32 = match t.kind {
            SkillKind::Damage => (targets.iter())
                .map(|&target| self.skill_hits(id, t, target))
                .sum(),
            SkillKind::Heal => (targets.iter())
                .map(|&target| self.heal(id, t, target))
                .sum(),
            SkillKind::Status => 1,
        };
        if t.status != 0 {
            for &target in &targets {
                self.add_status(target, t.status, Some(id));
            }
        }
        if amount > 0 && self.players.contains_key(&id) {
            let args = vec![
                i64::from(t.id).into(),
                i64::from(amount).into(),
                caster.record().into(),
            ];
            self.script_event(id, "on_cast", args);
        }
    }

    /// Who a cast of `t` at `goal` reaches: its target, or everyone fitting in its area.
    fn reached(&self, caster: EntityId, t: &SkillType, goal: CellPos, at: CastAt) -> Vec<EntityId> {
        let area = t.area as i32;
        // A status aimed at oneself reaches friends; at a monster or the ground, foes.
        let kind = match (t.kind, t.target) {
            (SkillKind::Status, SkillTarget::Myself) => SkillKind::Heal,
            (SkillKind::Status, _) => SkillKind::Damage,
            (kind, _) => kind,
        };
        match kind {
            SkillKind::Damage if area > 0 => {
                let monsters = (self.monsters.values())
                    .filter(|m| m.is_alive() && distance(m.cell, goal) <= area)
                    .map(|m| m.id);
                // On PK maps a blast reaches other players too.
                let players = (self.players.values())
                    .filter(|p| self.pk && p.id != caster)
                    .filter(|p| p.is_alive() && distance(p.cell, goal) <= area)
                    .map(|p| p.id);
                monsters.chain(players).collect()
            }
            SkillKind::Damage => match at {
                CastAt::Entity(e) if self.is_foe(caster, e) => vec![e],
                _ => Vec::new(),
            },
            SkillKind::Heal | SkillKind::Status if area > 0 => (self.players.values())
                .filter(|p| p.is_alive() && distance(p.cell, goal) <= area)
                .map(|p| p.id)
                .collect(),
            SkillKind::Heal | SkillKind::Status => match at {
                CastAt::Entity(e) if self.living_player_cell(e).is_some() => vec![e],
                _ => vec![caster],
            },
        }
    }

    /// A damage skill's hit on `target`; the life it took.
    fn skill_hits(&mut self, id: EntityId, t: &SkillType, target: EntityId) -> u32 {
        let attacker = self.player_fighter(id);
        if self.players.contains_key(&target) {
            let defender = self.player_fighter(target);
            let damage = self.rules.skill_hit(&attacker, &defender, t, &mut self.rng);
            self.hurt_player(id, target, damage);
            return damage;
        }
        let Some(m) = self.monsters.get_mut(&target) else {
            return 0;
        };
        let defender = m.fighter(&*self.rules);
        let damage = self
            .rules
            .skill_hit(&attacker, &defender, t, &mut self.rng)
            .min(m.life);
        m.life -= damage;
        let dead = m.life == 0;
        self.events.push(Event::Attacked {
            attacker: id,
            target,
            damage,
        });
        self.rouse(target, id);
        let mut exp = self.rules.hit_exp(&attacker, &defender, damage);
        if dead {
            exp += self.rules.kill_exp(&attacker, &defender);
            self.kill_monster(target, Some(id));
        }
        self.gain_exp(id, exp);
        damage
    }

    /// A healing skill on `target`; the life it gave back.
    fn heal(&mut self, id: EntityId, t: &SkillType, target: EntityId) -> u32 {
        let healer = self.player_fighter(id);
        let amount = self.rules.skill_heal(&healer, t);
        let Some(p) = self.players.get_mut(&target) else {
            return 0;
        };
        let healed = amount.min(p.stats.max_life - p.life.min(p.stats.max_life));
        p.life += healed;
        self.events.push(Event::Healed {
            target,
            amount: healed,
        });
        healed
    }
}
