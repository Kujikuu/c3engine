//! Mentors and students, kept in what every map shares (TQ's Guide.dll, adapters spec §5): a
//! player far enough above another teaches it (one asks, the other agrees); what the student
//! earns while it levels gives its mentor (and its mentor's mentor) a share the mentor claims
//! later as experience, blessed hours or an item. A student may leave its mentor (for a fee,
//! undone within the settings' hours), a mentor may send a student away.

use crate::EntityId;
use crate::event::Event;
use crate::sim::Sim;
use c3_script::WorldChange;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What a player does about mentors and students.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MentorAction {
    /// Offer to teach player `0` (here); take up its ask when it asked first.
    Teach(EntityId),
    /// Ask player `0` (here) to be one's mentor; take up its offer when it offered first.
    Learn(EntityId),
    /// Turn down the ask or offer of player `0`.
    Refuse(EntityId),
    /// The student leaves its mentor.
    Leave,
    /// The mentor sends the student of this name away.
    Expel(String),
    /// The mentor claims what its students gave: experience, blessed hours or the item.
    Claim(MentorGift),
}

/// What a mentor's students give it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MentorGift {
    Exp,
    Bless,
    Item,
}

/// What a mentor gets from its students: time at its level (taken as experience), blessed
/// hours and points towards an item (TQ's Exp, God_time and Addlevel).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gifts {
    pub time: i64,
    pub bless: i64,
    pub items: i64,
}

/// A student's mentor, and when (unix hours) the student left it (0: still its student).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mentorship {
    pub mentor: String,
    #[serde(default)]
    pub left: i64,
}

/// How often what students give is written to the shared world state (game ms).
const GIFT_FLUSH_MS: u64 = 60_000;

impl Sim {
    pub(crate) fn mentor_action(&mut self, id: EntityId, action: MentorAction) {
        if self.settings.mentor.min_level == 0 {
            return;
        }
        match action {
            MentorAction::Teach(other) => self.mentor_ask(id, other, true),
            MentorAction::Learn(other) => self.mentor_ask(id, other, false),
            MentorAction::Refuse(from) => {
                let me = self.player_name(id);
                if self.mentor_asks.remove(&(from, id)).is_some() {
                    self.message(from, format!("{me} has refused your request."));
                }
            }
            MentorAction::Leave => self.leave_mentor(id),
            MentorAction::Expel(student) => self.expel_student(id, &student),
            MentorAction::Claim(gift) => self.claim_gifts(id, gift),
        }
    }

    fn player_name(&self, id: EntityId) -> String {
        self.players
            .get(&id)
            .map_or_else(String::new, |p| p.name.clone())
    }

    /// Player `id`'s rebirths and level, as the mentor rules weigh them.
    fn standing(&self, id: EntityId) -> (u32, u32) {
        let Some(p) = self.players.get(&id) else {
            return (0, 0);
        };
        let rebirths = p.attributes.get("rebirths").copied().unwrap_or(0);
        (u32::try_from(rebirths.max(0)).unwrap_or(0), p.level)
    }

    /// Why `mentor` can't teach `student` (TQ's gap rule and student limit), if it can't.
    fn cannot_teach(&self, mentor: EntityId, student: EntityId) -> Option<String> {
        let m = &self.settings.mentor;
        let ((mr, ml), (sr, sl)) = (self.standing(mentor), self.standing(student));
        if mr == 0 && ml < m.min_level {
            return Some(format!("A mentor must be level {} or above.", m.min_level));
        }
        let gap = m.gaps.get(mr as usize).copied();
        let far_enough = mr >= sr && gap.is_some_and(|gap| ml >= sl + gap || (mr > 0 && sr < mr));
        if !far_enough {
            return Some("The student's level is too high.".into());
        }
        let name = self.player_name(mentor);
        let limit = (m.students.iter())
            .find(|[from, to, _]| (*from..=*to).contains(&ml))
            .map_or(0, |r| r[2]);
        if self.world.students_of(&name).len() >= limit as usize {
            return Some("You have too many students.".into());
        }
        let student = self.player_name(student);
        if self.world.mentors.contains_key(&student) {
            return Some(format!("{student} already has a mentor."));
        }
        None
    }

    /// `id` offers to teach (`teach`) or asks to learn from `other`; asked back, the two are
    /// mentor and student.
    fn mentor_ask(&mut self, id: EntityId, other: EntityId, teach: bool) {
        if other == id || !self.players.contains_key(&other) {
            return;
        }
        let (mentor, student) = if teach { (id, other) } else { (other, id) };
        if let Some(why) = self.cannot_teach(mentor, student) {
            return self.message(id, why);
        }
        // The other side asked first: agreed.
        if self.mentor_asks.remove(&(other, id)).is_some() {
            let (m, s) = (self.player_name(mentor), self.player_name(student));
            self.change_world(WorldChange::Mentorship {
                student: s.clone(),
                mentor: m.clone(),
                on: true,
                left: 0,
            });
            self.message(mentor, format!("{s} is your student now."));
            self.message(student, format!("{m} is your mentor now."));
            return;
        }
        self.mentor_asks.insert((id, other), teach);
        self.events.push(Event::MentorAsked {
            player: other,
            from: id,
            teach,
        });
        self.message(id, "The request has been sent.");
    }

    /// The student leaves its mentor: it pays the settings' fee, and the mentor may have it
    /// back within the settings' hours (TQ's betrayal, 50,000 silver, 72 hours).
    fn leave_mentor(&mut self, id: EntityId) {
        let me = self.player_name(id);
        let Some(m) = self.world.mentors.get(&me).filter(|m| m.left == 0).cloned() else {
            return;
        };
        let cost = self.settings.mentor.leave_cost;
        let Some(p) = self.players.get_mut(&id).filter(|p| p.money >= cost) else {
            return self.message(id, "You don't have enough money.");
        };
        p.money -= cost;
        let left = chrono::Utc::now().timestamp() / 3600;
        self.change_world(WorldChange::Mentorship {
            student: me.clone(),
            mentor: m.mentor.clone(),
            on: true,
            left,
        });
        self.message(id, format!("You have left your mentor {}.", m.mentor));
        if let Some(mentor) = self
            .players
            .values()
            .find(|p| p.name == m.mentor)
            .map(|p| p.id)
        {
            self.message(mentor, format!("Your student {me} has left you."));
        }
    }

    fn expel_student(&mut self, id: EntityId, student: &str) {
        let me = self.player_name(id);
        if (self.world.mentors.get(student)).is_none_or(|m| m.mentor != me) {
            return;
        }
        self.change_world(WorldChange::Mentorship {
            student: student.to_owned(),
            mentor: me,
            on: false,
            left: 0,
        });
        self.message(id, format!("You have expelled {student}."));
        if let Some(s) = self
            .players
            .values()
            .find(|p| p.name == student)
            .map(|p| p.id)
        {
            self.message(s, "Your mentor has expelled you.");
        }
    }

    /// Students who left their mentors longer ago than the settings' hours are gone for good.
    pub(crate) fn expire_mentors(&mut self) {
        let hours = self.settings.mentor.leave_hours;
        let now = chrono::Utc::now().timestamp() / 3600;
        let gone: Vec<(String, String)> = (self.world.mentors.iter())
            .filter(|(_, m)| m.left != 0 && now - m.left >= hours)
            .map(|(s, m)| (s.clone(), m.mentor.clone()))
            .collect();
        for (student, mentor) in gone {
            self.change_world(WorldChange::Mentorship {
                student,
                mentor,
                on: false,
                left: 0,
            });
        }
    }

    /// Player `id` gained `exp`: while it is a student below the settings' level, its mentor
    /// (and its mentor's mentor) get a share as time at its level (adapters spec §5.4).
    pub(crate) fn share_exp(&mut self, id: EntityId, exp: u64) {
        let m = &self.settings.mentor;
        if m.min_level == 0 || m.shares.is_empty() {
            return;
        }
        let Some(p) = self.players.get(&id) else {
            return;
        };
        if m.max_student_level > 0 && p.level >= m.max_student_level {
            return;
        }
        let (level, student) = (p.level, p.name.clone());
        let (Some(need), Some(time)) = (self.rules.level_exp(level), self.rules.level_time(level))
        else {
            return;
        };
        let mut mentor = student.clone();
        for generation in 0..2u32 {
            let Some(link) = self.world.mentors.get(&mentor).filter(|l| l.left == 0) else {
                break;
            };
            mentor = link.mentor.clone();
            let pct = (m.shares.iter())
                .find(|r| r[0] == generation && (r[1]..=r[2]).contains(&level))
                .map_or(0, |r| r[3]);
            let units = (u128::from(exp) * u128::from(time) * u128::from(pct))
                / (u128::from(need.max(1)) * 100);
            let units = i64::try_from(units).unwrap_or(i64::MAX);
            if units > 0 {
                let key = (mentor.clone(), student.clone());
                self.pending_gifts.entry(key).or_default().time += units;
            }
        }
        if self.next_gift_flush_ms == 0 {
            self.next_gift_flush_ms = self.now_ms + GIFT_FLUSH_MS;
        }
    }

    /// What students gave since the last flush, written to the world state.
    pub(crate) fn flush_gifts(&mut self) {
        if self.next_gift_flush_ms == 0 || self.now_ms < self.next_gift_flush_ms {
            return;
        }
        self.next_gift_flush_ms = 0;
        let pending: BTreeMap<(String, String), Gifts> = std::mem::take(&mut self.pending_gifts);
        for ((mentor, student), gifts) in pending {
            self.change_world(WorldChange::MentorGift {
                mentor,
                student,
                time: gifts.time,
                bless: gifts.bless,
                items: gifts.items,
            });
        }
    }

    /// The mentor takes what its students gave of `gift` (MsgGuidePrize 1-3).
    fn claim_gifts(&mut self, id: EntityId, gift: MentorGift) {
        let me = self.player_name(id);
        let pool = self
            .world
            .mentor_pools
            .get(&me)
            .copied()
            .unwrap_or_default();
        let taken = match gift {
            MentorGift::Exp if pool.time > 0 => {
                let exp = self.exp_for_time(id, pool.time);
                self.gain_exp(id, exp);
                Gifts {
                    time: -pool.time,
                    ..Gifts::default()
                }
            }
            MentorGift::Bless if pool.bless > 0 => {
                let status = self.settings.mentor.bless_status;
                let ms = u64::try_from(pool.bless).unwrap_or(0) * 3_600_000;
                self.add_status_for(id, status, None, Some(ms));
                Gifts {
                    bless: -pool.bless,
                    ..Gifts::default()
                }
            }
            MentorGift::Item if pool.items >= 100 => {
                if self
                    .players
                    .get(&id)
                    .is_none_or(|p| p.bag.len() >= self.bag_size())
                {
                    return self.message(id, "Your inventory is full!");
                }
                self.give_items(id, self.settings.mentor.item, 1);
                Gifts {
                    items: -100,
                    ..Gifts::default()
                }
            }
            _ => return self.message(id, "Your students haven't given you that yet."),
        };
        self.change_world(WorldChange::MentorGift {
            mentor: me,
            student: String::new(),
            time: taken.time,
            bless: taken.bless,
            items: taken.items,
        });
    }

    /// The experience that takes player `id` `units` of the levels' time further (TQ's
    /// AwardTimeExp).
    pub(crate) fn exp_for_time(&self, id: EntityId, units: i64) -> u64 {
        let Some(p) = self.players.get(&id) else {
            return 0;
        };
        let (mut level, cur) = (p.level, p.exp);
        let (Some(need), Some(time)) = (self.rules.level_exp(level), self.rules.level_time(level))
        else {
            return 0;
        };
        let mut left = units + (time as f64 * cur as f64 / need.max(1) as f64) as i64;
        let mut total = 0u64;
        while let (Some(need), Some(time)) =
            (self.rules.level_exp(level), self.rules.level_time(level))
        {
            if need == 0 || time == 0 {
                break;
            }
            let time = i64::try_from(time).unwrap_or(i64::MAX);
            if left < time {
                total += (need as f64 * left as f64 / time as f64) as u64;
                break;
            }
            left -= time;
            total += need;
            level += 1;
        }
        total.saturating_sub(cur)
    }
}
