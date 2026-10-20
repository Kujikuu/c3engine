//! Players fighting players, on maps that allow it (the map list's `pk`). Elsewhere players
//! only fight monsters. A player who kills another counts it (`pk_kills`; names show it).

use crate::EntityId;
use crate::event::Event;
use crate::sim::Sim;
use c3_scene::CellPos;

impl Sim {
    /// Lets players attack each other on this map (or not).
    pub fn set_pk(&mut self, pk: bool) {
        self.pk = pk;
    }

    /// Whether players may attack each other here.
    pub fn pk(&self) -> bool {
        self.pk
    }

    /// Sets where players who die here come back.
    pub fn set_revive_point(&mut self, at: Option<c3_scene::PortalTarget>) {
        self.revive_point = at;
    }

    /// Makes fights between players here bring no crime and no PK points (or not).
    pub fn set_pk_free(&mut self, free: bool) {
        self.pk_free = free;
    }

    /// Player `id` attacks other players by `mode` from now on.
    pub fn set_pk_mode(&mut self, id: EntityId, mode: c3_core::settings::PkMode) {
        if let Some(p) = self.players.get_mut(&id) {
            p.pk_mode = mode;
            self.events.push(Event::PkModeSet { player: id, mode });
        }
    }

    /// Whether player `attacker`'s PK mode lets it hit player `target`: free hits anyone,
    /// peace no one, team anyone but its team, guild and spouse, arrest only bad names.
    pub(crate) fn mode_allows(&self, attacker: EntityId, target: EntityId) -> bool {
        use c3_core::settings::PkMode;
        let (Some(a), Some(t)) = (self.players.get(&attacker), self.players.get(&target)) else {
            return true;
        };
        match a.pk_mode {
            PkMode::Free => true,
            PkMode::Peace => false,
            PkMode::Arrest => self.mark_of(target).is_evil(),
            PkMode::Team => {
                let teammate = a.team.is_some() && a.team == t.team;
                let world = &self.world;
                let guild = |name: &str| world.guild_of(name).map(|(g, _)| g);
                let guildmate = guild(&a.name).is_some() && guild(&a.name) == guild(&t.name);
                let spouse = (a.texts.get(&self.settings.spouse_text))
                    .is_some_and(|s| s.eq_ignore_ascii_case(&t.name));
                !(teammate || guildmate || spouse)
            }
        }
    }

    /// Player `id`'s name mark now.
    fn mark_of(&self, id: EntityId) -> crate::NameMark {
        let Some(p) = self.players.get(&id) else {
            return crate::NameMark::Good;
        };
        let pk = &self.settings.pk;
        if p.pk >= pk.black_at {
            crate::NameMark::Black
        } else if p.pk >= pk.red_at {
            crate::NameMark::Red
        } else if p.crime_until > self.now_ms {
            crate::NameMark::Criminal
        } else {
            crate::NameMark::Good
        }
    }

    /// Keeps every player's name mark up to date and wears PK points off.
    pub(crate) fn tick_marks(&mut self) {
        let now = self.now_ms;
        let decay = (self.settings.pk.decay_seconds * 1000.0) as u64;
        let ids: Vec<EntityId> = self.players.keys().copied().collect();
        for id in ids {
            let p = self.players.get_mut(&id).expect("listed");
            if decay > 0 {
                if p.next_decay_ms == 0 {
                    p.next_decay_ms = now + decay;
                } else if now >= p.next_decay_ms {
                    p.next_decay_ms = now + decay;
                    if p.pk > 0 {
                        p.pk -= 1;
                    }
                }
            }
            let mark = self.mark_of(id);
            let p = self.players.get_mut(&id).expect("listed");
            if p.mark != mark {
                p.mark = mark;
                self.events.push(Event::Dressed { player: id });
            }
        }
    }

    /// Player `id` attacked player `target`: attacking a player of good name (on a map that
    /// isn't free-PK) makes it a criminal for a while.
    fn commit_crime(&mut self, id: EntityId, target: EntityId, seconds: f64) {
        if self.pk_free || self.mark_of(target).is_evil() || seconds <= 0.0 {
            return;
        }
        let until = self.now_ms + (seconds * 1000.0) as u64;
        if let Some(p) = self.players.get_mut(&id) {
            p.crime_until = p.crime_until.max(until);
        }
    }

    /// Player `killer` killed player `victim` (of the mark it had): a victim of good name
    /// earns the killer PK points, and a criminal killer stays one longer.
    fn count_pk_kill(&mut self, killer: EntityId, victim_mark: crate::NameMark) {
        if self.pk_free || victim_mark.is_evil() {
            return;
        }
        let pk = self.settings.pk.clone();
        let now = self.now_ms;
        if let Some(p) = self.players.get_mut(&killer) {
            p.pk = (p.pk + pk.kill_points).min(30_000);
            if p.crime_until > now {
                p.crime_until = now + (pk.crime_kill_seconds * 1000.0) as u64;
            }
        }
    }

    /// Where `target` stands, if `attacker` may fight it: a living monster, or on a PK map
    /// another living player.
    /// Neither may stand in a safe zone, nor two players in a no-PK zone.
    pub(crate) fn foe_cell(&self, attacker: EntityId, target: EntityId) -> Option<CellPos> {
        let from = (self.players.get(&attacker).map(|p| p.cell))
            .or_else(|| self.monsters.get(&attacker).map(|m| m.cell))?;
        if let Some(m) = self.monsters.get(&target) {
            return (m.is_alive() && self.may_fight(from, m.cell, false)).then_some(m.cell);
        }
        let Some(p) = self.players.get(&target) else {
            let cell = self.hittable_npc_cell(target)?;
            return self.may_fight(from, cell, false).then_some(cell);
        };
        let allowed =
            self.pk && target != attacker && p.is_alive() && self.mode_allows(attacker, target);
        (allowed && self.may_fight(from, p.cell, true)).then_some(p.cell)
    }

    /// Player `id` hits player `target` (on a PK map).
    pub(crate) fn player_hits_player(&mut self, id: EntityId, target: EntityId) {
        let attacker = self.player_fighter(id);
        let defender = self.player_fighter(target);
        let damage = self.rules.hit(&attacker, &defender, &mut self.rng);
        self.hurt_player(id, target, damage);
        self.wear_items(id, |t| t.bonus.attack_max > 0);
    }

    /// What player `victim` (of mark `mark`) loses to player `killer`, but on a free-PK map
    /// (TQ's ProcessPk 00418ee3): a guarded victim of an unguarded killer loses no
    /// experience and the killer bears the curse; another loses its share of experience (a
    /// killer of no higher level gets part of it); either loses part of its mana.
    fn pk_kill_losses(&mut self, killer: EntityId, victim: EntityId, mark: crate::NameMark) {
        if self.pk_free {
            return;
        }
        let pk = self.settings.pk.clone();
        let now = chrono::Utc::now().timestamp();
        let guarded = |id: EntityId| {
            !pk.curse_guard.is_empty()
                && (self.players.get(&id))
                    .is_some_and(|p| p.attributes.get(&pk.curse_guard).is_some_and(|t| *t > now))
        };
        let spared = guarded(victim) && !guarded(killer);
        let Some(v) = self.players.get_mut(&victim) else {
            return;
        };
        v.mana -= v.mana * pk.mana_lost_percent.min(100) / 100;
        let victim_level = v.level;
        if spared {
            if pk.curse_status != 0 {
                self.message(victim, "Your blessing takes effect! You lose no EXP.");
                self.add_status(killer, pk.curse_status, Some(victim));
            }
            return;
        }
        let divisor = match mark {
            crate::NameMark::Black => pk.exp_lost_divisors[2],
            crate::NameMark::Red => pk.exp_lost_divisors[1],
            _ => pk.exp_lost_divisors[0],
        };
        let lost = match divisor {
            0 => 0,
            d => v.exp / u64::from(d),
        };
        if lost == 0 {
            return;
        }
        v.exp -= lost;
        self.message(victim, format!("You lost {lost} EXP!"));
        let Some(k) = self.players.get(&killer) else {
            return;
        };
        if !pk.killer_share || k.level > victim_level {
            return;
        }
        let between = i64::from(k.level) - i64::from(victim_level);
        let percent = (2 * between + 50).max(10) as u64;
        let most = match (
            self.rules.level_exp(k.level),
            self.rules.level_time(k.level),
        ) {
            (Some(exp), Some(time)) if time > 0 => exp.saturating_mul(300) / time,
            _ => u64::MAX,
        };
        self.gain_exp(killer, (lost * percent / 100).min(most));
    }

    /// `damage` from player `id` to player `target`; a kill counts against the killer.
    pub(crate) fn hurt_player(&mut self, id: EntityId, target: EntityId, damage: u32) {
        let victim_mark = self.mark_of(target);
        let crime = self.settings.pk.crime_seconds;
        self.commit_crime(id, target, crime);
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
            self.note_enemy(target, id);
            self.pk_kill_losses(id, target, victim_mark);
            self.kill_player(target, Some(id));
            if let Some(killer) = self.players.get_mut(&id) {
                killer.pk_kills += 1;
            }
            self.count_pk_kill(id, victim_mark);
        } else {
            self.wear_items(target, |t| t.bonus.attack_max == 0);
        }
    }
}
