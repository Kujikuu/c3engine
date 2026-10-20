//! Storage: items and money a player keeps with storage NPCs (a warehouse). Each NPC keeps
//! its own items for the player; the money kept is one sum. A storage holds
//! `gameplay.storage_size` items.

use crate::EntityId;
use crate::event::Event;
use crate::sim::Sim;

/// Most money a player keeps in storage (TQ's).
const MOST_SAVED: u64 = 1_000_000_000;

impl Sim {
    /// Whether player `id` stands near NPC `npc` (a storage is used there).
    fn near_npc(&self, id: EntityId, npc: u32) -> bool {
        let Some(p) = self.players.get(&id).filter(|p| p.is_alive()) else {
            return false;
        };
        (self.npcs.iter())
            .any(|n| n.id == npc && crate::distance(n.cell, p.cell) <= self.settings.trade_range)
    }

    /// Shows player `id` NPC `npc`'s storage.
    pub fn open_storage(&mut self, id: EntityId, npc: u32) {
        if self.near_npc(id, npc) {
            self.events.push(Event::StorageOpened { player: id, npc });
        }
    }

    /// Puts bag item `item` in NPC `npc`'s storage.
    pub fn store(&mut self, id: EntityId, npc: u32, item: u32) {
        if !self.near_npc(id, npc) {
            return;
        }
        let size = self.settings.storage_size.max(1) as usize;
        let p = self.players.get_mut(&id).expect("near");
        let Some(at) = p.bag.iter().position(|i| i.id == item) else {
            return;
        };
        let kept = p.storage.entry(npc).or_default();
        if kept.len() >= size {
            self.message(id, "The storage is full.");
            return;
        }
        let it = p.bag.remove(at);
        kept.push(it);
        self.events.push(Event::StorageChanged { player: id, npc });
    }

    /// Takes item `item` out of NPC `npc`'s storage into the bag.
    pub fn retrieve(&mut self, id: EntityId, npc: u32, item: u32) {
        if !self.near_npc(id, npc) {
            return;
        }
        let bag_size = self.bag_size();
        let p = self.players.get_mut(&id).expect("near");
        if p.bag.len() >= bag_size {
            self.message(id, "Your bag is full.");
            return;
        }
        let Some(kept) = p.storage.get_mut(&npc) else {
            return;
        };
        let Some(at) = kept.iter().position(|i| i.id == item) else {
            return;
        };
        let it = kept.remove(at);
        p.bag.push(it);
        self.events.push(Event::StorageChanged { player: id, npc });
    }

    /// Takes everything that waits for player `id` (its deposits) into its wallet.
    pub fn withdraw(&mut self, id: EntityId) {
        let Some(name) = self.players.get(&id).map(|p| p.name.clone()) else {
            return;
        };
        let Some(waiting) = self.world_state().deposits.get(&name).cloned() else {
            return;
        };
        self.change_world(c3_script::WorldChange::Deposit {
            name,
            currency: String::new(),
            amount: 0,
        });
        let p = self.players.get_mut(&id).expect("checked above");
        for (currency, amount) in &waiting {
            *p.wallet.entry(currency.clone()).or_default() += amount;
        }
        for (currency, amount) in waiting {
            self.events.push(Event::Withdrawn {
                player: id,
                currency,
                amount,
            });
        }
    }

    /// Moves `amount` of money into storage (`save`) or out of it, at NPC `npc`.
    pub fn move_money(&mut self, id: EntityId, npc: u32, amount: u64, save: bool) {
        if !self.near_npc(id, npc) || amount == 0 {
            return;
        }
        let p = self.players.get_mut(&id).expect("near");
        let fits = match save {
            true => amount <= p.money && p.saved_money + amount <= MOST_SAVED,
            false => amount <= p.saved_money,
        };
        if !fits {
            self.message(id, "You haven't so much money.");
            return;
        }
        if save {
            p.money -= amount;
            p.saved_money += amount;
        } else {
            p.saved_money -= amount;
            p.money += amount;
        }
        self.events.push(Event::StorageChanged { player: id, npc });
    }
}
