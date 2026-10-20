//! Friends and enemies, kept in what every map shares: two players become friends when one
//! asks and the other takes it up, and a player killed by another keeps its killer as an
//! enemy. Friends hear each other on the friend channel.

use crate::EntityId;
use crate::event::Event;
use crate::sim::Sim;
use c3_script::WorldChange;
use serde::{Deserialize, Serialize};

/// The most friends a player keeps (TQ's 50).
pub const MAX_FRIENDS: usize = 50;
/// The most enemies a player keeps; a new one pushes out the oldest (TQ's 20).
pub const MAX_ENEMIES: usize = 20;

/// What a player does about friends and enemies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FriendAction {
    /// Ask player `0` (here) to be friends; take up its ask when it asked first.
    Ask(EntityId),
    /// Take up the ask of player `0` (here).
    Accept(EntityId),
    /// No longer friends with the player of this name.
    Remove(String),
    /// Take the player of this name off the enemies.
    Forget(String),
}

impl Sim {
    pub(crate) fn friend_action(&mut self, id: EntityId, action: FriendAction) {
        let Some(me) = self.players.get(&id).map(|p| p.name.clone()) else {
            return;
        };
        match action {
            FriendAction::Ask(other) => {
                let Some(name) = self.players.get(&other).map(|p| p.name.clone()) else {
                    return self.message(id, "They aren't here.");
                };
                if other == id || self.world.are_friends(&me, &name) {
                    return;
                }
                if self.friend_asks.get(&other) == Some(&id) {
                    return self.friend_action(id, FriendAction::Accept(other));
                }
                if self.world.friends_of(&me).len() >= MAX_FRIENDS {
                    return self.message(id, "You have too many friends.");
                }
                self.friend_asks.insert(id, other);
                self.events.push(Event::FriendAsked {
                    player: other,
                    from: id,
                });
                self.message(id, format!("You asked {name} to be friends."));
            }
            FriendAction::Accept(from) => {
                if self.friend_asks.get(&from) != Some(&id) {
                    return;
                }
                self.friend_asks.remove(&from);
                let Some(name) = self.players.get(&from).map(|p| p.name.clone()) else {
                    return;
                };
                if self.world.friends_of(&me).len() >= MAX_FRIENDS
                    || self.world.friends_of(&name).len() >= MAX_FRIENDS
                {
                    return self.message(id, "One of you has too many friends.");
                }
                self.change_world(WorldChange::Friendship {
                    a: me.clone(),
                    b: name.clone(),
                    on: true,
                });
                self.message(id, format!("{name} is your friend now."));
                self.message(from, format!("{me} is your friend now."));
            }
            FriendAction::Remove(name) => {
                if self.world.are_friends(&me, &name) {
                    self.change_world(WorldChange::Friendship {
                        a: me,
                        b: name.clone(),
                        on: false,
                    });
                    self.message(id, format!("{name} is no longer your friend."));
                }
            }
            FriendAction::Forget(name) => self.change_world(WorldChange::Enemy {
                name: me,
                enemy: name,
                on: false,
            }),
        }
    }

    /// Player `victim` was killed by player `killer`: the killer is its enemy now.
    pub(crate) fn note_enemy(&mut self, victim: EntityId, killer: EntityId) {
        let names = (self.players.get(&victim)).zip(self.players.get(&killer));
        let Some((v, k)) = names.map(|(v, k)| (v.name.clone(), k.name.clone())) else {
            return;
        };
        if v != k {
            self.change_world(WorldChange::Enemy {
                name: v,
                enemy: k,
                on: true,
            });
        }
    }
}
