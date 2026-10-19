//! What happens to players beyond talking, as the project's events script says
//! (`scripts/events.rhai`, every function optional): `on_login()` when a player comes into
//! the game, `on_enter(map)` on each map it arrives on, `on_kill(monster)` and
//! `on_use(item)` with the type's id, `on_timer(name)` when a timer the script started
//! (`start_timer(name, seconds)`) runs out, and `on_enter_region(id)` / `on_leave_region(id)`
//! as the player walks into or out of one of the map's regions. They see and change the player as NPC scripts
//! do; a page they build opens as a dialogue.

use crate::EntityId;
use crate::sim::Sim;
use c3_script::{Conversation, Dynamic, NpcInfo};

impl Sim {
    /// Player `id` came into the game (not just onto another map): `on_login()`. One at full
    /// life and mana stays full when the script raises them (a new character's first points).
    pub fn logged_in(&mut self, id: EntityId) {
        let full = (self.players.get(&id))
            .map(|p| (p.life >= p.stats.max_life, p.mana >= p.stats.max_mana));
        self.script_event(id, "on_login", Vec::new());
        if let (Some((life, mana)), Some(p)) = (full, self.players.get_mut(&id)) {
            if life && p.is_alive() {
                p.life = p.stats.max_life;
            }
            if mana {
                p.mana = p.stats.max_mana;
            }
        }
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
        // The events script and every blueprint defining the event, in turn.
        for script in host.event_scripts() {
            let Some(host) = self.scripts.as_mut() else {
                return;
            };
            if !host.defines(&script, function, arity) {
                continue;
            }
            let mut state = self.player_state(id);
            let host = self.scripts.as_mut().expect("checked above");
            let npc = NpcInfo {
                id: 0,
                name: String::new(),
            };
            let step =
                Conversation::event_in(host, &script, function, args.clone(), npc, &mut state);
            self.after_step(id, 0, step, state);
        }
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
