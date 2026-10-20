//! Nobility (TQ's Peerage.dll, adapters spec §2): players from the settings' level donate
//! money (or another currency at the settings' rate) for a noble rank. The biggest donors
//! hold the top places' ranks (the settings' `by_place`); the rest of the board and everyone
//! off it rank by what they gave (`by_value`). A player who rises is announced to all.

use crate::EntityId;
use crate::sim::Sim;
use crate::world_state::WorldState;
use c3_core::settings::NobilitySettings;
use c3_script::WorldChange;
use serde::{Deserialize, Serialize};

/// What a player gave and its level when it gave (ties go to the higher level).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Donation {
    pub total: u64,
    pub level: u32,
}

/// One place on the nobility board: who, what it gave, its rank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Noble {
    pub name: String,
    pub total: u64,
    pub rank: u32,
}

/// The rank `total` alone earns (TQ's ByValue).
pub fn rank_by_value(settings: &NobilitySettings, total: u64) -> u32 {
    (settings.by_value.iter())
        .filter(|[value, _]| total >= *value)
        .map(|[_, rank]| *rank as u32)
        .max()
        .unwrap_or(0)
}

impl WorldState {
    /// The nobility board, the biggest donor first (TQ's sort: donation, then level, then
    /// name), at most the settings' places.
    pub fn nobles(&self, settings: &NobilitySettings) -> Vec<Noble> {
        let mut list: Vec<(&String, &Donation)> = self.donations.iter().collect();
        list.sort_by(|a, b| {
            (b.1.total.cmp(&a.1.total))
                .then(b.1.level.cmp(&a.1.level))
                .then(a.0.cmp(b.0))
        });
        (list.into_iter().take(settings.board_size).enumerate())
            .map(|(place, (name, d))| {
                let by_place = (settings.by_place.iter())
                    .find(|[places, _]| place < *places as usize)
                    .map(|[_, rank]| *rank);
                Noble {
                    name: name.clone(),
                    total: d.total,
                    rank: by_place.unwrap_or_else(|| rank_by_value(settings, d.total)),
                }
            })
            .collect()
    }

    /// Player `name`'s rank and place on the board (`None` when off it).
    pub fn noble_rank(&self, settings: &NobilitySettings, name: &str) -> (u32, Option<usize>) {
        let nobles = self.nobles(settings);
        match nobles.iter().position(|n| n.name == name) {
            Some(place) => (nobles[place].rank, Some(place)),
            None => {
                let total = self.donations.get(name).map_or(0, |d| d.total);
                (rank_by_value(settings, total), None)
            }
        }
    }

    /// What a player who gave `mine` must give more to reach `rank` (TQ's query price: past
    /// the first holder of that rank, at least the settings' least donation).
    pub fn noble_price(&self, settings: &NobilitySettings, rank: u32, mine: u64) -> u64 {
        let holder = self.nobles(settings).into_iter().find(|n| n.rank == rank);
        let least = settings.min_donation;
        let by_value = (settings.by_value.iter()).find(|[_, r]| *r as u32 == rank);
        match (holder, by_value) {
            (Some(h), _) => (h.total + 1).saturating_sub(mine).max(least),
            (None, Some([value, _])) => value.saturating_sub(mine).max(least),
            (None, None) => least,
        }
    }
}

impl Sim {
    /// Player `id` gives `amount` of money for nobility (TQ's MsgPeerage 1): from the
    /// settings' level and least amount, paid in money or else the other currency.
    pub(crate) fn donate(&mut self, id: EntityId, amount: u64) {
        let n = self.settings.nobility.clone();
        if n.min_level == 0 {
            return;
        }
        let Some(p) = self.players.get(&id) else {
            return;
        };
        if p.level < n.min_level {
            let text = format!("You need to be level {} or above to donate.", n.min_level);
            return self.message(id, text);
        }
        if amount < n.min_donation {
            return self.message(id, "You need to donate at least 3,000,000 silver.");
        }
        if n.max_donation > 0 && amount > n.max_donation {
            return self.message(id, "You can donate up to 1,000,000,000 silver at a time.");
        }
        let (name, level) = (p.name.clone(), p.level);
        let in_currency = amount / n.rate.max(1);
        let p = self.players.get_mut(&id).expect("checked");
        if p.money >= amount {
            p.money -= amount;
        } else {
            let purse = p.wallet.entry(n.currency.clone()).or_default();
            if n.currency.is_empty() || *purse < in_currency {
                return self.message(id, "You don`t have enough Silver/CPs.");
            }
            *purse -= in_currency;
        }
        let (before, _) = self.world.noble_rank(&n, &name);
        let total = self.world.donations.get(&name).map_or(0, |d| d.total) + amount;
        self.change_world(WorldChange::Donation {
            name: name.clone(),
            total,
            level: i64::from(level),
        });
        let (after, place) = self.world.noble_rank(&n, &name);
        let mut told = format!("You have donated {amount} silver. Your total donation is {total}");
        if let Some(place) = place {
            told += &format!(", now you rank {}th in the Nobility Rank Board.", place + 1);
        } else {
            told.push('.');
        }
        self.message(id, told);
        // A rise is announced to everyone here.
        if after > before
            && let Some((_, notice)) = n.notices.iter().find(|(rank, _)| *rank == after)
        {
            let text = notice.replace("%s", &name);
            let players: Vec<EntityId> = self.players.keys().copied().collect();
            for player in players {
                let text = text.clone();
                self.events.push(crate::Event::Notice { player, text });
            }
        }
    }
}
