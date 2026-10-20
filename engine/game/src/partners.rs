//! Trade partners, kept in what every map shares (TQ's cq_business, social spec §3): one
//! player asks another, here, and the partnership starts when the other asks back; for the
//! settings' probation it is on trial, then it is formal. Either side may end it; an offline
//! partner is told by leave words.

use crate::EntityId;
use crate::event::Event;
use crate::sim::Sim;
use c3_script::{Delivery, WorldChange};
use serde::{Deserialize, Serialize};

/// The most trade partners a player keeps (TQ's 20).
pub const MAX_PARTNERS: usize = 20;

/// What a player does about trade partners.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartnerAction {
    /// Ask player `0` (here) to be a trade partner; take up its ask when it asked first.
    Ask(EntityId),
    /// Turn down the ask of player `0`.
    Refuse(EntityId),
    /// End the partnership with the player of this name.
    Break(String),
}

impl Sim {
    pub(crate) fn partner_action(&mut self, id: EntityId, action: PartnerAction) {
        let Some(me) = self.players.get(&id).map(|p| p.name.clone()) else {
            return;
        };
        match action {
            PartnerAction::Ask(other) => self.ask_partner(id, &me, other),
            PartnerAction::Refuse(from) => {
                if self.partner_asks.get(&from) == Some(&id) {
                    self.partner_asks.remove(&from);
                    self.message(from, format!("{me} has refused to be your Trade Partner."));
                }
            }
            PartnerAction::Break(name) => {
                if !self.world.are_partners(&me, &name) {
                    return;
                }
                self.change_world(WorldChange::Partnership {
                    a: me.clone(),
                    b: name.clone(),
                    on: false,
                    since: 0,
                });
                self.message(
                    id,
                    format!("You have ended up the partnership with {name}."),
                );
                let told = format!("{me} has ended up the partnership with you.");
                match self.players.values().find(|p| p.name == name).map(|p| p.id) {
                    Some(other) => self.message(other, told),
                    // An offline partner finds leave words.
                    None => self.change_world(WorldChange::Deliver {
                        to: name,
                        delivery: Delivery::Word {
                            from: me,
                            text: told,
                        },
                    }),
                }
            }
        }
    }

    fn ask_partner(&mut self, id: EntityId, me: &str, other: EntityId) {
        let Some(name) = self.players.get(&other).map(|p| p.name.clone()) else {
            return self.message(id, "The target player is offline.");
        };
        if other == id {
            return;
        }
        if self.world.are_partners(me, &name) {
            return self.message(id, "This player is already your Trade Partner.");
        }
        if self.world.partners_of(me).len() >= MAX_PARTNERS {
            return self.message(id, "Sorry, you can`t have more than 20 Trade Partners.");
        }
        if self.world.partners_of(&name).len() >= MAX_PARTNERS {
            return self.message(id, "This player`s can`t have more Trade Partners.");
        }
        // Asked back: partners from now, on trial for the probation.
        if self.partner_asks.get(&other) == Some(&id) {
            self.partner_asks.remove(&other);
            let since = chrono::Utc::now().timestamp();
            self.change_world(WorldChange::Partnership {
                a: me.to_owned(),
                b: name.clone(),
                on: true,
                since,
            });
            let days = self.settings.partner_probation_secs / 86_400;
            let after = format!("You will be formal Trade Partners after {days} days.");
            self.message(id, format!("You have accepted {name}`s request. {after}"));
            self.message(other, format!("{me} has accepted your request. {after}"));
            return;
        }
        self.partner_asks.insert(id, other);
        self.events.push(Event::PartnerAsked {
            player: other,
            from: id,
        });
        self.message(id, "The request has been sent.");
    }
}
