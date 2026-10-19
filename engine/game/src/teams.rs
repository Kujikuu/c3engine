//! Teams: a leader and up to [`TEAM_SIZE`] players in all, made and run with `/team` in chat
//! (create, invite, join, leave, kick). A player's team is its leader's id, kept with the
//! player (clients see who is in theirs). Members near a kill share its experience.
//!
//! Teams reach across maps: the host tells each map who plays on the others
//! ([`Sim::set_elsewhere`]), and carries what a team asks of those players
//! ([`Sim::take_team_ops`]) to their maps ([`Sim::apply_team_op`]).

use crate::event::Event;
use crate::sim::Sim;
use crate::{EntityId, distance};
use serde::{Deserialize, Serialize};

/// Most players in a team, its leader included.
pub const TEAM_SIZE: usize = 5;

/// What `/team` does, for its usage line.
const USAGE: &str =
    "/team · /team create · /team invite NAME · /team join NAME · /team leave · /team kick NAME";

/// A player as teams see it, on any map: its name, team and life.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mate {
    pub id: EntityId,
    pub name: String,
    pub team: Option<EntityId>,
    pub life: u32,
    pub max_life: u32,
}

/// What a player does about teams (the same as `/team ...`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TeamAction {
    Create,
    Invite(EntityId),
    /// Join the team of the leader who invited the player.
    Join(EntityId),
    Leave,
    Kick(EntityId),
}

/// What a team asks of a player on another map (the host carries it there).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeamOp {
    Notice(String),
    SetTeam(Option<EntityId>),
    /// Invited by this leader.
    Invite(EntityId),
}

impl Sim {
    /// Every player here, as teams on other maps see them.
    pub fn mates(&self) -> Vec<Mate> {
        (self.players.values())
            .map(|p| Mate {
                id: p.id,
                name: p.name.clone(),
                team: p.team,
                life: p.life,
                max_life: p.stats.max_life,
            })
            .collect()
    }

    /// The players on other maps (the host sets them before each tick).
    pub fn set_elsewhere(&mut self, mates: Vec<Mate>) {
        self.elsewhere = mates.into_iter().map(|m| (m.id, m)).collect();
    }

    /// What teams here asked of players on other maps since last taken.
    pub fn take_team_ops(&mut self) -> Vec<(EntityId, TeamOp)> {
        std::mem::take(&mut self.team_ops)
    }

    /// Does `op` for player `id` (one on this map) that a team on another map asked.
    pub fn apply_team_op(&mut self, id: EntityId, op: TeamOp) {
        match op {
            TeamOp::Notice(text) => self.tell(id, text),
            TeamOp::SetTeam(team) => self.set_member_team(id, team),
            TeamOp::Invite(leader) => self.invited(id, leader),
        }
    }

    /// Player `id` acts on teams; told how it went.
    pub fn team(&mut self, id: EntityId, action: TeamAction) {
        let (word, other) = match action {
            TeamAction::Create => ("create", None),
            TeamAction::Invite(e) => ("invite", Some(e)),
            TeamAction::Join(e) => ("join", Some(e)),
            TeamAction::Leave => ("leave", None),
            TeamAction::Kick(e) => ("kick", Some(e)),
        };
        let name = other.map(|e| self.name_of(e));
        let words: Vec<&str> = std::iter::once(word).chain(name.as_deref()).collect();
        let text = self.team_command(id, &words).unwrap_or_else(|why| why);
        self.events.push(Event::Notice { player: id, text });
    }

    /// Player `id` (on this map) is invited to `leader`'s team.
    fn invited(&mut self, id: EntityId, leader: EntityId) {
        self.team_invites.insert(id, leader);
        self.events.push(Event::TeamInvited { player: id, leader });
    }

    /// The members of player `id`'s team on every map (the leader first); empty when it
    /// has none.
    pub fn team_members(&self, id: EntityId) -> Vec<EntityId> {
        match self.team_of_any(id) {
            Some(leader) => self.in_team(leader),
            None => Vec::new(),
        }
    }

    /// The hero's teammates (itself among them) for its team list, the leader first.
    pub fn team_mates(&self, id: EntityId) -> Vec<Mate> {
        let here = self.mates();
        let find = |m: EntityId| {
            (here.iter())
                .find(|p| p.id == m)
                .or_else(|| self.elsewhere.get(&m))
                .cloned()
        };
        self.team_members(id).into_iter().filter_map(find).collect()
    }

    /// Everyone in `leader`'s team, here or elsewhere, the leader first.
    fn in_team(&self, leader: EntityId) -> Vec<EntityId> {
        let here = (self.players.values()).map(|p| (p.id, p.team));
        let away = (self.elsewhere.values()).map(|m| (m.id, m.team));
        let mut members: Vec<EntityId> = (here.chain(away))
            .filter(|&(_, team)| team == Some(leader))
            .map(|(id, _)| id)
            .collect();
        members.sort_by_key(|&m| (m != leader, m));
        members.dedup();
        members
    }

    /// The team of player `id`, here or on another map.
    fn team_of_any(&self, id: EntityId) -> Option<EntityId> {
        match self.players.get(&id) {
            Some(p) => p.team,
            None => self.elsewhere.get(&id).and_then(|m| m.team),
        }
    }

    fn name_of(&self, id: EntityId) -> String {
        (self.players.get(&id).map(|p| p.name.clone()))
            .or_else(|| self.elsewhere.get(&id).map(|m| m.name.clone()))
            .unwrap_or_default()
    }

    fn player_named(&self, name: &str) -> Option<EntityId> {
        let here = (self.players.values()).map(|p| (p.id, &p.name));
        let away = (self.elsewhere.values()).map(|m| (m.id, &m.name));
        (here.chain(away))
            .find(|(_, n)| n.eq_ignore_ascii_case(name))
            .map(|(id, _)| id)
    }

    fn tell(&mut self, player: EntityId, text: String) {
        if self.players.contains_key(&player) {
            self.events.push(Event::Notice { player, text });
        } else if self.elsewhere.contains_key(&player) {
            self.team_ops.push((player, TeamOp::Notice(text)));
        }
    }

    /// Puts player `id` (here or elsewhere) in `team`.
    fn set_member_team(&mut self, id: EntityId, team: Option<EntityId>) {
        if let Some(p) = self.players.get_mut(&id) {
            p.team = team;
        } else if let Some(m) = self.elsewhere.get_mut(&id) {
            m.team = team;
            self.team_ops.push((id, TeamOp::SetTeam(team)));
        }
    }

    /// Tells every member of `leader`'s team, on every map.
    pub(crate) fn tell_team(&mut self, leader: EntityId, text: &str) {
        for m in self.in_team(leader) {
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
                .ok_or_else(|| format!("There is no player called {name} online."))
        };
        match words.first().copied().unwrap_or("") {
            "" => {
                let names: Vec<String> = (self.team_members(id).into_iter())
                    .map(|m| self.name_of(m))
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
                self.set_member_team(id, Some(id));
                Ok("You lead a new team. /team invite NAME asks players in.".into())
            }
            "invite" => {
                if !lead {
                    return Err("Only a team's leader invites.".into());
                }
                let other = target(self)?;
                if self.team_of_any(other).is_some() {
                    return Err("They are in a team already.".into());
                }
                if self.in_team(id).len() >= TEAM_SIZE {
                    return Err(format!("A team has at most {TEAM_SIZE} players."));
                }
                if self.players.contains_key(&other) {
                    self.invited(other, id);
                } else {
                    self.team_ops.push((other, TeamOp::Invite(id)));
                }
                self.tell(
                    other,
                    format!("{my_name} invites you to a team: /team join {my_name}"),
                );
                Ok(format!("You invited {}.", self.name_of(other)))
            }
            "join" => {
                let leader = target(self)?;
                if self.team_invites.get(&id) != Some(&leader) {
                    return Err("They haven't invited you.".into());
                }
                if my_team.is_some() {
                    return Err("You are in a team already.".into());
                }
                if self.team_of_any(leader) != Some(leader)
                    || self.in_team(leader).len() >= TEAM_SIZE
                {
                    return Err("That team can't take you.".into());
                }
                self.team_invites.remove(&id);
                self.set_member_team(id, Some(leader));
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
                if other == id || self.team_of_any(other) != Some(id) {
                    return Err("They aren't in your team.".into());
                }
                self.leave_team(other, id);
                self.tell(other, "You were sent from the team.".into());
                Ok("Sent from the team.".into())
            }
            _ => Err(format!("Usage: {USAGE}")),
        }
    }

    /// Player `id` leaves `leader`'s team; a leader leaving ends it (on every map).
    pub(crate) fn leave_team(&mut self, id: EntityId, leader: EntityId) {
        if id == leader {
            self.tell_team(leader, "The team broke up.");
            for m in self.in_team(leader) {
                self.set_member_team(m, None);
            }
        } else {
            let name = self.name_of(id);
            self.set_member_team(id, None);
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
