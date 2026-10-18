//! Status effects and buffs: a shield raising defence, a curse slowing a monster, poison
//! eating life, a stun. Skills put them on whoever they reach (or GM commands and scripts);
//! each lasts a while, may change life every so often, and changes its bearer's stats.

use crate::EntityId;
use crate::event::Event;
use crate::rules::{Stats, StatusType};
use crate::sim::Sim;
use serde::{Deserialize, Serialize};

/// A status someone bears now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveStatus {
    /// Its status type.
    pub id: u32,
    /// When (game ms) it wears off.
    pub until_ms: u64,
    /// When it next changes life (for those that do).
    pub(crate) next_tick_ms: u64,
    /// Who put it there (a kill by poison is theirs).
    pub(crate) from: Option<EntityId>,
}

/// `base` changed by every status in `statuses` (percent changes, added up).
pub(crate) fn with_statuses(
    base: Stats,
    statuses: &[ActiveStatus],
    rules: &dyn crate::Rules,
) -> Stats {
    let mut change = [0i32; 4];
    for t in statuses.iter().filter_map(|s| rules.status(s.id)) {
        change[0] += t.attack_percent;
        change[1] += t.defence_percent;
        change[2] += t.dodge_percent;
        change[3] += t.speed_percent;
    }
    let at = |v: u32, percent: i32| (i64::from(v) * i64::from((100 + percent).max(0)) / 100) as u32;
    // Faster means fewer milliseconds per step and per hit.
    let quicker = |ms: u32| (i64::from(ms) * 100 / i64::from((100 + change[3]).max(10))) as u32;
    Stats {
        attack_min: at(base.attack_min, change[0]),
        attack_max: at(base.attack_max, change[0]),
        defence: at(base.defence, change[1]),
        dodge: at(base.dodge, change[2]),
        dexterity: at(base.dexterity, change[2]),
        attack_ms: quicker(base.attack_ms),
        step_ms: quicker(base.step_ms),
        ..base
    }
}

impl Sim {
    /// Puts status `status` on `target` (a player or monster) for its time, from `from`;
    /// one already there starts over. False if there is no such status or no such target.
    pub fn add_status(&mut self, target: EntityId, status: u32, from: Option<EntityId>) -> bool {
        let Some(t) = self.rules.status(status).cloned() else {
            return false;
        };
        let active = ActiveStatus {
            id: status,
            until_ms: self.now_ms + u64::from(t.duration_ms),
            next_tick_ms: self.now_ms + u64::from(t.tick_ms.max(1)),
            from,
        };
        let Some(list) = self.statuses_mut(target) else {
            return false;
        };
        list.retain(|s| s.id != status);
        list.push(active);
        self.events.push(Event::StatusStarted { target, status });
        self.refresh_any_stats(target);
        true
    }

    /// Whether `id` bears a status that keeps it from acting.
    pub(crate) fn is_stunned(&self, id: EntityId) -> bool {
        let statuses = match self.players.get(&id) {
            Some(p) => &p.statuses,
            None => match self.monsters.get(&id) {
                Some(m) => &m.statuses,
                None => return false,
            },
        };
        statuses
            .iter()
            .any(|s| self.rules.status(s.id).is_some_and(|t| t.stun))
    }

    /// The statuses of `id`, if it is alive (the dead take none).
    fn statuses_mut(&mut self, id: EntityId) -> Option<&mut Vec<ActiveStatus>> {
        let alive = (self.players.get(&id).map(|p| p.is_alive()))
            .or_else(|| self.monsters.get(&id).map(|m| m.is_alive()))?;
        if alive { self.status_list(id) } else { None }
    }

    fn status_list(&mut self, id: EntityId) -> Option<&mut Vec<ActiveStatus>> {
        match self.players.get_mut(&id) {
            Some(p) => Some(&mut p.statuses),
            None => self.monsters.get_mut(&id).map(|m| &mut m.statuses),
        }
    }

    fn refresh_any_stats(&mut self, id: EntityId) {
        if self.players.contains_key(&id) {
            self.refresh_stats(id);
        } else {
            self.refresh_monster_stats(id);
        }
    }

    /// A monster's stats: its type's, changed by its statuses.
    pub(crate) fn refresh_monster_stats(&mut self, id: EntityId) {
        let Some(m) = self.monsters.get_mut(&id) else {
            return;
        };
        let base = self.rules.monster(m.kind).map_or(m.stats, |t| t.stats);
        m.stats = with_statuses(base, &m.statuses, &*self.rules);
        m.life = m.life.min(m.stats.max_life);
    }

    /// One tick of every status: life changes, and those worn off end. The dead lose theirs.
    pub(crate) fn tick_statuses(&mut self) {
        let now = self.now_ms;
        let bearers: Vec<(EntityId, bool, Vec<ActiveStatus>)> = (self.players.values())
            .filter(|p| !p.statuses.is_empty())
            .map(|p| (p.id, p.is_alive(), p.statuses.clone()))
            .chain(
                (self.monsters.values())
                    .filter(|m| !m.statuses.is_empty())
                    .map(|m| (m.id, m.is_alive(), m.statuses.clone())),
            )
            .collect();
        for (id, alive, statuses) in bearers {
            let before = statuses.len();
            let mut kept = Vec::new();
            for mut s in statuses {
                let Some(t) = self.rules.status(s.id).cloned().filter(|_| alive) else {
                    self.events.push(Event::StatusEnded {
                        target: id,
                        status: s.id,
                    });
                    continue;
                };
                // A tick due as it wears off still lands.
                if t.life_per_tick != 0 && now >= s.next_tick_ms {
                    s.next_tick_ms = now + u64::from(t.tick_ms.max(1));
                    self.status_life(id, &t, s.from);
                }
                if now >= s.until_ms {
                    self.events.push(Event::StatusEnded {
                        target: id,
                        status: s.id,
                    });
                } else {
                    kept.push(s);
                }
            }
            let ended = kept.len() != before;
            if let Some(list) = self.status_list(id) {
                *list = kept;
            }
            if ended {
                self.refresh_any_stats(id);
            }
        }
    }

    /// A status's life change on `id`: healing, or harm that may kill.
    fn status_life(&mut self, id: EntityId, t: &StatusType, from: Option<EntityId>) {
        let amount = t.life_per_tick.unsigned_abs();
        if t.life_per_tick > 0 {
            if let Some(p) = self.players.get_mut(&id) {
                let healed = amount.min(p.stats.max_life.saturating_sub(p.life));
                p.life += healed;
                self.events.push(Event::Healed {
                    target: id,
                    amount: healed,
                });
            } else if let Some(m) = self.monsters.get_mut(&id) {
                let healed = amount.min(m.stats.max_life.saturating_sub(m.life));
                m.life += healed;
                self.events.push(Event::Healed {
                    target: id,
                    amount: healed,
                });
            }
            return;
        }
        let attacker = from.unwrap_or(id);
        if let Some(p) = self.players.get_mut(&id) {
            let damage = amount.min(p.life);
            p.life -= damage;
            let dead = p.life == 0;
            self.events.push(Event::Attacked {
                attacker,
                target: id,
                damage,
            });
            if dead {
                self.kill_player(id, from);
            }
        } else if let Some(m) = self.monsters.get_mut(&id) {
            let damage = amount.min(m.life);
            m.life -= damage;
            let dead = m.life == 0;
            self.events.push(Event::Attacked {
                attacker,
                target: id,
                damage,
            });
            if dead {
                self.kill_monster(id, from.filter(|f| self.players.contains_key(f)));
            }
        }
    }
}
