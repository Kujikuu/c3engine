//! What happens to players beyond talking, as the project's events script says
//! (`scripts/events.rhai`, every function optional): `on_login()` when a player comes into
//! the game, `on_enter(map)` on each map it arrives on, `on_kill(monster)` and
//! `on_use(item)` with the type's id, and `on_timer(name)` when a timer the script started
//! (`start_timer(name, seconds)`) runs out. They see and change the player as NPC scripts
//! do; a page they build opens as a dialogue.

use crate::EntityId;
use crate::sim::Sim;
use c3_script::{Conversation, Dynamic, EVENTS_SCRIPT, NpcInfo};

impl Sim {
    /// Player `id` came into the game (not just onto another map): `on_login()`.
    pub fn logged_in(&mut self, id: EntityId) {
        self.script_event(id, "on_login", Vec::new());
    }

    /// Runs `function(args...)` of the events script for player `id`, if the project has it.
    pub(crate) fn script_event(&mut self, id: EntityId, function: &str, args: Vec<Dynamic>) {
        if !self.players.contains_key(&id) {
            return;
        }
        let arity = args.len();
        let Some(host) = self.scripts.as_mut() else {
            return;
        };
        if !host.defines(EVENTS_SCRIPT, function, arity) {
            return;
        }
        let mut state = self.player_state(id);
        let host = self.scripts.as_mut().expect("checked above");
        let npc = NpcInfo {
            id: 0,
            name: String::new(),
        };
        let step = Conversation::event(host, function, args, npc, &mut state);
        self.after_step(id, 0, step, state);
    }

    /// Starts timer `name` of player `id`: `on_timer(name)` runs after `seconds`.
    pub(crate) fn start_timer(&mut self, id: EntityId, name: String, seconds: f64) {
        let at = self.now_ms + (seconds.max(0.0) * 1000.0) as u64;
        if let Some(p) = self.players.get_mut(&id) {
            p.timers.retain(|(n, _)| *n != name);
            p.timers.push((name, at));
        }
    }

    /// Runs the timers that ran out.
    pub(crate) fn tick_timers(&mut self) {
        let now = self.now_ms;
        let due: Vec<(EntityId, String)> = (self.players.values_mut())
            .flat_map(|p| {
                let id = p.id;
                let (done, left): (Vec<_>, Vec<_>) = std::mem::take(&mut p.timers)
                    .into_iter()
                    .partition(|(_, at)| *at <= now);
                p.timers = left;
                done.into_iter().map(move |(name, _)| (id, name))
            })
            .collect();
        for (id, name) in due {
            self.script_event(id, "on_timer", vec![name.into()]);
        }
    }
}
