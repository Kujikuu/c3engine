//! Teams: a leader and up to [`TEAM_SIZE`] players in all, made and run with `/team` in chat
//! (create, invite, join, leave, kick). A player's team is its leader's id, kept with the
//! player (clients see who is in theirs). Members near a kill share its experience.

use crate::event::Event;
use crate::sim::Sim;
use crate::{EntityId, distance};

/// Most players in a team, its leader included.
pub const TEAM_SIZE: usize = 5;

/// What `/team` does, for its usage line.
const USAGE: &str =
    "/team · /team create · /team invite NAME · /team join NAME · /team leave · /team kick NAME";

impl Sim {
    /// The members of player `id`'s team (the leader first); empty when it has none.
    pub fn team_members(&self, id: EntityId) -> Vec<EntityId> {
        let Some(leader) = self.players.get(&id).and_then(|p| p.team) else {
            return Vec::new();
        };
        let mut members: Vec<EntityId> = (self.players.values())
            .filter(|p| p.team == Some(leader))
            .map(|p| p.id)
            .collect();
        members.sort_by_key(|&m| m != leader);
        members
    }

    fn player_named(&self, name: &str) -> Option<EntityId> {
        (self.players.values())
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .map(|p| p.id)
    }

    fn tell(&mut self, player: EntityId, text: String) {
        self.events.push(Event::Notice { player, text });
    }

    /// Tells every member of `leader`'s team.
    pub(crate) fn tell_team(&mut self, leader: EntityId, text: &str) {
        let members: Vec<EntityId> = (self.players.values())
            .filter(|p| p.team == Some(leader))
            .map(|p| p.id)
            .collect();
        for m in members {
            self.tell(m, text.to_owned());
        }
    }

    /// `/team ...` from player `id`; the answer, or why it can't be done.
    pub(crate) fn team_command(&mut self, id: EntityId, words: &[&str]) -> Result<String, String> {
        let me = self.players.get(&id).ok_or("No such player")?;
        let (my_name, my_team) = (me.name.clone(), me.team);
        let lead = my_team == Some(id);
        let target = |s: &Sim| -> Result<EntityId, String> {
            let name = words.get(1).ok_or(format!("Usage: {USAGE}"))?;
            s.player_named(name)
                .ok_or_else(|| format!("There is no player called {name} here."))
        };
        match words.first().copied().unwrap_or("") {
            "" => {
                let names: Vec<String> = (self.team_members(id).iter())
                    .filter_map(|m| self.players.get(m).map(|p| p.name.clone()))
                    .collect();
                if names.is_empty() {
                    Ok("You are in no team. /team create makes one.".into())
                } else {
                    Ok(format!("Your team: {}", names.join(", ")))
                }
            }
            "create" => {
                if my_team.is_some() {
                    return Err("You are in a team already.".into());
                }
                self.players.get_mut(&id).expect("checked").team = Some(id);
                Ok("You lead a new team. /team invite NAME asks players in.".into())
            }
            "invite" => {
                if !lead {
                    return Err("Only a team's leader invites.".into());
                }
                let other = target(self)?;
                if self.players[&other].team.is_some() {
                    return Err("They are in a team already.".into());
                }
                if self.team_members(id).len() >= TEAM_SIZE {
                    return Err(format!("A team has at most {TEAM_SIZE} players."));
                }
                self.team_invites.insert(other, id);
                let name = self.players[&other].name.clone();
                self.tell(
                    other,
                    format!("{my_name} invites you to a team: /team join {my_name}"),
                );
                Ok(format!("You invited {name}."))
            }
            "join" => {
                let leader = target(self)?;
                if self.team_invites.get(&id) != Some(&leader) {
                    return Err("They haven't invited you.".into());
                }
                if my_team.is_some() {
                    return Err("You are in a team already.".into());
                }
                if self.players[&leader].team != Some(leader)
                    || self.team_members(leader).len() >= TEAM_SIZE
                {
                    return Err("That team can't take you.".into());
                }
                self.team_invites.remove(&id);
                self.players.get_mut(&id).expect("checked").team = Some(leader);
                self.tell_team(leader, &format!("{my_name} joined the team."));
                Ok("You joined the team.".into())
            }
            "leave" => {
                let leader = my_team.ok_or("You are in no team.")?;
                self.leave_team(id, leader);
                Ok("You left the team.".into())
            }
            "kick" => {
                if !lead {
                    return Err("Only a team's leader kicks.".into());
                }
                let other = target(self)?;
                if other == id || self.players[&other].team != Some(id) {
                    return Err("They aren't in your team.".into());
                }
                self.leave_team(other, id);
                self.tell(other, "You were sent from the team.".into());
                Ok("Sent from the team.".into())
            }
            _ => Err(format!("Usage: {USAGE}")),
        }
    }

    /// Player `id` leaves `leader`'s team; a leader leaving ends it.
    pub(crate) fn leave_team(&mut self, id: EntityId, leader: EntityId) {
        let name = self
            .players
            .get(&id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        if id == leader {
            self.tell_team(leader, "The team broke up.");
            for p in self.players.values_mut().filter(|p| p.team == Some(leader)) {
                p.team = None;
            }
        } else if let Some(p) = self.players.get_mut(&id) {
            p.team = None;
            self.tell_team(leader, &format!("{name} left the team."));
        }
        self.team_invites
            .retain(|_, l| *l != leader || id != leader);
    }

    /// Experience for a kill by `killer`: its teammates near it (within view) get half.
    pub(crate) fn share_kill_exp(&mut self, killer: EntityId, exp: u64) {
        let Some(at) = self.players.get(&killer).map(|p| p.cell) else {
            return;
        };
        let range = self.settings.view_range;
        let near: Vec<EntityId> = (self.team_members(killer).into_iter())
            .filter(|&m| m != killer)
            .filter(|m| {
                self.players
                    .get(m)
                    .is_some_and(|p| p.is_alive() && distance(p.cell, at) <= range)
            })
            .collect();
        for m in near {
            self.gain_exp(m, exp / 2);
        }
    }
}
