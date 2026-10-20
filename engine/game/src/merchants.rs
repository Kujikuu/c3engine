//! Merchant accounts (TQ's business state, tq_re spec systems/business.md §4): a player applies
//! and, after `merchant_days` (at once on a map of `merchant_now_maps`), becomes a merchant,
//! whose items are never protected; either may be called off. The state is
//! [`Player::merchant`](crate::Player): 0, the day it is granted (YYYYMMDD), or `MERCHANT`.

use crate::EntityId;
use crate::event::Event;
use crate::protection::MERCHANT;
use crate::sim::Sim;
use serde::{Deserialize, Serialize};

/// What a player asks of its merchant account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MerchantAction {
    Apply,
    /// Calls off the application, or the account.
    Cancel,
}

/// What a player is told of its merchant account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MerchantNews {
    /// It may apply (told at login where accounts are granted at once).
    Offered,
    /// Applied: granted on day `until` (YYYYMMDD).
    Applied {
        until: u32,
    },
    /// Waiting (told at login): granted in `days` days.
    Pending {
        days: u32,
    },
    /// It became a merchant just now.
    Became,
    /// It is a merchant (told at login).
    Is,
    Cancelled,
}

/// Today as YYYYMMDD, and the day `days` after it.
pub(crate) fn day(days: u32) -> u32 {
    use chrono::Datelike;
    let date = chrono::Local::now().date_naive() + chrono::Days::new(u64::from(days));
    (date.year().max(0) as u32) * 10_000 + date.month() * 100 + date.day()
}

impl Sim {
    fn tell_merchant(&mut self, player: EntityId, news: MerchantNews) {
        self.events.push(Event::Merchant { player, news });
    }

    /// Whether this map grants merchant accounts at once.
    fn grants_now(&self) -> bool {
        let maps = &self.settings.merchant_now_maps;
        self.map_ids.iter().any(|m| maps.contains(m))
    }

    pub(crate) fn merchant_action(&mut self, id: EntityId, action: MerchantAction) {
        let days = self.settings.merchant_days;
        let now = self.grants_now();
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        match action {
            // As TQ: applying again starts the wait again.
            MerchantAction::Apply if days > 0 => {
                p.merchant = if now { MERCHANT } else { day(days) };
                let news = match now {
                    true => MerchantNews::Became,
                    false => MerchantNews::Applied { until: p.merchant },
                };
                self.tell_merchant(id, news);
            }
            MerchantAction::Apply => {}
            MerchantAction::Cancel if p.merchant != 0 => {
                p.merchant = 0;
                self.tell_merchant(id, MerchantNews::Cancelled);
            }
            MerchantAction::Cancel => {}
        }
    }

    /// Player `id` comes in: told it may apply, how long it waits, or that it is a merchant
    /// (one whose day has come becomes one).
    pub(crate) fn merchant_login(&mut self, id: EntityId) {
        let days = self.settings.merchant_days;
        if days == 0 {
            return;
        }
        let now = self.grants_now();
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        let today = day(0);
        let news = match p.merchant {
            0 if now => MerchantNews::Offered,
            0 => return,
            MERCHANT => MerchantNews::Is,
            until if until <= today => {
                p.merchant = MERCHANT;
                MerchantNews::Is
            }
            until => match (1..=days).find(|d| day(*d) == until) {
                Some(days) => MerchantNews::Pending { days },
                None => return,
            },
        };
        self.tell_merchant(id, news);
    }
}
