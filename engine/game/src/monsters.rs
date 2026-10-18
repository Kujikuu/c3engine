//! Monsters: born in spawn areas, they wander near home, notice and chase players, hit them,
//! and die leaving drops. Hits between players and monsters are settled here too.

use crate::entity::{GroundItem, Monster};
use crate::event::Event;
use crate::rules::Drop;
use crate::sim::Sim;
use crate::{EntityId, distance, ms};
use c3_scene::CellPos;
use c3_scene::path::{self, DIRECTIONS, PathRules};

/// Chance per tick (of 1000) that an idle monster takes a wandering step.
const WANDER_CHANCE: u32 = 15;
/// Tries to find a free cell for a newborn monster.
const SPAWN_TRIES: usize = 20;

impl Sim {
    /// Fills every spawn area to its most monsters alive (a new world starts full).
    pub(crate) fn fill_spawns(&mut self) {
        for area in 0..self.spawns.len() {
            let missing = self.spawns[area].spawn.max_alive;
            self.spawn_wave(area, missing);
        }
    }

    pub(crate) fn tick_spawns(&mut self) {
        for area in 0..self.spawns.len() {
            let a = &self.spawns[area];
            if a.alive < a.spawn.max_alive && self.now_ms >= a.next_wave_ms {
                let count = a.spawn.per_wave.max(1).min(a.spawn.max_alive - a.alive);
                self.spawn_wave(area, count);
            }
        }
    }

    fn spawn_wave(&mut self, area: usize, count: u32) {
        let spawn = self.spawns[area].spawn.clone();
        self.spawns[area].next_wave_ms = self.now_ms + u64::from(spawn.respawn_secs) * 1000;
        let Some(kind) = self.rules.monster(spawn.monster).cloned() else {
            self.events.push(Event::Warning(format!(
                "Spawn area {}: there is no monster type {}",
                spawn.id, spawn.monster
            )));
            // Don't warn every wave.
            self.spawns[area].next_wave_ms = u64::MAX;
            return;
        };
        for _ in 0..count {
            let Some(cell) = (0..SPAWN_TRIES).find_map(|_| {
                let c = (
                    self.rng.range(spawn.min.0.into(), spawn.max.0.into()) as i32,
                    self.rng.range(spawn.min.1.into(), spawn.max.1.into()) as i32,
                );
                (self.walkable(c) && !self.monster_on(c)).then_some(c)
            }) else {
                continue;
            };
            let id = self.new_entity();
            let direction = self.rng.range(0, 7) as u8;
            self.monsters.insert(
                id,
                Monster {
                    id,
                    kind: kind.id,
                    level: kind.level,
                    cell,
                    direction,
                    life: kind.stats.max_life,
                    stats: kind.stats,
                    statuses: Vec::new(),
                    home: cell,
                    spawn: Some(spawn.id),
                    target: None,
                    next_step_ms: 0,
                    next_attack_ms: 0,
                    died_at_ms: None,
                },
            );
            self.index.insert(id, cell);
            self.spawns[area].alive += 1;
            self.events.push(Event::Appeared(id));
        }
    }

    /// A living monster stands on `cell`.
    fn monster_on(&self, cell: CellPos) -> bool {
        self.index
            .near(cell, 0)
            .any(|(id, _)| self.monsters.get(&id).is_some_and(|m| m.is_alive()))
    }

    pub(crate) fn tick_monster(&mut self, id: EntityId) {
        let now = self.now_ms;
        let (leash, wander) = (self.settings.monster_leash, self.settings.monster_wander);
        let m = &self.monsters[&id];
        if let Some(died) = m.died_at_ms {
            if now >= died + ms(self.settings.corpse_seconds) {
                self.remove_monster(id);
            }
            return;
        }
        if self.is_stunned(id) {
            return;
        }
        let m = &self.monsters[&id];
        let (cell, home, kind) = (m.cell, m.home, m.kind);
        let view = self.rules.monster(kind).map_or(0, |t| t.view_range) as i32;
        let aggressive = self.rules.monster(kind).is_some_and(|t| t.aggressive);
        let boss = self.is_boss(id);
        // Drop a target that died, left or led it too far from home (bosses follow anywhere).
        let target = m.target.filter(|t| {
            self.players.get(t).is_some_and(|p| {
                p.is_alive()
                    && distance(p.cell, cell) <= view.max(1) * 2
                    && (boss || distance(cell, home) <= leash)
                    && self.may_fight(cell, p.cell, false)
            })
        });
        let target = target.or_else(|| {
            if !aggressive {
                return None;
            }
            self.index
                .near(cell, view)
                .filter(|(e, c)| {
                    self.players.get(e).is_some_and(|p| p.is_alive())
                        && self.may_fight(cell, *c, false)
                })
                .min_by_key(|(_, c)| distance(*c, cell))
                .map(|(e, _)| e)
        });
        self.monsters.get_mut(&id).expect("ticking").target = target;
        match target {
            Some(t) if self.wants_to_flee(id) => {
                let from = self.players[&t].cell;
                self.step_monster(id, Self::away(cell, from));
            }
            Some(t) => {
                let goal = self.players[&t].cell;
                let m = self.monsters.get_mut(&id).expect("ticking");
                if distance(cell, goal) <= m.stats.attack_range.max(1) as i32 {
                    if now >= m.next_attack_ms {
                        m.next_attack_ms = now + u64::from(m.stats.attack_ms.max(1));
                        m.direction = path::facing(cell, goal).unwrap_or(m.direction);
                        match self.monster_skill_now(id) {
                            Some(skill) => self.monster_casts(id, &skill, t),
                            None => self.monster_hits(id, t),
                        }
                    }
                } else {
                    self.step_monster(id, goal);
                }
            }
            None if distance(cell, home) > wander => self.step_monster(id, home),
            None => {
                if self.rng.chance(WANDER_CHANCE, 1000) {
                    let d = DIRECTIONS[self.rng.range(0, 7) as usize];
                    let to = (cell.0 + d.0, cell.1 + d.1);
                    if distance(to, home) <= wander {
                        self.step_monster(id, to);
                    }
                }
            }
        }
    }

    /// One step towards `goal` when it is time, the way TQ monsters move: straight at it,
    /// else the nearest directions around, avoiding other monsters.
    fn step_monster(&mut self, id: EntityId, goal: CellPos) {
        let now = self.now_ms;
        let m = &self.monsters[&id];
        if now < m.next_step_ms {
            return;
        }
        let Some(ahead) = path::facing(m.cell, goal) else {
            return;
        };
        let from = m.cell;
        let rules = PathRules::default();
        let step = [0i8, 1, -1, 2, -2].into_iter().find_map(|turn| {
            let dir = (ahead as i8 + turn).rem_euclid(8) as u8;
            let d = DIRECTIONS[dir as usize];
            let to = (from.0 + d.0, from.1 + d.1);
            (path::can_step(&self.grid, from, to, &rules) && !self.monster_on(to))
                .then_some((dir, to))
        });
        let m = self.monsters.get_mut(&id).expect("stepping");
        m.next_step_ms = now + u64::from(m.stats.step_ms.max(1));
        let Some((dir, to)) = step else { return };
        m.direction = dir;
        m.cell = to;
        self.index.moved(id, from, to);
        self.events.push(Event::Moved {
            id,
            to,
            running: false,
        });
    }

    fn remove_monster(&mut self, id: EntityId) {
        let Some(m) = self.monsters.remove(&id) else {
            return;
        };
        self.index.remove(id, m.cell);
        if let Some(area) = self.spawns.iter_mut().find(|a| Some(a.spawn.id) == m.spawn) {
            area.alive = area.alive.saturating_sub(1);
        }
        self.events.push(Event::Vanished(id));
    }

    // ---- combat ----

    pub(crate) fn player_hits(&mut self, id: EntityId, target: EntityId) {
        let attacker = self.players[&id].fighter();
        let level = attacker.level;
        let m = self.monsters.get_mut(&target).expect("hitting a monster");
        let defender = m.fighter();
        let damage = self
            .rules
            .hit(&attacker, &defender, &mut self.rng)
            .min(m.life);
        m.life -= damage;
        let dead = m.life == 0;
        self.events.push(Event::Attacked {
            attacker: id,
            target,
            damage,
        });
        self.rouse(target, id);
        let mut exp = self.rules.hit_exp(level, &defender, damage);
        if dead {
            exp += self.rules.kill_exp(level, &defender);
            self.kill_monster(target, Some(id));
        }
        self.gain_exp(id, exp);
        self.wear_items(id, |t| t.bonus.attack_max > 0);
    }

    fn monster_hits(&mut self, id: EntityId, target: EntityId) {
        let attacker = self.monsters[&id].fighter();
        let defender = self.players[&target].fighter();
        let damage = self.rules.hit(&attacker, &defender, &mut self.rng);
        self.monster_hurts(id, target, damage);
    }

    /// `damage` from monster `id` to player `target`.
    pub(crate) fn monster_hurts(&mut self, id: EntityId, target: EntityId, damage: u32) {
        let Some(p) = self.players.get_mut(&target) else {
            return;
        };
        let damage = damage.min(p.life);
        p.life -= damage;
        let dead = p.life == 0;
        self.events.push(Event::Attacked {
            attacker: id,
            target,
            damage,
        });
        if dead {
            self.kill_player(target, Some(id));
        } else {
            self.wear_items(target, |t| t.bonus.attack_max == 0);
        }
    }

    pub(crate) fn kill_monster(&mut self, id: EntityId, killer: Option<EntityId>) {
        let now = self.now_ms;
        let m = self.monsters.get_mut(&id).expect("killing");
        m.died_at_ms = Some(now);
        m.target = None;
        let (cell, kind) = (m.cell, m.kind);
        self.events.push(Event::Died { id, killer });
        if let Some(player) = killer {
            self.count_kill(player, kind);
            self.script_event(player, "on_kill", vec![i64::from(kind).into()]);
        }
        let drops = self.rules.drops(kind, &mut self.rng);
        for drop in drops {
            self.drop_at(cell, drop);
        }
    }

    /// Lays `drop` on the free cell nearest `cell`.
    fn drop_at(&mut self, cell: CellPos, drop: Drop) {
        let free = |s: &Sim, c: CellPos| {
            s.walkable(c) && !s.index.near(c, 0).any(|(e, _)| s.items.contains_key(&e))
        };
        let Some(spot) = (0..=2).find_map(|r| {
            (-r..=r)
                .flat_map(|dy| (-r..=r).map(move |dx| (cell.0 + dx, cell.1 + dy)))
                .find(|&c| free(self, c))
        }) else {
            return;
        };
        let (money, kind) = match drop {
            Drop::Money(m) => (m, None),
            Drop::Item(k) => (0, Some(k)),
        };
        let id = self.new_entity();
        self.items.insert(
            id,
            GroundItem {
                id,
                cell: spot,
                money,
                kind,
                expires_ms: self.now_ms + ms(self.settings.item_life_seconds),
            },
        );
        self.index.insert(id, spot);
        self.events.push(Event::Appeared(id));
    }

    pub(crate) fn expire_items(&mut self) {
        let now = self.now_ms;
        let expired: Vec<EntityId> = self
            .items
            .values()
            .filter(|i| now >= i.expires_ms)
            .map(|i| i.id)
            .collect();
        for id in expired {
            let item = self.items.remove(&id).expect("listed above");
            self.index.remove(id, item.cell);
            self.events.push(Event::Vanished(id));
        }
    }
}
