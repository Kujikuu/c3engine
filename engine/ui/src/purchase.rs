//! Buying several of an item at once in a shop priced in another currency than the game's
//! money (TQ's CP shops: CDlgItemPurchase5065 and its limit message CDlgBuyMessage5065): the
//! amount asked, held to the bag's room and the funds, the cost confirmed, then bought one by
//! one.

use crate::GameView;
use c3_game::Player;

/// An item being bought: the shop, the item kind and the step shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Purchase {
    pub npc: u32,
    pub kind: u32,
    pub step: Step,
}

/// The amount asked, the most told (more was asked), or `n` to be bought confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Amount,
    Limit,
    Confirm(u32),
}

impl Step {
    /// The step's number, as the controls show them (0 amount, 1 limit, 2 confirm).
    pub fn number(self) -> u8 {
        match self {
            Self::Amount => 0,
            Self::Limit => 1,
            Self::Confirm(_) => 2,
        }
    }
}

/// How many the hero may buy: the price of one, the most (the bag's room and the funds), and
/// whether the funds are what hold it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub price: u64,
    pub most: u32,
    pub by_funds: bool,
}

/// The currency NPC `npc` sells item `kind` in and its price there, when that is not the
/// game's money (a silver buy needs no amount).
pub fn priced(view: &GameView, npc: u32, kind: u32) -> Option<(String, u64)> {
    let rules = view.sim.rules();
    let item = rules.item(kind)?;
    let currency =
        (rules.shop_currency(npc, kind)).map_or_else(|| item.currency.clone(), str::to_owned);
    if currency.is_empty() {
        return None;
    }
    let price = item.price_in(&currency).filter(|p| *p > 0)?;
    Some((currency, price))
}

/// What the hero may buy of item `kind` at NPC `npc` (CDlgItemPurchase5065::OpenByType).
pub fn limits(view: &GameView, player: &Player, npc: u32, kind: u32) -> Option<Limits> {
    let (currency, price) = priced(view, npc, kind)?;
    let room = (view.sim.settings().bag_size as usize).saturating_sub(player.bag.len());
    let room = u32::try_from(room).unwrap_or(u32::MAX);
    let funds = player.wallet.get(&currency).copied().unwrap_or(0);
    let afford = u32::try_from(funds / price).unwrap_or(u32::MAX);
    Some(Limits {
        price,
        most: room.min(afford),
        by_funds: afford <= room,
    })
}
