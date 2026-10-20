//! The team dialog's auto check (TQ's aura group, CTeam::Process in Team.cpp 173-222): every
//! 30 seconds a leader whose team invites on its own asks the teamless players in view in,
//! and a teamless player with the check on asks up to four such leaders in view to take it;
//! one who said no is left alone for a minute (MsgTeam 14).

use c3_game::{EntityId, Input, Sim, TEAM_SIZE, TeamAction};
use std::collections::HashMap;

/// How often the check looks around, and how long one who said no is left alone (ms).
const PERIOD_MS: u64 = 30_000;
const HELD_OFF_MS: u64 = 60_000;

/// Leaders a teamless player asks at a time.
const APPLIES: usize = 4;

#[derive(Debug, Default)]
pub(crate) struct AutoTeam {
    /// A teamless hero asks the leaders near to take it.
    pub apply: bool,
    looked: Option<u64>,
    /// Who said no, and when.
    refused: HashMap<EntityId, u64>,
}

impl AutoTeam {
    /// `by` said no to the hero's invite or ask.
    pub fn refused(&mut self, by: EntityId, now_ms: u64) {
        self.refused.insert(by, now_ms);
    }

    /// What the hero asks now, if its check is due.
    pub fn asks(&mut self, sim: &Sim, hero: EntityId) -> Vec<Input> {
        let Some(me) = sim.player(hero).filter(|p| p.is_alive()) else {
            return Vec::new();
        };
        let leads = me.team == Some(hero);
        if (leads && !sim.auto_inviting(hero)) || (me.team.is_none() && !self.apply) {
            return Vec::new();
        }
        let now = sim.now_ms();
        // Each map's clock is its own: one behind the last look is due.
        if self
            .looked
            .is_some_and(|t| now.checked_sub(t).is_some_and(|d| d < PERIOD_MS))
        {
            return Vec::new();
        }
        self.looked = Some(now);
        let range = sim.settings().view_range;
        let free = |id: &EntityId| {
            (self.refused.get(id))
                .is_none_or(|t| now.checked_sub(*t).is_none_or(|d| d >= HELD_OFF_MS))
        };
        let near = sim
            .players()
            .filter(|p| p.id != hero && free(&p.id) && c3_game::distance(p.cell, me.cell) <= range);
        let size = |leader: EntityId| sim.team_members(leader).len();
        if me.team.is_none() {
            near.filter(|p| {
                p.team == Some(p.id) && sim.auto_inviting(p.id) && size(p.id) < TEAM_SIZE
            })
            .take(APPLIES)
            .map(|p| Input::Team(TeamAction::Apply(p.id)))
            .collect()
        } else if leads {
            near.filter(|p| p.team.is_none())
                .take(TEAM_SIZE.saturating_sub(size(hero)))
                .map(|p| Input::Team(TeamAction::Invite(p.id)))
                .collect()
        } else {
            Vec::new()
        }
    }
}
