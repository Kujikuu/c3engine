//! Offline training (TQ's training ground, Training.dll's "HungGame"): minutes online bank a
//! credit (up to the settings' most), each worth some training minutes counted in steps; a
//! player (holding the settings' attribute, TQ's heaven blessing) starts it on leaving, and
//! on coming back claims the experience of the time it was away (TQ's `Window` and
//! `Simulate`: level-time units at the settings' rate, carried through the levels).

use crate::EntityId;
use crate::entity::Player;
use crate::event::Event;
use crate::sim::Sim;
use serde::{Deserialize, Serialize};

/// What a player asks of offline training.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrainingAction {
    /// How many minutes it may train (answered by `Offer`, or `Denied`).
    Ask,
    /// Starting it (the player then leaves).
    Start,
    /// What it trained so far (answered by `Info`).
    Query,
    /// Taking the experience, the training over.
    Claim,
}

/// What offline training tells a player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrainingNews {
    Offer {
        minutes: u32,
    },
    Denied,
    Started,
    /// Come back from training: what it trained may be asked.
    Ready,
    /// Minutes trained and still banked, the level it would reach and how far into it (in
    /// ten-millionths).
    Info {
        trained: u32,
        remaining: u32,
        level: u32,
        fraction: u32,
    },
    Claimed,
}

/// Unix seconds now.
pub(crate) fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

impl Sim {
    /// The credit player `p` holds now: banked, and its minutes online since.
    fn training_credit(&self, p: &Player, now: i64) -> u32 {
        let online = u32::try_from((now - p.online_since).max(0) / 60).unwrap_or(u32::MAX);
        let most = self.settings.offline_training.most_credit;
        p.training_credit.saturating_add(online).min(most)
    }

    /// Whether player `p` holds the settings' attribute (heaven's blessing) at `now`, and
    /// when it ends (unix seconds; none: not needed).
    fn training_blessing(&self, p: &Player, now: i64) -> (bool, Option<i64>) {
        let name = &self.settings.offline_training.attribute;
        if name.is_empty() {
            return (true, None);
        }
        let until = p.attributes.get(name).copied().unwrap_or(0);
        (until > now, Some(until))
    }

    /// Minutes trained and minutes left of the credit (TQ's `Window`).
    fn training_window(&self, p: &Player, now: i64) -> (u32, u32) {
        let t = &self.settings.offline_training;
        let step = t.step.max(1);
        let credit = p.training_credit.min(t.most_credit);
        let cap = credit * t.minutes_a_credit / step * step;
        let away = u32::try_from((now - p.training_since).max(0) / 60).unwrap_or(u32::MAX);
        let mut trained = if away < cap { away / step * step } else { cap };
        // The blessing's end stops the count.
        if let (false, Some(until)) = self.training_blessing(p, now) {
            let blessed = u32::try_from((until / 60 - p.training_since / 60).max(0)).unwrap_or(0);
            trained = trained.min(blessed / step * step);
        }
        (trained, cap - trained)
    }

    /// The levels gained, the experience then and how far into the level (ten-millionths)
    /// `minutes` of training give a player of `level` holding `exp` (TQ's `Simulate`).
    pub(crate) fn training_result(&self, level: u32, exp: u64, minutes: u32) -> (u32, u64, u32) {
        let rules = self.rules.as_ref();
        let Some(need) = rules.level_exp(level).filter(|n| *n > 0) else {
            return (0, exp, 0);
        };
        let time = |l: u32| rules.level_time(l).unwrap_or(0);
        // Levels that take no time give nothing.
        if time(level) == 0 {
            return (0, exp, 0);
        }
        let rate = f64::from(self.settings.offline_training.rate_percent) / 100.0;
        let mut units = f64::from(minutes) * rate + exp as f64 * time(level) as f64 / need as f64;
        let mut gained = 0;
        while rules.level_exp(level + gained).is_some() {
            let u = time(level + gained);
            if u < 1 || units < u as f64 {
                break;
            }
            units -= u as f64;
            gained += 1;
        }
        let at = level + gained;
        let need_at = rules.level_exp(at).unwrap_or(0).max(1);
        let exp_at = (need_at as f64 * units / time(at).max(1) as f64) as u64;
        let fraction = (exp_at as f64 / need_at as f64 * 10_000_000.0) as u32;
        (gained, exp_at, fraction)
    }

    pub(crate) fn training_action(&mut self, id: EntityId, action: TrainingAction) {
        if self.settings.offline_training.most_credit == 0 {
            return;
        }
        let now = now_secs();
        let Some(p) = self.players.get(&id) else {
            return;
        };
        let training = p.training_since > 0;
        let news = match action {
            TrainingAction::Ask => {
                let mac = self.settings.offline_training.minutes_a_credit;
                let credit = if training {
                    p.training_credit
                } else {
                    self.training_credit(p, now)
                };
                TrainingNews::Offer {
                    minutes: credit * mac,
                }
            }
            TrainingAction::Start if training => TrainingNews::Started,
            TrainingAction::Start => {
                if !self.training_blessing(p, now).0 {
                    return;
                }
                let credit = self.training_credit(p, now);
                let p = self.players.get_mut(&id).expect("checked above");
                p.training_credit = credit;
                p.training_since = now;
                p.online_since = now;
                TrainingNews::Started
            }
            TrainingAction::Query if training => {
                let (trained, remaining) = self.training_window(p, now);
                let (gained, _, fraction) = self.training_result(p.level, p.exp, trained);
                TrainingNews::Info {
                    trained,
                    remaining,
                    level: p.level + gained,
                    fraction,
                }
            }
            TrainingAction::Claim if training => {
                let (trained, remaining) = self.training_window(p, now);
                let (gained, exp, _) = self.training_result(p.level, p.exp, trained);
                for _ in 0..gained {
                    self.raise_level(id);
                }
                let mac = self.settings.offline_training.minutes_a_credit.max(1);
                let p = self.players.get_mut(&id).expect("checked above");
                if gained > 0 || p.exp < exp {
                    p.exp = exp;
                }
                p.training_credit = remaining / mac;
                p.training_since = 0;
                p.online_since = now;
                TrainingNews::Claimed
            }
            TrainingAction::Query | TrainingAction::Claim => return,
        };
        self.events.push(Event::Training { player: id, news });
    }

    /// A player come back: its stay online counts from now, and one away training is told.
    pub(crate) fn training_login(&mut self, id: EntityId) {
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        p.online_since = now_secs();
        if p.training_since > 0 && self.settings.offline_training.most_credit > 0 {
            let news = TrainingNews::Ready;
            self.events.push(Event::Training { player: id, news });
        }
    }

    /// The credit to save for player `p`: banked, and its stay online unless away training.
    pub(crate) fn training_credit_saved(&self, p: &Player) -> u32 {
        match p.training_since {
            0 => self.training_credit(p, now_secs()),
            _ => p.training_credit,
        }
    }
}
