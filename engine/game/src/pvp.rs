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

    /// Where `target` stands, if `attacker` may fight it: a living monster, or on a PK map
    /// another living player.
    /// Neither may stand in a safe zone, nor two players in a no-PK zone.
    pub(crate) fn foe_cell(&self, attacker: EntityId, target: EntityId) -> Option<CellPos> {
        let from = (self.players.get(&attacker).map(|p| p.cell))
            .or_else(|| self.monsters.get(&attacker).map(|m| m.cell))?;
        if let Some(m) = self.monsters.get(&target) {
            return (m.is_alive() && self.may_fight(from, m.cell, false)).then_some(m.cell);
        }
        let p = self.players.get(&target)?;
        let allowed = self.pk && target != attacker && p.is_alive();
        (allowed && self.may_fight(from, p.cell, true)).then_some(p.cell)
    }

    /// Player `id` hits player `target` (on a PK map).
    pub(crate) fn player_hits_player(&mut self, id: EntityId, target: EntityId) {
        let attacker = self.players[&id].fighter();
        let defender = self.players[&target].fighter();
        let damage = self.rules.hit(&attacker, &defender, &mut self.rng);
        self.hurt_player(id, target, damage);
        self.wear_items(id, |t| t.bonus.attack_max > 0);
    }

    /// `damage` from player `id` to player `target`; a kill counts against the killer.
    pub(crate) fn hurt_player(&mut self, id: EntityId, target: EntityId, damage: u32) {
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
            if let Some(killer) = self.players.get_mut(&id) {
                killer.pk_kills += 1;
            }
        } else {
            self.wear_items(target, |t| t.bonus.attack_max == 0);
        }
    }
}
