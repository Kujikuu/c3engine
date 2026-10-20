//! Paid broadcasts to every player (TQ's Pigeon.dll, adapters spec §4): a player from the
//! settings' level pays to have a line told to the whole world; lines wait in one queue on
//! the shared world state, each its gap after the one before, and every map tells its own
//! players a line when its time comes. A waiting line may be moved to the front or up a few
//! places for a further fee.

use crate::EntityId;
use crate::event::Event;
use crate::sim::Sim;
use c3_script::WorldChange;
use serde::{Deserialize, Serialize};

/// A broadcast line: who pays for it, what it says, and when (unix seconds) it is told.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Broadcast {
    pub id: u32,
    pub author: String,
    pub text: String,
    pub at: i64,
}

/// What a player does about broadcasts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BroadcastAction {
    /// Pay for `0` to be told to everyone.
    Send(String),
    /// Move one's waiting line `0` to the front of the queue.
    First(u32),
    /// Move one's waiting line `0` up the settings' places.
    Bump(u32),
}

/// How long ago (seconds) a line may have come due and still be told by a map that only
/// now sees it.
const LATE_SECS: i64 = 30;

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

impl Sim {
    pub(crate) fn broadcast_action(&mut self, id: EntityId, action: BroadcastAction) {
        if self.settings.broadcast.cost == 0 {
            return;
        }
        match action {
            BroadcastAction::Send(text) => self.send_broadcast(id, &text),
            BroadcastAction::First(line) => self.move_broadcast(id, line, true),
            BroadcastAction::Bump(line) => self.move_broadcast(id, line, false),
        }
    }

    /// Takes `cost` of the broadcast currency from player `id`, if it has it.
    fn pay_broadcast(&mut self, id: EntityId, cost: u64) -> bool {
        let currency = self.settings.broadcast.currency.clone();
        let Some(p) = self.players.get(&id) else {
            return false;
        };
        if self.funds(p, &currency) < cost {
            return false;
        }
        let p = self.players.get_mut(&id).expect("checked");
        match currency.is_empty() {
            true => p.money -= cost,
            false => *p.wallet.entry(currency).or_default() -= cost,
        }
        true
    }

    fn send_broadcast(&mut self, id: EntityId, text: &str) {
        let b = self.settings.broadcast.clone();
        let text: String = text.replace(['\r', '\n'], "");
        let Some(p) = self.players.get(&id) else {
            return;
        };
        if p.level < b.min_level {
            return;
        }
        let funds = self.funds(p, &b.currency);
        if funds < b.cost {
            return self.message(id, "You don`t have enough CPs to broadcast your message.");
        }
        if text.trim().is_empty() {
            return self.message(id, "Please enter your message before submit.");
        }
        if text.chars().count() > b.max_len {
            return self.message(id, "Failed to submit. The message is too long.");
        }
        let author = p.name.clone();
        let now = now_secs();
        let waiting = (self.world.broadcasts.iter())
            .filter(|l| l.at > now && l.author == author)
            .count();
        if waiting >= b.max_queued {
            let text = "You have five messages waiting for broadcast. Please submit again later.";
            return self.message(id, text);
        }
        if !self.pay_broadcast(id, b.cost) {
            return;
        }
        let last = self.world.broadcasts.iter().map(|l| l.at).max();
        let at = last.map_or(now, |last| (last + b.gap_secs).max(now));
        let line = self
            .world
            .broadcasts
            .iter()
            .map(|l| l.id)
            .max()
            .unwrap_or(0)
            + 1;
        self.change_world(WorldChange::Broadcast {
            id: line,
            author,
            text,
            at,
        });
        self.message(id, "The message has been submitted to broadcast.");
    }

    /// Moves player `id`'s waiting line to the front (`first`) or up the settings' places,
    /// and times the queue again from the line told last.
    fn move_broadcast(&mut self, id: EntityId, line: u32, first: bool) {
        let b = self.settings.broadcast.clone();
        let now = now_secs();
        let me = self
            .players
            .get(&id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let mut queue: Vec<_> = (self.world.broadcasts.iter())
            .filter(|l| l.at > now)
            .cloned()
            .collect();
        let Some(at) = queue.iter().position(|l| l.id == line && l.author == me) else {
            return self.message(
                id,
                "Failed to prioritize. The message has been broadcasted.",
            );
        };
        let cost = if first { b.first_cost } else { b.bump_cost };
        if !self.pay_broadcast(id, cost) {
            return self.message(id, "You don`t have enough CPs to prioritize your message.");
        }
        let moved = queue.remove(at);
        let to = if first {
            0
        } else {
            at.saturating_sub(b.bump_places)
        };
        queue.insert(to, moved);
        let told = (self.world.broadcasts.iter())
            .filter(|l| l.at <= now)
            .map(|l| l.at)
            .max();
        let mut next = told.map_or(now, |t| (t + b.gap_secs).max(now));
        for line in queue {
            self.change_world(WorldChange::Broadcast {
                at: next,
                id: line.id,
                author: line.author,
                text: line.text,
            });
            next += b.gap_secs;
        }
    }

    /// Lines whose time has come, told to this map's players once.
    pub(crate) fn tell_broadcasts(&mut self) {
        let now = now_secs();
        let due: Vec<Broadcast> = (self.world.broadcasts.iter())
            .filter(|l| l.at <= now && l.at > now - LATE_SECS)
            .filter(|l| !self.broadcasts_told.contains(&l.id))
            .cloned()
            .collect();
        let players: Vec<(EntityId, String)> = self
            .players
            .values()
            .map(|p| (p.id, p.name.clone()))
            .collect();
        for line in due {
            self.broadcasts_told.insert(line.id);
            for (player, hearer) in &players {
                self.events.push(Event::Chat {
                    player: *player,
                    name: line.author.clone(),
                    text: line.text.clone(),
                    channel: crate::ChatChannel::Broadcast,
                    to: Some(*player),
                    hearer: hearer.clone(),
                });
            }
        }
    }
}
