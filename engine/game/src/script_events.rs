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
    /// Player `id` came into the game (not just onto another map): `on_login()`. What the
    /// script raises the most life and mana by is added to them (a new character's first
    /// points find it at full).
    pub fn logged_in(&mut self, id: EntityId) {
        let most = (self.players.get(&id)).map(|p| (p.stats.max_life, p.stats.max_mana));
        self.script_event(id, "on_login", Vec::new());
        if let (Some((life, mana)), Some(p)) = (most, self.players.get_mut(&id))
            && p.is_alive()
        {
            p.life = (p.life + p.stats.max_life.saturating_sub(life)).min(p.stats.max_life);
            p.mana = (p.mana + p.stats.max_mana.saturating_sub(mana)).min(p.stats.max_mana);
        }
    }

    /// Whether this map runs the world events (`on_minute`, world timers): a server has one
    /// map do it, so they run once.
    pub fn set_world_events(&mut self, on: bool) {
        self.world_events = on;
    }

    /// `on_minute` every minute of game time, and world timers that ran out.
    pub(crate) fn tick_world_events(&mut self) {
        if !self.world_events {
            return;
        }
        let tick = self.settings.tick_ms.max(1);
        if self.now_ms / 60_000 != self.now_ms.saturating_sub(tick) / 60_000 {
            self.world_event("on_minute");
        }
        let now = self.now_ms;
        let (due, left): (Vec<_>, Vec<_>) = std::mem::take(&mut self.world_timers)
            .into_iter()
            .partition(|(at, _)| *at <= now);
        self.world_timers = left;
        for (_, name) in due {
            self.world_event(&name);
        }
    }

    /// Runs `function()` of every events script and blueprint with no player: what it does
    /// to the world (variables, guilds, announcements, monsters, NPCs, effects) happens.
    pub fn world_event(&mut self, function: &str) {
        let Some(host) = self.scripts.as_mut() else {
            return;
        };
        for script in host.event_scripts() {
            let Some(host) = self.scripts.as_mut() else {
                return;
            };
            if !host.defines(&script, function, 0) {
                continue;
            }
            let mut state = c3_script::PlayerState {
                map: self.map_ids.first().copied().map_or(0, i64::from),
                world: self.world_flags.clone(),
                guilds: self.world.guild_views(),
                ..Default::default()
            };
            let host = self.scripts.as_mut().expect("checked above");
            let npc = NpcInfo {
                id: 0,
                name: String::new(),
            };
            match Conversation::event_in(host, &script, function, Vec::new(), npc, &mut state) {
                Ok((_, effects)) => {
                    let changed: Vec<(String, i64)> = (state.world.iter())
                        .filter(|(k, v)| self.world_flags.get(*k) != Some(v))
                        .map(|(k, &v)| (k.clone(), v))
                        .collect();
                    for (name, value) in changed {
                        self.change_world(c3_script::WorldChange::Flag { name, value });
                    }
                    for effect in effects {
                        self.script_effect(EntityId(0), 0, effect);
                    }
                }
                Err(e) => self
                    .events
                    .push(crate::Event::Warning(format!("Script error: {e}"))),
            }
        }
    }

    /// Whether any events script or blueprint answers `function` with `arity` arguments.
    pub(crate) fn event_defined(&mut self, function: &str, arity: usize) -> bool {
        let Some(host) = self.scripts.as_mut() else {
            return false;
        };
        let scripts = host.event_scripts();
        scripts.iter().any(|s| host.defines(s, function, arity))
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
