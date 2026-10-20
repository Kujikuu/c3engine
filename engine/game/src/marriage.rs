//! Marrying: a single player proposes to one near it (`Input::Propose`) whom the rules let
//! it marry (`Rules::may_marry`); the one proposed to accepts (`Input::AcceptProposal`) and
//! each is the other's spouse (the player text variable the settings' `spouse_text` names).
//! Things for a player who may be on another map or offline (an ex-spouse's divorce) wait on
//! the shared world state as deliveries until the player is on a map.

use crate::sim::Sim;
use crate::{EntityId, Event, distance};
use c3_script::{Delivery, WorldChange};

impl Sim {
    /// Whether player `id` has no spouse.
    fn single(&self, id: EntityId) -> bool {
        let key = &self.settings.spouse_text;
        (self.players.get(&id)).is_some_and(|p| p.texts.get(key).is_none_or(String::is_empty))
    }

    /// Player `id` proposes to player `target`: both single, near, and the rules let them.
    pub fn propose(&mut self, id: EntityId, target: EntityId) {
        let range = self.settings.view_range;
        let near = match (self.players.get(&id), self.players.get(&target)) {
            (Some(a), Some(b)) => a.is_alive() && b.is_alive() && distance(a.cell, b.cell) <= range,
            _ => false,
        };
        if self.settings.spouse_text.is_empty() || id == target || !near || !self.single(id) {
            return;
        }
        if !self
            .rules
            .may_marry(&self.player_fighter(id), &self.player_fighter(target))
        {
            self.message(id, "You can't marry that player.");
            return;
        }
        self.courting.insert(id, target);
        self.events.push(Event::Proposed {
            player: target,
            from: id,
        });
    }

    /// Player `id` accepts the proposal of `from`: each is the other's spouse.
    pub fn accept_proposal(&mut self, id: EntityId, from: EntityId) {
        let range = self.settings.view_range;
        let near = match (self.players.get(&id), self.players.get(&from)) {
            (Some(a), Some(b)) => distance(a.cell, b.cell) <= range,
            _ => false,
        };
        if self.courting.get(&from) != Some(&id) || !near || !self.single(id) {
            return;
        }
        self.courting.remove(&from);
        let key = self.settings.spouse_text.clone();
        let names = [from, id].map(|p| self.players[&p].name.clone());
        for (who, spouse) in [(from, &names[1]), (id, &names[0])] {
            if let Some(p) = self.players.get_mut(&who) {
                p.texts.insert(key.clone(), spouse.clone());
            }
        }
        self.events.push(Event::Married {
            player: id,
            spouse: from,
        });
        let line = format!(
            "Congratulations! {} and {} got married!",
            names[1], names[0]
        );
        let players: Vec<EntityId> = self.players.keys().copied().collect();
        for player in players {
            let text = line.clone();
            self.events.push(Event::Notice { player, text });
        }
    }

    /// Gives the players on this map what waits for them on the shared world state.
    pub(crate) fn deliver_waiting(&mut self) {
        let here: Vec<(EntityId, String)> = (self.players.values())
            .filter(|p| self.world.deliveries.contains_key(&p.name))
            .map(|p| (p.id, p.name.clone()))
            .collect();
        for (id, name) in here {
            let waiting = self
                .world
                .deliveries
                .get(&name)
                .cloned()
                .unwrap_or_default();
            for delivery in waiting {
                match delivery {
                    Delivery::Text { name, value } => {
                        if let Some(p) = self.players.get_mut(&id) {
                            p.texts.insert(name, value);
                        }
                    }
                    // Words a friend left while it was offline, as a line said to it.
                    Delivery::Word { from, text } => self.events.push(Event::Chat {
                        player: id,
                        name: from,
                        text,
                        channel: crate::ChatChannel::LeaveWord,
                        to: Some(id),
                        hearer: name.clone(),
                    }),
                    Delivery::Item { kind } => {
                        if let Ok(kind) = u32::try_from(kind) {
                            self.give_items(id, kind, 1);
                        }
                    }
                }
            }
            self.change_world(WorldChange::Delivered { to: name });
        }
    }
}
