//! Item locks (TQ's equipment lock, MsgEquipLock 2048): a worn-kind item a player locks can't
//! be dropped, sold, set out in a booth or traded but to a trade partner. Unlocking takes the
//! settings' `item_lock_days`: the item stays locked until that day (`Item::lock` holds it as
//! YYYYMMDD), then is free; locking it again before then calls the unlocking off.

use crate::entity::Item;
use crate::event::Event;
use crate::input::Input;
use crate::sim::Sim;
use crate::{BoothAction, EntityId};

/// `Item::lock` of a locked item (other than 0, an unlocking one's day).
pub const LOCKED: u32 = 1;

/// Today as YYYYMMDD, and the day `days` after it.
fn day(days: u32) -> u32 {
    use chrono::Datelike;
    let date = chrono::Local::now().date_naive() + chrono::Days::new(u64::from(days));
    (date.year().max(0) as u32) * 10_000 + date.month() * 100 + date.day()
}

impl Sim {
    /// Player `id`'s item `item` wherever it carries it (bag or worn).
    fn carried_mut(&mut self, id: EntityId, item: u32) -> Option<&mut Item> {
        let p = self.players.get_mut(&id)?;
        (p.bag.iter_mut().chain(p.equipment.iter_mut())).find(|i| i.id == item)
    }

    /// Locks item `item` (a kind that is worn), or calls its unlocking off.
    pub fn lock_item(&mut self, id: EntityId, item: u32) {
        if self.settings.item_lock_days == 0 {
            return;
        }
        let worn_kind = |s: &Self, kind| s.rules.item(kind).is_some_and(|t| t.slot.is_some());
        let Some(kind) = self.carried_mut(id, item).map(|i| i.kind) else {
            return;
        };
        if !worn_kind(self, kind) {
            self.message(id, "Only equipment can be locked!");
            return;
        }
        let it = self.carried_mut(id, item).expect("carried");
        if it.lock == LOCKED {
            return;
        }
        it.lock = LOCKED;
        self.events.push(Event::ItemLocked { player: id, item });
    }

    /// Starts unlocking locked item `item`: it is free after the settings' days.
    pub fn unlock_item(&mut self, id: EntityId, item: u32) {
        let days = self.settings.item_lock_days;
        let Some(it) = self.carried_mut(id, item) else {
            return;
        };
        let kind = it.kind;
        match it.lock {
            0 => {}
            LOCKED => {
                it.lock = day(days);
                self.events.push(Event::ItemLocked { player: id, item });
            }
            until => {
                let name = self
                    .rules
                    .item(kind)
                    .map_or_else(String::new, |t| t.name.clone());
                let (y, m, d) = (until / 10_000, until / 100 % 100, until % 100);
                self.message(
                    id,
                    format!("{name} will be unlocked on {y},{m},{d} (yy,mm,dd)."),
                );
            }
        }
    }

    /// Frees player `id`'s items whose unlocking day has come.
    pub(crate) fn unlock_due(&mut self, id: EntityId) {
        let today = day(0);
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        let mut freed = Vec::new();
        for it in p.bag.iter_mut().chain(p.equipment.iter_mut()) {
            if it.lock > LOCKED && it.lock <= today {
                it.lock = 0;
                freed.push((it.id, it.kind));
            }
        }
        for (item, kind) in freed {
            let name = self
                .rules
                .item(kind)
                .map_or_else(String::new, |t| t.name.clone());
            self.message(id, format!("{name} is unlocked!"));
            self.events.push(Event::ItemLocked { player: id, item });
        }
    }

    /// Why `input` is refused for a locked item it would let go of (dropped, sold, set out
    /// in a booth, traded to one not a trade partner); none: it goes ahead.
    pub(crate) fn locked_refusal(&self, id: EntityId, input: &Input) -> Option<&'static str> {
        let item = match *input {
            Input::Drop(item) | Input::Sell { item, .. } | Input::TradeItem(item) => item,
            Input::Booth(BoothAction::Add { item, .. }) => item,
            _ => return None,
        };
        let p = self.players.get(&id)?;
        let locked = (p.bag.iter()).any(|i| i.id == item && i.lock != 0);
        if !locked {
            return None;
        }
        if let Input::TradeItem(_) = input {
            let partner = self.trade_partner(id).and_then(|o| self.players.get(&o));
            if partner.is_some_and(|o| self.world_state().are_partners(&p.name, &o.name)) {
                return None;
            }
            return Some("Locked items can only be traded to your trade partners.");
        }
        Some("This item is locked. It can't be traded, discarded or sold.")
    }
}
