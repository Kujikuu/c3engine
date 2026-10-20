//! Dice tables (TQ's sic-bo, CNpcBet; tq_re spec systems/dice.md): an NPC of the settings'
//! kinds keeps a table. Talking to it offers a seat; a seat starts the round when the table is
//! idle. Bets are taken for `bet_seconds`, then closed. Three dice roll `roll_seconds` later,
//! and the next round opens `next_seconds` after that. A chip is one of the settings' sizes, and
//! a player's bets in a round total at most `most`. Each bet pays its stake times its `pays`
//! (stake included) when its rule holds. The table is held in memory only.

use crate::event::Event;
use crate::sim::Sim;
use crate::{EntityId, distance};
use c3_core::settings::DiceWin;
use serde::{Deserialize, Serialize};

/// What a player does at a dice table (NPC `npc`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiceAction {
    Join(u32),
    Leave(u32),
    /// A chip of `money` on bet `bet`.
    Chip {
        npc: u32,
        bet: u8,
        money: u64,
    },
    /// Every bet taken back while betting is open.
    CancelAll(u32),
}

/// What a dice table tells a player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiceNews {
    /// A seat offered (the NPC was talked to).
    Offered,
    /// `name` sat down (the player's own name: it is seated) or left.
    Joined {
        name: String,
    },
    Left {
        name: String,
    },
    /// Betting opened with `seconds` left, or closed.
    Open {
        seconds: u32,
    },
    Closed,
    /// Chips put on (or, `cancel`, taken off) bets: the player's own when `mine`.
    Chips {
        bets: Vec<(u8, u64)>,
        mine: bool,
        cancel: bool,
    },
    /// The dice (to everyone near the table).
    Rolled {
        dice: [u8; 3],
    },
    /// `name` won `amount` on a bet.
    Won {
        name: String,
        amount: u64,
    },
    /// A bet after betting closed.
    Missed,
}

/// Where a table is in its round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Round {
    #[default]
    Idle,
    Betting,
    Closed,
}

/// One table: its round, its players (in the order they sat), its bets (one a player and
/// bet, in the order placed), its countdown (seconds; below 0 once closed) and the last second
/// it stepped.
#[derive(Debug, Clone, Default)]
pub(crate) struct DiceTable {
    round: Round,
    players: Vec<EntityId>,
    bets: Vec<(EntityId, u8, u64)>,
    countdown: i64,
    second: u64,
}

impl Sim {
    fn dice_npc(&self, npc: u32) -> Option<c3_scene::CellPos> {
        let kinds = &self.settings.dice.kinds;
        (self.npcs.iter())
            .find(|n| n.id == npc && kinds.contains(&n.kind))
            .map(|n| n.cell)
    }

    /// Whether NPC `npc` keeps a dice table (talking to it offers a seat).
    pub(crate) fn offer_dice(&mut self, id: EntityId, npc: u32) -> bool {
        if self.dice_npc(npc).is_none() {
            return false;
        }
        self.tell_dice(id, npc, DiceNews::Offered);
        true
    }

    fn tell_dice(&mut self, player: EntityId, npc: u32, news: DiceNews) {
        self.events.push(Event::Dice { player, npc, news });
    }

    /// Tells every player at table `npc` (but `but`).
    fn tell_table(&mut self, npc: u32, but: Option<EntityId>, news: &DiceNews) {
        let players = self
            .dice
            .get(&npc)
            .map(|t| t.players.clone())
            .unwrap_or_default();
        for p in players.into_iter().filter(|p| Some(*p) != but) {
            self.tell_dice(p, npc, news.clone());
        }
    }

    fn near_dice(&self, id: EntityId, npc: u32) -> bool {
        let range = self.settings.view_range;
        match (self.players.get(&id), self.dice_npc(npc)) {
            (Some(p), Some(cell)) => distance(p.cell, cell) <= range,
            _ => false,
        }
    }

    pub(crate) fn dice_action(&mut self, id: EntityId, action: DiceAction) {
        match action {
            DiceAction::Join(npc) if self.near_dice(id, npc) => self.join_dice(id, npc),
            DiceAction::Leave(npc) => self.leave_dice(id, npc),
            DiceAction::Chip { npc, bet, money } if self.near_dice(id, npc) => {
                self.chip_in(id, npc, bet, money);
            }
            DiceAction::CancelAll(npc) if self.near_dice(id, npc) => self.cancel_bets(id, npc),
            _ => {}
        }
    }

    fn join_dice(&mut self, id: EntityId, npc: u32) {
        let Some(name) = self.players.get(&id).map(|p| p.name.clone()) else {
            return;
        };
        let bet_seconds = i64::from(self.settings.dice.bet_seconds);
        let table = self.dice.entry(npc).or_default();
        table.players.retain(|p| *p != id);
        if table.round == Round::Idle {
            table.round = Round::Betting;
            table.countdown = bet_seconds;
            table.bets.clear();
        }
        let round = match table.round {
            Round::Betting => DiceNews::Open {
                seconds: u32::try_from(table.countdown.max(0)).unwrap_or(0),
            },
            _ => DiceNews::Closed,
        };
        let seated: Vec<EntityId> = table.players.iter().rev().copied().collect();
        let bets = table.bets.clone();
        table.players.push(id);
        self.tell_dice(id, npc, DiceNews::Joined { name });
        self.tell_dice(id, npc, round);
        // Who already sits, each with what it bet.
        for p in seated {
            let Some(name) = self.players.get(&p).map(|p| p.name.clone()) else {
                continue;
            };
            self.tell_dice(id, npc, DiceNews::Joined { name });
            let theirs: Vec<(u8, u64)> = (bets.iter())
                .filter(|(who, _, _)| *who == p)
                .map(|(_, bet, money)| (*bet, *money))
                .collect();
            if !theirs.is_empty() {
                let news = DiceNews::Chips {
                    bets: theirs,
                    mine: false,
                    cancel: false,
                };
                self.tell_dice(id, npc, news);
            }
        }
    }

    fn leave_dice(&mut self, id: EntityId, npc: u32) {
        let Some(name) = self.players.get(&id).map(|p| p.name.clone()) else {
            return;
        };
        self.tell_dice(id, npc, DiceNews::Left { name: name.clone() });
        let Some(table) = self.dice.get_mut(&npc).filter(|t| t.players.contains(&id)) else {
            return;
        };
        // Bets come back while betting is open; once closed they stay in the round.
        let mut refunds = Vec::new();
        if table.round == Round::Betting {
            table.bets.retain(|&(who, bet, money)| {
                let keep = who != id;
                if !keep {
                    refunds.push((bet, money));
                }
                keep
            });
        }
        table.players.retain(|p| *p != id);
        if table.players.is_empty() {
            table.round = Round::Idle;
        }
        if let Some(p) = self.players.get_mut(&id) {
            p.money += refunds.iter().map(|(_, m)| m).sum::<u64>();
        }
        if !refunds.is_empty() {
            let news = DiceNews::Chips {
                bets: refunds,
                mine: false,
                cancel: true,
            };
            self.tell_table(npc, Some(id), &news);
        }
        self.tell_table(npc, None, &DiceNews::Left { name });
    }

    fn chip_in(&mut self, id: EntityId, npc: u32, bet: u8, money: u64) {
        let d = &self.settings.dice;
        let fits = usize::from(bet) < d.bets.len() && d.chips.contains(&money);
        let most = d.most;
        let Some(table) = self.dice.get_mut(&npc).filter(|t| t.players.contains(&id)) else {
            return;
        };
        if !fits || table.round == Round::Idle {
            return;
        }
        if table.round == Round::Closed {
            self.tell_dice(id, npc, DiceNews::Missed);
            return;
        }
        let all: u64 = (table.bets.iter())
            .filter(|(who, _, _)| *who == id)
            .map(|b| b.2)
            .sum();
        let held = self.players.get(&id).map_or(0, |p| p.money);
        if held < money || all + money > most {
            return;
        }
        // One record a player and bet, moved to the end with the chip added.
        let before = (table.bets.iter())
            .position(|&(who, b, _)| who == id && b == bet)
            .map(|at| table.bets.remove(at).2);
        table.bets.push((id, bet, before.unwrap_or(0) + money));
        if let Some(p) = self.players.get_mut(&id) {
            p.money -= money;
        }
        let chips = |mine| DiceNews::Chips {
            bets: vec![(bet, money)],
            mine,
            cancel: false,
        };
        self.tell_dice(id, npc, chips(true));
        self.tell_table(npc, Some(id), &chips(false));
    }

    fn cancel_bets(&mut self, id: EntityId, npc: u32) {
        let Some(table) = self.dice.get_mut(&npc).filter(|t| t.players.contains(&id)) else {
            return;
        };
        match table.round {
            Round::Idle => return,
            Round::Closed => return self.tell_dice(id, npc, DiceNews::Missed),
            Round::Betting => {}
        }
        let mut back = Vec::new();
        table.bets.retain(|&(who, bet, money)| {
            let keep = who != id;
            if !keep {
                back.push((bet, money));
            }
            keep
        });
        if let Some(p) = self.players.get_mut(&id) {
            p.money += back.iter().map(|(_, m)| m).sum::<u64>();
        }
        let news = |mine| DiceNews::Chips {
            bets: back.clone(),
            mine,
            cancel: true,
        };
        self.tell_dice(id, npc, news(true));
        self.tell_table(npc, Some(id), &news(false));
    }

    /// Steps every dice table once a second (CNpcBet::OnTimer).
    pub(crate) fn tick_dice(&mut self) {
        let second = self.now_ms / 1000;
        let d = self.settings.dice.clone();
        let npcs: Vec<u32> = self.dice.keys().copied().collect();
        for npc in npcs {
            let table = self.dice.get_mut(&npc).expect("listed");
            if table.second >= second {
                continue;
            }
            table.second = second;
            match table.round {
                Round::Idle => {}
                Round::Betting => {
                    table.countdown -= 1;
                    if table.countdown < 1 {
                        table.round = Round::Closed;
                        self.tell_table(npc, None, &DiceNews::Closed);
                    }
                }
                Round::Closed => {
                    table.countdown -= 1;
                    let roll_at = -i64::from(d.roll_seconds);
                    if table.countdown == roll_at {
                        self.roll_dice(npc);
                    } else if table.countdown < roll_at - i64::from(d.next_seconds) + 1 {
                        table.round = Round::Betting;
                        table.countdown = i64::from(d.bet_seconds);
                        table.bets.clear();
                        let seconds = d.bet_seconds;
                        self.tell_table(npc, None, &DiceNews::Open { seconds });
                    }
                }
            }
        }
    }

    /// Rolls three dice at table `npc`, tells everyone near and pays its bets (last placed
    /// first; a player no longer here forfeits).
    fn roll_dice(&mut self, npc: u32) {
        let dice: [u8; 3] = std::array::from_fn(|_| self.rng.range(1, 6) as u8);
        let near: Vec<EntityId> = match self.dice_npc(npc) {
            Some(cell) => (self.players.values())
                .filter(|p| distance(p.cell, cell) <= self.settings.view_range)
                .map(|p| p.id)
                .collect(),
            None => Vec::new(),
        };
        for p in near {
            self.tell_dice(p, npc, DiceNews::Rolled { dice });
        }
        let sum = u32::from(dice[0]) + u32::from(dice[1]) + u32::from(dice[2]);
        let triple = (dice[0] == dice[1] && dice[1] == dice[2]).then_some(u32::from(dice[0]));
        let d = self.settings.dice.clone();
        let bets = self
            .dice
            .get(&npc)
            .map(|t| t.bets.clone())
            .unwrap_or_default();
        for (who, bet, money) in bets.into_iter().rev() {
            let Some(rule) = d.bets.get(usize::from(bet)) else {
                continue;
            };
            let wins = match rule.wins {
                DiceWin::SumAtMost(n) => sum <= n,
                DiceWin::SumAtLeast(n) => sum >= n,
                DiceWin::Triple(n) => triple == Some(n),
                DiceWin::Sum(n) => sum == n,
            };
            let Some(p) = self.players.get_mut(&who).filter(|_| wins) else {
                continue;
            };
            let amount = money * u64::from(rule.pays);
            p.money += amount;
            let name = p.name.clone();
            let cell = p.cell;
            self.tell_table(
                npc,
                None,
                &DiceNews::Won {
                    name: name.clone(),
                    amount,
                },
            );
            if d.notice_over > 0 && amount > d.notice_over {
                let currency = self.settings.currency.clone();
                let text = format!("{name} won {amount} {currency} by playing dice!");
                let range = self.settings.view_range;
                let near: Vec<EntityId> = (self.players.values())
                    .filter(|o| distance(o.cell, cell) <= range)
                    .map(|o| o.id)
                    .collect();
                for player in near {
                    let text = text.clone();
                    self.events.push(Event::Notice { player, text });
                }
            }
        }
    }
}
