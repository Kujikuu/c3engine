//! Quests in the world: the simulation counts the kills of players' kill quests. Taking and
//! completing quests happens in dialogue scripts (see `c3_script::quest`); the state lives in
//! the player's script variables.

use crate::event::Event;
use crate::sim::Sim;
use crate::{EntityId, Player};
use c3_script::quest::{self, Goal, Quest, TAKEN};

impl Sim {
    /// The game's quests (the project's `quests` table).
    pub fn set_quests(&mut self, quests: Vec<Quest>) {
        self.quests = quests;
    }

    pub fn quests(&self) -> &[Quest] {
        &self.quests
    }

    /// Counts the kill of a monster of type `kind` towards `player`'s kill quests.
    pub(crate) fn count_kill(&mut self, player: EntityId, kind: u32) {
        let Some(p) = self.players.get_mut(&player) else {
            return;
        };
        let mut notices = Vec::new();
        for q in &self.quests {
            let Goal::Kill { monster, count } = q.goal else {
                continue;
            };
            if monster != i64::from(kind) || p.quest_state(q.id) != TAKEN {
                continue;
            }
            let flag = quest::kills_flag(q.id);
            let kills = p.flags.get(&flag).copied().unwrap_or(0);
            if kills >= count {
                continue;
            }
            p.flags.insert(flag, kills + 1);
            notices.push(if kills + 1 == count {
                format!("{}: done! Go back for your reward.", q.name)
            } else {
                format!("{}: {} / {count}", q.name, kills + 1)
            });
        }
        for text in notices {
            self.events.push(Event::Notice { player, text });
        }
    }
}

impl Player {
    /// The state of quest `id`: `quest::NOT_TAKEN`, `TAKEN` or `DONE`.
    pub fn quest_state(&self, id: i64) -> i64 {
        self.flags.get(&quest::state_flag(id)).copied().unwrap_or(0)
    }

    /// How far quest `q` is: (done, needed).
    pub fn quest_progress(&self, q: &Quest) -> (i64, i64) {
        q.progress(
            |f| self.flags.get(f).copied().unwrap_or(0),
            |item| {
                self.bag
                    .iter()
                    .filter(|i| i64::from(i.kind) == item)
                    .count() as i64
            },
        )
    }
}
