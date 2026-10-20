//! Chests: bag items that hold other items (TQ's sashes, CDlgPortableChest5065), each of the
//! settings' `chests` kinds holding so many. A chest is opened by using it (not while vending
//! or trading); a bag item is put in (not another chest) and taken back out. What a chest
//! holds is kept with its owner by the chest's item id.

use crate::EntityId;
use crate::event::Event;
use crate::sim::Sim;

impl Sim {
    /// How many items a chest of item kind `kind` holds; none: it is no chest.
    pub fn chest_size(&self, kind: u32) -> Option<u32> {
        (self.settings.chests.iter())
            .find(|[k, _]| *k == kind)
            .map(|[_, size]| *size)
    }

    /// The kind of bag item `item` of player `id` when it is a chest.
    fn chest_in_bag(&self, id: EntityId, item: u32) -> Option<u32> {
        let p = self.players.get(&id)?;
        let kind = p.bag.iter().find(|i| i.id == item)?.kind;
        self.chest_size(kind).map(|_| kind)
    }

    /// Opens bag chest `chest` for player `id` (using it).
    pub(crate) fn open_chest(&mut self, id: EntityId, chest: u32) {
        let Some(p) = self.players.get(&id) else {
            return;
        };
        if p.booth.is_some() {
            self.message(id, "You can't open a chest while vending.");
            return;
        }
        if self.trade_index(id).is_some() {
            self.message(id, "You can't open a chest while trading.");
            return;
        }
        self.events.push(Event::ChestOpened { player: id, chest });
    }

    /// Puts bag item `item` in bag chest `chest`.
    pub fn chest_put(&mut self, id: EntityId, chest: u32, item: u32) {
        let Some(kind) = self.chest_in_bag(id, chest) else {
            return;
        };
        let size = self.chest_size(kind).unwrap_or(0) as usize;
        let is_chest = |s: &Self, i: &crate::Item| s.chest_size(i.kind).is_some();
        let Some(p) = self.players.get(&id) else {
            return;
        };
        let Some(at) = p.bag.iter().position(|i| i.id == item) else {
            return;
        };
        if is_chest(self, &p.bag[at]) {
            self.message(id, "A chest can't be put in a chest.");
            return;
        }
        if p.chests.get(&chest).map_or(0, Vec::len) >= size {
            self.message(id, "The chest is full.");
            return;
        }
        let p = self.players.get_mut(&id).expect("checked above");
        let it = p.bag.remove(at);
        p.chests.entry(chest).or_default().push(it);
        self.events.push(Event::ChestChanged { player: id, chest });
    }

    /// Takes item `item` out of bag chest `chest` into the bag.
    pub fn chest_take(&mut self, id: EntityId, chest: u32, item: u32) {
        if self.chest_in_bag(id, chest).is_none() {
            return;
        }
        let bag_size = self.bag_size();
        let p = self.players.get_mut(&id).expect("its chest is in its bag");
        if p.bag.len() >= bag_size {
            self.message(id, "Your bag is full.");
            return;
        }
        let Some(kept) = p.chests.get_mut(&chest) else {
            return;
        };
        let Some(at) = kept.iter().position(|i| i.id == item) else {
            return;
        };
        let it = kept.remove(at);
        if kept.is_empty() {
            p.chests.remove(&chest);
        }
        p.bag.push(it);
        self.events.push(Event::ChestChanged { player: id, chest });
    }
}
