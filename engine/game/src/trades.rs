//! Trades between two players on a map: one asks, the other asks back to open it; each
//! offers bag items and once some money; when both accept, the goods change hands (when
//! both bags have room). Walking apart, dying or leaving the map closes it.

use crate::EntityId;
use crate::entity::Item;
use crate::event::Event;
use crate::sim::Sim;
use serde::{Deserialize, Serialize};

/// Most items one side offers.
const MOST_ITEMS: usize = 20;
/// The farthest apart (cells) two traders stay.
const TRADE_REACH: i32 = 18;

/// What a trader is told about its trade.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeNews {
    /// `partner` asks to trade.
    Asked,
    /// The trade with `partner` is open.
    Opened,
    /// The partner offers this item, or this much money.
    PartnerItem(Item),
    PartnerMoney(u64),
    /// The player's own money offer, as it now stands.
    OwnMoney(u64),
    /// The partner accepted.
    PartnerAccepted,
    /// An item that couldn't be offered.
    Refused(u32),
    /// Done: the goods changed hands; or closed without a trade.
    Done,
    Closed,
}

/// One side of a trade.
#[derive(Debug, Clone)]
struct Side {
    player: EntityId,
    items: Vec<u32>,
    money: u64,
    accepted: bool,
}

impl Side {
    fn of(player: EntityId) -> Self {
        Self {
            player,
            items: Vec::new(),
            money: 0,
            accepted: false,
        }
    }
}

/// A trade in progress.
#[derive(Debug, Clone)]
pub(crate) struct Trade {
    sides: [Side; 2],
}

impl Trade {
    fn side_of(&mut self, id: EntityId) -> Option<(&mut Side, EntityId)> {
        let [a, b] = &mut self.sides;
        if a.player == id {
            let partner = b.player;
            Some((a, partner))
        } else if b.player == id {
            let partner = a.player;
            Some((b, partner))
        } else {
            None
        }
    }
}

impl Sim {
    fn tell_trade(&mut self, player: EntityId, partner: EntityId, news: TradeNews) {
        self.events.push(Event::Trade {
            player,
            partner,
            news,
        });
    }

    /// Who player `id` is trading with.
    pub fn trade_partner(&self, id: EntityId) -> Option<EntityId> {
        let trade = &self.trades[self.trade_index(id)?];
        (trade.sides.iter())
            .find(|s| s.player != id)
            .map(|s| s.player)
    }

    pub(crate) fn trade_index(&self, id: EntityId) -> Option<usize> {
        (self.trades.iter()).position(|t| t.sides.iter().any(|s| s.player == id))
    }

    /// Whether `a` and `b` are both alive here and near enough to trade.
    fn can_trade(&self, a: EntityId, b: EntityId) -> bool {
        match (self.players.get(&a), self.players.get(&b)) {
            (Some(p), Some(q)) => {
                a != b
                    && p.is_alive()
                    && q.is_alive()
                    && crate::distance(p.cell, q.cell) <= TRADE_REACH
            }
            _ => false,
        }
    }

    /// Player `id` asks `target` to trade; asked back, the trade opens.
    pub fn ask_trade(&mut self, id: EntityId, target: EntityId) {
        if !self.can_trade(id, target) || self.trade_index(target).is_some() {
            return;
        }
        self.close_trade(id);
        if self.trade_asks.get(&target) == Some(&id) {
            self.trade_asks.remove(&target);
            self.trades.push(Trade {
                sides: [Side::of(target), Side::of(id)],
            });
            self.tell_trade(id, target, TradeNews::Opened);
            self.tell_trade(target, id, TradeNews::Opened);
        } else {
            self.trade_asks.insert(id, target);
            self.tell_trade(target, id, TradeNews::Asked);
        }
    }

    /// Player `id` offers bag item `item`.
    pub fn offer_item(&mut self, id: EntityId, item: u32) {
        let Some(at) = self.trade_index(id) else {
            return;
        };
        let held = (self.players.get(&id))
            .and_then(|p| p.bag.iter().find(|i| i.id == item))
            .copied();
        let (side, partner) = self.trades[at].side_of(id).expect("in it");
        let fits = !side.accepted && side.items.len() < MOST_ITEMS && !side.items.contains(&item);
        match held {
            Some(it) if fits => {
                side.items.push(item);
                self.tell_trade(partner, id, TradeNews::PartnerItem(it));
            }
            _ => self.tell_trade(id, partner, TradeNews::Refused(item)),
        }
    }

    /// Player `id` offers `amount` of money (once).
    pub fn offer_money(&mut self, id: EntityId, amount: u64) {
        let Some(at) = self.trade_index(id) else {
            return;
        };
        let money = self.players.get(&id).map_or(0, |p| p.money);
        let (side, partner) = self.trades[at].side_of(id).expect("in it");
        if side.accepted || side.money != 0 || amount > money {
            return;
        }
        side.money = amount;
        self.tell_trade(partner, id, TradeNews::PartnerMoney(amount));
        self.tell_trade(id, partner, TradeNews::OwnMoney(amount));
    }

    /// Player `id` accepts the trade; both having accepted, it is made.
    pub fn accept_trade(&mut self, id: EntityId) {
        let Some(at) = self.trade_index(id) else {
            return;
        };
        let (side, partner) = self.trades[at].side_of(id).expect("in it");
        side.accepted = true;
        self.tell_trade(partner, id, TradeNews::PartnerAccepted);
        if self.trades[at].sides.iter().all(|s| s.accepted) {
            let trade = self.trades.remove(at);
            let done = self.make_trade(&trade);
            let [a, b] = [trade.sides[0].player, trade.sides[1].player];
            let news = if done {
                TradeNews::Done
            } else {
                TradeNews::Closed
            };
            self.tell_trade(a, b, news.clone());
            self.tell_trade(b, a, news);
        }
    }

    /// Gives each side the other's goods; false (nothing moves) when they can't.
    fn make_trade(&mut self, trade: &Trade) -> bool {
        let [a, b] = &trade.sides;
        if !self.can_trade(a.player, b.player) {
            return false;
        }
        let bag_size = self.bag_size();
        let offered = |s: &Sim, side: &Side| -> Option<Vec<Item>> {
            let p = s.players.get(&side.player)?;
            (side.money <= p.money).then_some(())?;
            (side.items.iter())
                .map(|id| p.bag.iter().find(|i| i.id == *id).copied())
                .collect()
        };
        let (Some(from_a), Some(from_b)) = (offered(self, a), offered(self, b)) else {
            return false;
        };
        // Room in each bag for what comes in, less what goes out.
        let room = |s: &Sim, side: &Side, gets: usize| {
            let p = &s.players[&side.player];
            p.bag.len() - side.items.len() + gets <= bag_size
        };
        if !room(self, a, from_b.len()) || !room(self, b, from_a.len()) {
            return false;
        }
        for (side, gives, gets, other) in [(a, &from_a, &from_b, b), (b, &from_b, &from_a, a)] {
            let p = self.players.get_mut(&side.player).expect("checked");
            p.bag.retain(|i| !gives.iter().any(|g| g.id == i.id));
            p.bag.extend(gets.iter().copied());
            p.money = p.money - side.money + other.money;
        }
        true
    }

    /// Closes player `id`'s trade (and its ask).
    pub fn close_trade(&mut self, id: EntityId) {
        self.trade_asks.remove(&id);
        if let Some(at) = self.trade_index(id) {
            let trade = self.trades.remove(at);
            let [a, b] = [trade.sides[0].player, trade.sides[1].player];
            self.tell_trade(a, b, TradeNews::Closed);
            self.tell_trade(b, a, TradeNews::Closed);
        }
    }

    /// Trades whose traders walked apart, died or left close.
    pub(crate) fn tick_trades(&mut self) {
        let broken: Vec<EntityId> = (self.trades.iter())
            .filter(|t| !self.can_trade(t.sides[0].player, t.sides[1].player))
            .map(|t| t.sides[0].player)
            .collect();
        for id in broken {
            self.close_trade(id);
        }
        self.trade_asks
            .retain(|from, to| self.players.contains_key(from) && self.players.contains_key(to));
    }
}
