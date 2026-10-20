//! Protected items (TQ's item protection, tq_re spec systems/business.md §3): the game's rules
//! name a player's items that may not be let go of: not set out in a booth, dropped, sold,
//! worked as a minor part, or traded but to a formal trade partner (or with a GM). A merchant
//! account protects nothing.

use crate::EntityId;
use crate::booths::BoothAction;
use crate::input::Input;
use crate::sim::Sim;

/// A player's merchant state: none, becoming one on a day (YYYYMMDD), or one.
pub const MERCHANT: u32 = 255;

const NOT_DROPPED: &str = "This item cannot be dropped.";
const NOT_SOLD: &str = "This item cannot be sold.";
const NOT_TRADED: &str = "This item cannot be traded!";

impl Sim {
    /// Whether player `id`'s item `item` is protected (the game's rules say so and it is no
    /// merchant).
    pub fn item_protected(&self, id: EntityId, item: u32) -> bool {
        let Some(p) = self.players.get(&id).filter(|p| p.merchant != MERCHANT) else {
            return false;
        };
        let found = (p.bag.iter()).chain(&p.equipment).find(|i| i.id == item);
        found.is_some_and(|i| self.rules.protected(self, p, i))
    }

    /// Whether players `a` and `b` are formal trade partners (past their trial).
    pub fn formal_partners(&self, a: &str, b: &str) -> bool {
        let now = chrono::Utc::now().timestamp();
        let trial = self.settings.partner_probation_secs;
        (self.world_state().partners_of(a).iter()).any(|(n, since)| n == b && since + trial <= now)
    }

    /// Whether `input` lets go of a protected item it may not: refused, with the words to
    /// say (none: refused silently, as TQ's booth and work refusals are).
    pub(crate) fn protected_refusal(
        &self,
        id: EntityId,
        input: &Input,
    ) -> Option<Option<&'static str>> {
        let held = |item: u32| self.item_protected(id, item);
        match input {
            Input::Drop(item) if held(*item) => Some(Some(NOT_DROPPED)),
            Input::Sell { item, .. } if held(*item) => Some(Some(NOT_SOLD)),
            Input::Booth(BoothAction::Add { item, .. }) if held(*item) => Some(None),
            Input::TradeItem(item) if held(*item) => {
                let p = self.players.get(&id)?;
                let other = self.trade_partner(id).and_then(|o| self.players.get(&o))?;
                let gm = self.settings.is_gm(&p.name) || self.settings.is_gm(&other.name);
                let formal = self.formal_partners(&p.name, &other.name);
                (!gm && !formal).then_some(Some(NOT_TRADED))
            }
            Input::Combine { kind, items, .. } => (items.iter().enumerate())
                .any(|(at, item)| self.rules.protects_work(*kind, at) && held(*item))
                .then_some(None),
            _ => None,
        }
    }
}
