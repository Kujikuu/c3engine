//! The world seen from far away: a server sends each player what is around them as a
//! [`WorldUpdate`] (entities that came into view or changed, those gone, and the events they
//! could see), and a client keeps a mirror [`Sim`] by applying them. The mirror never
//! simulates; the UI and views read it exactly as they read a local simulation.

use crate::EntityId;
use crate::entity::{GroundItem, Monster, Player};
use crate::event::Event;
use crate::sim::Sim;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One entity as sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EntityState {
    Player(Box<Player>),
    Monster(Monster),
    Item(GroundItem),
}

impl EntityState {
    pub fn id(&self) -> EntityId {
        match self {
            Self::Player(p) => p.id,
            Self::Monster(m) => m.id,
            Self::Item(i) => i.id,
        }
    }
}

/// What changed in a player's view since the last update.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldUpdate {
    /// Game time on the server.
    pub now_ms: u64,
    /// Entities new in view or changed.
    pub changed: Vec<EntityState>,
    /// Entities that left the view or the world.
    pub gone: Vec<EntityId>,
    /// What happened that the player could see.
    pub events: Vec<Event>,
}

impl WorldUpdate {
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.gone.is_empty() && self.events.is_empty()
    }
}

/// What a player was last sent, so the next update holds only changes.
#[derive(Debug, Default)]
pub struct Viewer {
    known: BTreeMap<EntityId, EntityState>,
}

impl Player {
    /// The player as others see it: no bag, variables, dialogue or plans.
    fn public(&self) -> Player {
        Player {
            bag: Vec::new(),
            flags: Default::default(),
            dialogue: None,
            shop: None,
            path: Default::default(),
            exp: 0,
            money: 0,
            wallet: Default::default(),
            skills: Vec::new(),
            cooldowns: Default::default(),
            ..self.clone()
        }
    }
}

impl Event {
    /// The player only this event is for (private messages and their own trade).
    fn private_to(&self) -> Option<EntityId> {
        match *self {
            Event::ExpGained { player, .. }
            | Event::Bought { player, .. }
            | Event::Sold { player, .. }
            | Event::Message { player, .. }
            | Event::Notice { player, .. }
            | Event::Dialogue { player }
            | Event::ShopOpened { player, .. }
            | Event::CommandReply { player, .. } => Some(player),
            _ => None,
        }
    }

    /// The entities an event is about (anyone seeing one sees the event).
    fn entities(&self) -> Vec<EntityId> {
        match *self {
            Event::Appeared(id) | Event::Vanished(id) | Event::Revived(id) => vec![id],
            Event::Moved { id, .. }
            | Event::Placed { id, .. }
            | Event::Jumped { id, .. }
            | Event::Died { id, .. } => vec![id],
            Event::Attacked {
                attacker, target, ..
            } => vec![attacker, target],
            Event::Cast {
                caster,
                ref targets,
                ..
            } => std::iter::once(caster)
                .chain(targets.iter().copied())
                .collect(),
            Event::Healed { target, .. } => vec![target],
            Event::LeveledUp { player, .. }
            | Event::PickedUp { player, .. }
            | Event::Used { player, .. }
            | Event::Dressed { player }
            | Event::Chat { player, .. } => vec![player],
            _ => Vec::new(),
        }
    }
}

impl Sim {
    /// What player `id` should be sent now, given what `viewer` holds and this tick's
    /// `events`; `viewer` then holds it too.
    pub fn update_for(&self, id: EntityId, viewer: &mut Viewer, events: &[Event]) -> WorldUpdate {
        let mut update = WorldUpdate {
            now_ms: self.now_ms,
            ..Default::default()
        };
        let Some(me) = self.players.get(&id) else {
            return update;
        };
        let mut seen: BTreeMap<EntityId, EntityState> = BTreeMap::new();
        seen.insert(id, EntityState::Player(Box::new(me.clone())));
        for (other, _) in self.index.near(me.cell, self.settings.view_range) {
            let state = if let Some(p) = self.players.get(&other) {
                EntityState::Player(Box::new(p.public()))
            } else if let Some(m) = self.monsters.get(&other) {
                EntityState::Monster(m.clone())
            } else if let Some(i) = self.items.get(&other) {
                EntityState::Item(i.clone())
            } else {
                continue;
            };
            seen.entry(other).or_insert(state);
        }
        let was: BTreeSet<EntityId> = viewer.known.keys().copied().collect();
        for (other, state) in &seen {
            if viewer.known.get(other) != Some(state) {
                update.changed.push(state.clone());
            }
        }
        update.gone = was
            .iter()
            .filter(|e| !seen.contains_key(e))
            .copied()
            .collect();
        // Events about anyone seen now or until now; the mirror makes its own comings and
        // goings from `changed` and `gone`.
        update.events = events
            .iter()
            .filter(|e| match e.private_to() {
                Some(player) => player == id,
                None => {
                    !matches!(e, Event::Appeared(_) | Event::Vanished(_))
                        && e.entities()
                            .iter()
                            .any(|x| seen.contains_key(x) || was.contains(x))
                }
            })
            .cloned()
            .collect();
        viewer.known = seen;
        update
    }

    /// Mirrors `update` (a client's copy of the server's world): entities come, change and
    /// go, and the events are queued for `take_events` as if they had happened here.
    pub fn apply_update(&mut self, update: WorldUpdate) {
        self.now_ms = update.now_ms;
        for state in update.changed {
            let id = state.id();
            let (old, new) = match state {
                EntityState::Player(p) => {
                    let new = p.cell;
                    (self.players.insert(id, *p).map(|o| o.cell), new)
                }
                EntityState::Monster(m) => {
                    let new = m.cell;
                    (self.monsters.insert(id, m).map(|o| o.cell), new)
                }
                EntityState::Item(i) => {
                    let new = i.cell;
                    (self.items.insert(id, i).map(|o| o.cell), new)
                }
            };
            match old {
                Some(from) => self.index.moved(id, from, new),
                None => {
                    self.index.insert(id, new);
                    self.events.push(Event::Appeared(id));
                }
            }
        }
        self.events.extend(update.events);
        for id in update.gone {
            let cell = self
                .players
                .remove(&id)
                .map(|p| p.cell)
                .or_else(|| self.monsters.remove(&id).map(|m| m.cell))
                .or_else(|| self.items.remove(&id).map(|i| i.cell));
            if let Some(cell) = cell {
                self.index.remove(id, cell);
                self.events.push(Event::Vanished(id));
            }
        }
    }
}
