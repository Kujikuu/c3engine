//! Talking to NPCs: the player walks up to an NPC, its script builds the dialogue, and the
//! player's answers move it on. What a script gives, takes or says lands on the player and
//! in events; a script can open the NPC's shop. Scripts run on the simulation's side, so
//! the server (M6) runs them the same way Play mode does.

use crate::entity::Task;
use crate::event::Event;
use crate::sim::Sim;
use crate::{EntityId, TALK_RANGE, distance};
use c3_scene::CellPos;
use c3_script::{Conversation, Effect, NpcInfo, PlayerState, ScriptError, Step};
use std::collections::{BTreeMap, BTreeSet};

impl Sim {
    /// Walks the player up to NPC `npc` and talks to it.
    pub fn talk(&mut self, id: EntityId, npc: u32) {
        let Some(n) = self.npcs.iter().find(|n| n.id == npc) else {
            return;
        };
        if n.script.is_empty() || self.scripts.is_none() {
            let text = format!("{} has nothing to say.", n.name);
            self.message(id, text);
            return;
        }
        let goal = n.cell;
        self.stop_talking(id);
        let Some(p) = self.players.get_mut(&id).filter(|p| p.is_alive()) else {
            return;
        };
        p.task = Task::Talk(npc);
        p.path.clear();
        let from = p.cell;
        if distance(from, goal) > 1 {
            let beside = self.beside(goal, from);
            self.plan_path(id, beside);
        }
    }

    /// Picks answer `index` of the player's dialogue.
    pub fn answer(&mut self, id: EntityId, index: usize) {
        let Some(conversation) = self.players.get_mut(&id).and_then(|p| p.dialogue.take()) else {
            return;
        };
        let npc = conversation.npc.id;
        let mut state = self.player_state(id);
        let Some(host) = self.scripts.as_mut() else {
            return;
        };
        let step = conversation.choose(host, index, &mut state);
        self.after_step(id, npc, step, state);
    }

    /// Closes the player's dialogue.
    pub fn close_dialogue(&mut self, id: EntityId) {
        if let Some(p) = self.players.get_mut(&id)
            && p.dialogue.take().is_some()
        {
            self.events.push(Event::Dialogue { player: id });
        }
    }

    /// Closes the shop the player has open.
    pub fn close_shop(&mut self, id: EntityId) {
        if let Some(p) = self.players.get_mut(&id) {
            p.shop = None;
        }
    }

    /// The player turned to something else: dialogue and shop close.
    pub(crate) fn stop_talking(&mut self, id: EntityId) {
        self.close_dialogue(id);
        self.close_shop(id);
    }

    /// Walking up to an NPC; on arrival, talks.
    pub(crate) fn walk_to_npc(&mut self, id: EntityId, npc: u32) {
        if self.step_player(id) {
            return;
        }
        let p = self.players.get_mut(&id).expect("ticking");
        p.task = Task::Idle;
        let here = p.cell;
        let Some(n) = self.npcs.iter().find(|n| n.id == npc) else {
            return;
        };
        let (cell, name, script) = (n.cell, n.name.clone(), n.script.clone());
        if distance(here, cell) > TALK_RANGE {
            self.message(id, format!("You can't reach {name}."));
            return;
        }
        let p = self.players.get_mut(&id).expect("ticking");
        p.direction = c3_scene::path::facing(here, cell).unwrap_or(p.direction);
        let mut state = self.player_state(id);
        let Some(host) = self.scripts.as_mut() else {
            return;
        };
        let step = Conversation::start(host, &script, NpcInfo { id: npc, name }, &mut state);
        self.after_step(id, npc, step, state);
    }

    /// Carries out a dialogue step: the player takes the script's changes, its effects
    /// become events, and the next page (if any) opens.
    fn after_step(
        &mut self,
        id: EntityId,
        npc: u32,
        step: Result<Step, ScriptError>,
        state: PlayerState,
    ) {
        // Changes made before an error stay.
        self.apply_state(id, &state);
        match step {
            Ok((next, effects)) => {
                if let Some(p) = self.players.get_mut(&id) {
                    p.dialogue = next;
                }
                self.events.push(Event::Dialogue { player: id });
                for effect in effects {
                    self.script_effect(id, npc, effect);
                }
            }
            Err(e) => {
                self.message(id, "Something went wrong with this NPC.");
                self.events
                    .push(Event::Warning(format!("Script error: {e}")));
            }
        }
    }

    fn script_effect(&mut self, id: EntityId, npc: u32, effect: Effect) {
        let name = |s: &Sim, kind: i64| {
            u32::try_from(kind)
                .ok()
                .and_then(|k| s.rules.item(k))
                .map_or_else(|| format!("item {kind}"), |t| t.name.clone())
        };
        let notice = |s: &mut Sim, text: String| s.events.push(Event::Notice { player: id, text });
        match effect {
            Effect::Message(text) => notice(self, text),
            Effect::GaveItem { item, count } => {
                let text = format!("You received {count} × {}.", name(self, item));
                notice(self, text);
            }
            Effect::TookItem { item, count } => {
                let text = format!("You handed over {count} × {}.", name(self, item));
                notice(self, text);
            }
            Effect::Money(n) if n >= 0 => notice(self, format!("You received {n} silver.")),
            Effect::Money(n) => notice(self, format!("You paid {} silver.", -n)),
            Effect::Teleport { map, cell } => {
                let here = map == 0 || self.map_ids.iter().any(|&m| i64::from(m) == map);
                let to = (cell.0 as i32, cell.1 as i32);
                if here && self.walkable(to) {
                    self.close_dialogue(id);
                    self.place(id, to);
                } else {
                    self.message(
                        id,
                        format!("Map {map} isn't running here, so the teleport to ({}, {}) can't happen.", to.0, to.1),
                    );
                }
            }
            Effect::OpenShop => {
                if self.rules.shop(npc).is_empty() {
                    self.message(id, "This NPC has nothing to sell.");
                } else if let Some(p) = self.players.get_mut(&id) {
                    p.shop = Some(npc);
                    p.dialogue = None;
                    self.events.push(Event::Dialogue { player: id });
                    self.events.push(Event::ShopOpened { player: id, npc });
                }
            }
            Effect::Log(text) => self.events.push(Event::ScriptLog(text)),
        }
    }

    /// The player as scripts see it.
    fn player_state(&self, id: EntityId) -> PlayerState {
        let Some(p) = self.players.get(&id) else {
            return PlayerState::default();
        };
        let mut items = BTreeMap::new();
        for item in &p.bag {
            *items.entry(i64::from(item.kind)).or_insert(0) += 1;
        }
        PlayerState {
            name: p.name.clone(),
            level: p.level.into(),
            money: p.money as i64,
            items,
            flags: p.flags.clone(),
        }
    }

    /// Puts a script's changes to money, items and variables on the player.
    fn apply_state(&mut self, id: EntityId, after: &PlayerState) {
        let before = self.player_state(id);
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        p.money = after.money.max(0) as u64;
        p.flags = after.flags.clone();
        let kinds: BTreeSet<i64> = before
            .items
            .keys()
            .chain(after.items.keys())
            .copied()
            .collect();
        for kind in kinds {
            let Ok(k) = u32::try_from(kind) else { continue };
            let change = after.item_count(kind) - before.item_count(kind);
            if change > 0 {
                for _ in 0..change {
                    self.give_item(id, k);
                }
            } else if change < 0 {
                self.take_items(id, k, (-change) as usize);
            }
        }
    }

    /// The walkable cell next to `npc` nearest `from` (or the NPC's own cell).
    fn beside(&self, npc: CellPos, from: CellPos) -> CellPos {
        (-1..=1)
            .flat_map(|dx| (-1..=1).map(move |dy| (npc.0 + dx, npc.1 + dy)))
            .filter(|&c| c != npc && self.walkable(c))
            .min_by_key(|&c| distance(c, from) * 100 + (c.0 - from.0).abs() + (c.1 - from.1).abs())
            .unwrap_or(npc)
    }
}
