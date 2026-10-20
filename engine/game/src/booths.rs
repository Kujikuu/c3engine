//! Booths: a player at a booth spot (an NPC of a booth kind, `gameplay.booth_spots`) sets
//! out items from its bag at its own prices; others who come to the spot see them and buy.
//! As TQ's booths (booth_trade spec §1): the owner stands two cells out from the spot along
//! its facing, the booth one cell further; wares stay in the owner's bag until sold and drop
//! off the booth when they leave it; the booth closes when its owner closes it, moves or
//! leaves.

use crate::event::Event;
use crate::sim::Sim;
use crate::{EntityId, Item};
use c3_scene::CellPos;
use serde::{Deserialize, Serialize};

/// The most wares a booth shows (TQ's CBooth::AddItem).
pub const MAX_WARES: usize = 20;

/// The longest cry kept (CDlgHawk's edit limit).
const MAX_CRY: usize = 64;

/// A player's booth: the spot it stands at, where it lies and faces, its wares and what it
/// cries out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Booth {
    pub spot: u32,
    #[serde(default)]
    pub cell: CellPos,
    #[serde(default)]
    pub direction: u8,
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
    /// Open a booth at spot NPC `0`.
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
        let owner = (self.players.values())
            .find(|p| p.booth.as_ref().is_some_and(|b| b.spot == npc))
            .map(|p| p.id);
        self.events.push(match owner {
            Some(owner) => Event::BoothShown { player: id, owner },
            None => Event::BoothOffered {
                player: id,
                spot: npc,
            },
        });
    }

    pub(crate) fn booth_action(&mut self, id: EntityId, action: BoothAction) {
        match action {
            BoothAction::Open(spot) => self.open_booth(id, spot),
            BoothAction::Add {
                item,
                price,
                currency,
            } => self.add_ware(id, item, price, currency),
            BoothAction::Remove(item) => {
                if let Some(b) = self.players.get_mut(&id).and_then(|p| p.booth.as_mut()) {
                    b.wares.retain(|w| w.item.id != item);
                    self.events.push(Event::BoothChanged { owner: id });
                }
            }
            BoothAction::Cry(text) => {
                if let Some(b) = self.players.get_mut(&id).and_then(|p| p.booth.as_mut()) {
                    b.cry = text.chars().take(MAX_CRY).collect();
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
        // The owner stands two cells out from the spot along its facing, its booth one more
        // (TQ's CHero::CanBooth and CBoothManager::Open: the flag faces 6, the booth at x + 1).
        let direction = n.direction % 8;
        let (dx, dy) = c3_scene::path::DIRECTIONS[usize::from(direction)];
        let stand = (n.cell.0 + 2 * dx, n.cell.1 + 2 * dy);
        let cell = (stand.0 + dx, stand.1 + dy);
        if (self.players.values()).any(|p| p.booth.as_ref().is_some_and(|b| b.spot == spot)) {
            return self.message(id, "Someone has a booth there already.");
        }
        let can = (self.players.get(&id)).is_some_and(|p| p.is_alive() && p.booth.is_none());
        if !can || !self.walkable(stand) {
            return;
        }
        self.place(id, stand);
        if let Some(p) = self.players.get_mut(&id) {
            p.direction = direction;
            p.booth = Some(Booth {
                spot,
                cell,
                direction,
                ..Booth::default()
            });
        }
        self.events.push(Event::BoothChanged { owner: id });
        self.events.push(Event::BoothShown {
            player: id,
            owner: id,
        });
    }

    /// Bag item `item` set out on player `id`'s booth (TQ's CBooth::AddItem: a price, room,
    /// not there already).
    fn add_ware(&mut self, id: EntityId, item: u32, price: u64, currency: String) {
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        let Some(found) = p.bag.iter().find(|i| i.id == item).cloned() else {
            return;
        };
        let Some(booth) = p.booth.as_mut() else {
            return;
        };
        let listed = booth.wares.iter().any(|w| w.item.id == item);
        if price == 0 || booth.wares.len() >= MAX_WARES || listed {
            return;
        }
        booth.wares.push(Ware {
            item: found,
            price,
            currency,
        });
        self.events.push(Event::BoothChanged { owner: id });
    }

    /// Closes player `id`'s booth, if it has one (its owner closed it, moved or left).
    pub(crate) fn close_booth(&mut self, id: EntityId) {
        let had = self.players.get_mut(&id).and_then(|p| p.booth.take());
        if had.is_some() {
            self.events.push(Event::BoothChanged { owner: id });
        }
    }

    /// Wares whose items left their owners' bags (used, dropped, traded) drop off the booths
    /// (TQ's DelItem on any bag removal).
    pub(crate) fn prune_booths(&mut self) {
        let mut changed = Vec::new();
        for p in self.players.values_mut() {
            let bag = &p.bag;
            let Some(booth) = p.booth.as_mut() else {
                continue;
            };
            let before = booth.wares.len();
            booth
                .wares
                .retain(|w| bag.iter().any(|i| i.id == w.item.id));
            if booth.wares.len() != before {
                changed.push(p.id);
            }
        }
        for owner in changed {
            self.events.push(Event::BoothChanged { owner });
        }
    }

    /// Player `buyer` buys item `item` off `owner`'s booth (TQ's CBooth::Buy): the item
    /// changes bags, the price changes purses, the owner is told.
    fn buy_from_booth(&mut self, buyer: EntityId, owner: EntityId, item: u32) {
        let ware = (self.players.get(&owner))
            .and_then(|p| p.booth.as_ref())
            .and_then(|b| b.wares.iter().find(|w| w.item.id == item))
            .cloned();
        let Some(ware) = ware else {
            return self.message(buyer, "Failed to trade.");
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
            return self.message(buyer, "Your inventory is full!");
        }
        let buyer_name = b.name.clone();
        let Some(o) = self.players.get_mut(&owner) else {
            return;
        };
        let Some(slot) = o.bag.iter().position(|i| i.id == item) else {
            return self.message(buyer, "Failed to trade.");
        };
        let sold = o.bag.remove(slot);
        if let Some(booth) = o.booth.as_mut() {
            booth.wares.retain(|w| w.item.id != item);
        }
        let pay = |p: &mut crate::Player, amount: u64, add: bool| {
            let purse = match ware.currency.is_empty() {
                true => &mut p.money,
                false => p.wallet.entry(ware.currency.clone()).or_default(),
            };
            *purse = match add {
                true => purse.saturating_add(amount),
                false => purse.saturating_sub(amount),
            };
        };
        pay(o, ware.price, true);
        let b = self.players.get_mut(&buyer).expect("checked above");
        pay(b, ware.price, false);
        b.bag.push(sold);
        self.events.push(Event::BoothChanged { owner });
        let item_name =
            (self.rules.item(ware.item.kind)).map_or_else(String::new, |t| t.name.clone());
        let coin = self.currency_name(&ware.currency).to_owned();
        let text = format!(
            "{buyer_name} paid {} {coin} to buy your {item_name}.",
            ware.price
        );
        self.message(owner, text);
    }
}
