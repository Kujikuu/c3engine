//! Booths: a player at a booth spot (an NPC of a booth kind, `gameplay.booth_spots`) sets
//! out items from its bag at its own prices; others who come to the spot see them and buy.
//! A booth closes when its owner closes it, moves or leaves.

use crate::event::Event;
use crate::sim::Sim;
use crate::{EntityId, Item};
use serde::{Deserialize, Serialize};

/// The most wares a booth shows (TQ's booth pages of six, up to 20).
pub const MAX_WARES: usize = 20;

/// A player's booth: the spot it stands at, its wares and what it cries out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Booth {
    pub spot: u32,
    pub wares: Vec<Ware>,
    pub cry: String,
}

/// An item set out, at its price in a currency (empty: the money).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ware {
    pub item: Item,
    pub price: u64,
    #[serde(default)]
    pub currency: String,
}

/// What a player does about booths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoothAction {
    /// Open a booth at spot NPC `0` (free, near).
    Open(u32),
    /// Set out bag item `item` at `price` in `currency`.
    Add {
        item: u32,
        price: u64,
        #[serde(default)]
        currency: String,
    },
    /// Take item `0` back off the booth.
    Remove(u32),
    /// What the booth cries out.
    Cry(String),
    Close,
    /// Buy item `item` from `owner`'s booth.
    Buy {
        owner: EntityId,
        item: u32,
    },
}

impl Sim {
    /// The booth spot NPC `npc` talked to: its owner's booth shown, or (free) an offer to
    /// open one there.
    pub(crate) fn booth_spot(&mut self, id: EntityId, npc: u32) {
        let owner =
            (self.players.values()).find(|p| p.booth.as_ref().is_some_and(|b| b.spot == npc));
        match owner.map(|p| p.id) {
            Some(owner) => self.events.push(Event::BoothShown { player: id, owner }),
            None => self.events.push(Event::BoothOffered {
                player: id,
                spot: npc,
            }),
        }
    }

    pub(crate) fn booth_action(&mut self, id: EntityId, action: BoothAction) {
        match action {
            BoothAction::Open(spot) => self.open_booth(id, spot),
            BoothAction::Add {
                item,
                price,
                currency,
            } => {
                let Some(p) = self.players.get_mut(&id) else {
                    return;
                };
                let Some(found) = p.bag.iter().find(|i| i.id == item).cloned() else {
                    return;
                };
                let Some(booth) = p.booth.as_mut() else {
                    return;
                };
                if price == 0 || booth.wares.len() >= MAX_WARES {
                    return;
                }
                if booth.wares.iter().any(|w| w.item.id == item) {
                    return;
                }
                booth.wares.push(Ware {
                    item: found,
                    price,
                    currency,
                });
                self.events.push(Event::BoothChanged { owner: id });
            }
            BoothAction::Remove(item) => {
                if let Some(b) = self.players.get_mut(&id).and_then(|p| p.booth.as_mut()) {
                    b.wares.retain(|w| w.item.id != item);
                    self.events.push(Event::BoothChanged { owner: id });
                }
            }
            BoothAction::Cry(text) => {
                if let Some(b) = self.players.get_mut(&id).and_then(|p| p.booth.as_mut()) {
                    b.cry = text.chars().take(64).collect();
                    self.events.push(Event::BoothChanged { owner: id });
                }
            }
            BoothAction::Close => self.close_booth(id),
            BoothAction::Buy { owner, item } => self.buy_from_booth(id, owner, item),
        }
    }

    fn open_booth(&mut self, id: EntityId, spot: u32) {
        let Some(n) = self.npcs.iter().find(|n| n.id == spot) else {
            return;
        };
        if !self.settings.booth_spots.contains(&n.kind) {
            return;
        }
        let cell = n.cell;
        if (self.players.values()).any(|p| p.booth.as_ref().is_some_and(|b| b.spot == spot)) {
            return self.message(id, "Someone has a booth there already.");
        }
        let Some(from) = self
            .players
            .get(&id)
            .filter(|p| p.is_alive())
            .map(|p| p.cell)
        else {
            return;
        };
        // The owner stands beside its spot, sitting.
        let at = self.beside(cell, from);
        self.place(id, at);
        if let Some(p) = self.players.get_mut(&id) {
            p.booth = Some(Booth {
                spot,
                ..Booth::default()
            });
        }
        self.events.push(Event::BoothChanged { owner: id });
        self.events.push(Event::BoothShown {
            player: id,
            owner: id,
        });
    }

    /// Closes player `id`'s booth, if it has one (its owner closed it, moved or left).
    pub(crate) fn close_booth(&mut self, id: EntityId) {
        let had = self.players.get_mut(&id).and_then(|p| p.booth.take());
        if had.is_some() {
            self.events.push(Event::BoothChanged { owner: id });
        }
    }

    fn buy_from_booth(&mut self, buyer: EntityId, owner: EntityId, item: u32) {
        let ware = (self.players.get(&owner))
            .and_then(|p| p.booth.as_ref())
            .and_then(|b| b.wares.iter().find(|w| w.item.id == item))
            .cloned();
        let Some(ware) = ware else {
            return self.message(buyer, "That item is gone.");
        };
        let Some(b) = self.players.get(&buyer) else {
            return;
        };
        if buyer == owner {
            return;
        }
        if self.funds(b, &ware.currency) < ware.price {
            let name = self.currency_name(&ware.currency).to_owned();
            return self.message(buyer, format!("You don't have enough {name}."));
        }
        if b.bag.len() >= self.bag_size() {
            return self.message(buyer, "Your bag is full.");
        }
        // The item leaves the owner's bag (it must still be there).
        let Some(o) = self.players.get_mut(&owner) else {
            return;
        };
        let Some(slot) = o.bag.iter().position(|i| i.id == item) else {
            if let Some(booth) = o.booth.as_mut() {
                booth.wares.retain(|w| w.item.id != item);
            }
            return self.message(buyer, "That item is gone.");
        };
        let sold = o.bag.remove(slot);
        if let Some(booth) = o.booth.as_mut() {
            booth.wares.retain(|w| w.item.id != item);
        }
        let pay = |p: &mut crate::Player, amount: u64, add: bool| match ware.currency.is_empty() {
            true if add => p.money = p.money.saturating_add(amount),
            true => p.money -= amount,
            false => {
                let purse = p.wallet.entry(ware.currency.clone()).or_default();
                *purse = if add {
                    purse.saturating_add(amount)
                } else {
                    *purse - amount
                };
            }
        };
        pay(o, ware.price, true);
        let b = self.players.get_mut(&buyer).expect("checked above");
        pay(b, ware.price, false);
        b.bag.push(sold);
        self.events.push(Event::BoothChanged { owner });
        let name = self
            .rules
            .item(ware.item.kind)
            .map_or_else(String::new, |t| t.name.clone());
        self.message(buyer, format!("You bought {name} for {}.", ware.price));
        self.message(owner, format!("Your {name} sold for {}.", ware.price));
    }
}
