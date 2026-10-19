//! Talking to NPCs: the player walks up to an NPC, its script builds the dialogue, and the
//! player's answers move it on. What a script gives, takes or says lands on the player and
//! in events; a script can open the NPC's shop. Scripts run on the simulation's side, so
//! the server (M6) runs them the same way Play mode does.

use crate::entity::Task;
use crate::event::Event;
use crate::sim::Sim;
use crate::{EntityId, distance};
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
        if distance(here, cell) > self.settings.talk_range {
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
    pub(crate) fn after_step(
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
            Effect::Money(n) => {
                let currency = self.settings.currency.clone();
                notice(self, money_text(n, &currency));
            }
            Effect::Currency { name, amount } => notice(self, money_text(amount, &name)),
            Effect::LearnedSkill(skill) => {
                let skill = u32::try_from(skill).ok().and_then(|s| self.rules.skill(s));
                if let Some(t) = skill {
                    let text = format!("You learned {}.", t.name);
                    notice(self, text);
                }
            }
            Effect::Teleport { map, cell } => {
                let to = c3_scene::PortalTarget {
                    map: u32::try_from(map).unwrap_or(0),
                    cell: (cell.0 as i32, cell.1 as i32),
                };
                if self.is_this_map(to.map) && !self.walkable(to.cell) {
                    self.message(
                        id,
                        format!(
                            "({}, {}) can't be stood on, so the teleport can't happen.",
                            to.cell.0, to.cell.1
                        ),
                    );
                } else {
                    self.close_dialogue(id);
                    self.travel(id, to);
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
            Effect::Exp(n) => self.gain_exp(id, n.max(0) as u64),
            Effect::QuestTaken { name, .. } => notice(self, format!("Quest taken: {name}")),
            Effect::QuestDone { name, .. } => notice(self, format!("Quest complete: {name}")),
            Effect::Log(text) => self.events.push(Event::ScriptLog(text)),
            Effect::StartTimer { name, seconds } => self.start_timer(id, name, seconds),
            Effect::Life(change) => self.script_life(id, change),
            Effect::Mana(change) => {
                if let Some(p) = self.players.get_mut(&id) {
                    let mana = (i64::from(p.mana) + change).clamp(0, i64::from(p.stats.max_mana));
                    p.mana = mana as u32;
                }
            }
            Effect::Pk(change) => {
                if let Some(p) = self.players.get_mut(&id) {
                    // TQ keeps PK points within ±30000.
                    p.pk = (i64::from(p.pk) + change).clamp(-30_000, 30_000) as i32;
                }
            }
            Effect::AddStatus(status) => {
                if let Ok(status) = u32::try_from(status) {
                    self.add_status(id, status, None);
                }
            }
            Effect::PlayEffect(effect) => self.events.push(Event::Effect { entity: id, effect }),
            Effect::PlaySound(sound) => self.events.push(Event::Sound { entity: id, sound }),
            Effect::Announce(text) => {
                let players: Vec<EntityId> = self.players.keys().copied().collect();
                for player in players {
                    let text = text.clone();
                    self.events.push(Event::Notice { player, text });
                }
            }
            Effect::OpenWindow(dialog) => {
                if let Ok(dialog) = u32::try_from(dialog) {
                    self.events.push(Event::OpenWindow { player: id, dialog });
                }
            }
            Effect::Spawn { kind, cell } => {
                let cell = (cell.0 as i32, cell.1 as i32);
                if let Ok(kind) = u32::try_from(kind)
                    && self.spawn_monster(kind, cell).is_none()
                {
                    self.events.push(Event::Warning(format!(
                        "A script's monster {kind} can't appear at ({}, {})",
                        cell.0, cell.1
                    )));
                }
            }
            Effect::TeamMessage(text) => {
                if let Some(leader) = self.players.get(&id).and_then(|p| p.team) {
                    self.tell_team(leader, &text);
                }
            }
            Effect::WalkTo { cell, tag } => {
                self.walk(id, (cell.0 as i32, cell.1 as i32));
                if let Some(p) = self.players.get_mut(&id) {
                    p.arrive_tag = Some(tag);
                }
            }
        }
    }

    /// A script healed (`change` > 0) or hurt the player; scripts never kill.
    fn script_life(&mut self, id: EntityId, change: i64) {
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        let before = i64::from(p.life);
        let after = (before + change).clamp(1, i64::from(p.stats.max_life));
        p.life = after as u32;
        if after > before {
            let amount = (after - before) as u32;
            self.events.push(Event::Healed { target: id, amount });
        }
    }

    /// The player as scripts see it.
    pub(crate) fn player_state(&self, id: EntityId) -> PlayerState {
        let Some(p) = self.players.get(&id) else {
            return PlayerState::default();
        };
        let mut items = BTreeMap::new();
        for item in &p.bag {
            *items.entry(i64::from(item.kind)).or_insert(0) += i64::from(item.count);
        }
        PlayerState {
            name: p.name.clone(),
            level: p.level.into(),
            money: p.money as i64,
            items,
            skills: p.skills.iter().map(|&s| i64::from(s)).collect(),
            wallet: (p.wallet.iter())
                .map(|(k, &v)| (k.clone(), i64::try_from(v).unwrap_or(i64::MAX)))
                .collect(),
            flags: p.flags.clone(),
            world: self.world_flags.clone(),
            statuses: p.statuses.iter().map(|s| i64::from(s.id)).collect(),
            map: self.map_ids.first().copied().map_or(0, i64::from),
            cell: (i64::from(p.cell.0), i64::from(p.cell.1)),
            life: p.life.into(),
            max_life: p.stats.max_life.into(),
            team_size: self.team_members(id).len() as i64,
            team_leader: (p.team)
                .and_then(|l| self.players.get(&l))
                .map(|l| l.name.clone())
                .unwrap_or_default(),
            profession: p.profession.into(),
            pk: p.pk.into(),
            worn: (p.equipment.iter())
                .filter_map(|item| {
                    let slot = self.rules.item(item.kind)?.slot?;
                    Some((i64::from(slot), i64::from(item.kind)))
                })
                .collect(),
            worn_attrs: (p.equipment.iter())
                .filter_map(|item| {
                    let slot = self.rules.item(item.kind)?.slot?;
                    let names = self.settings.item_attributes.iter();
                    let attrs = (names.zip(item.attrs))
                        .map(|(name, value)| (name.clone(), i64::from(value)))
                        .collect();
                    Some((i64::from(slot), attrs))
                })
                .collect(),
            attributes: (self.settings.attributes.iter())
                .map(|a| (a.name.clone(), a.default))
                .chain(p.attributes.iter().map(|(k, &v)| (k.clone(), v)))
                .collect(),
            attribute_limits: (self.settings.attributes.iter())
                .map(|a| (a.name.clone(), (a.min, a.max)))
                .collect(),
            record: p
                .record
                .map(|r| (i64::from(r.map), (i64::from(r.cell.0), i64::from(r.cell.1)))),
            bag_space: self.bag_size().saturating_sub(p.bag.len()) as i64,
            mana: p.mana.into(),
            max_mana: p.stats.max_mana.into(),
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
        // Attributes at their default aren't kept.
        let defaults: BTreeMap<&str, i64> = (self.settings.attributes.iter())
            .map(|a| (a.name.as_str(), a.default))
            .collect();
        p.attributes = (after.attributes.iter())
            .filter(|(k, v)| defaults.get(k.as_str()).copied().unwrap_or(0) != **v)
            .map(|(k, &v)| (k.clone(), v))
            .collect();
        p.record = after.record.map(|(map, cell)| c3_scene::PortalTarget {
            map: u32::try_from(map).unwrap_or(0),
            cell: (cell.0 as i32, cell.1 as i32),
        });
        let names = self.settings.item_attributes.clone();
        for item in &mut p.equipment {
            let Some(slot) = self.rules.item(item.kind).and_then(|t| t.slot) else {
                continue;
            };
            let Some(attrs) = after.worn_attrs.get(&i64::from(slot)) else {
                continue;
            };
            for (i, name) in names.iter().enumerate().take(item.attrs.len()) {
                if let Some(&v) = attrs.get(name) {
                    item.attrs[i] = v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
                }
            }
        }
        p.skills.retain(|s| after.skills.contains(&i64::from(*s)));
        self.world_flags = after.world.clone();
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        p.wallet = (after.wallet.iter())
            .filter(|(_, v)| **v > 0)
            .map(|(k, &v)| (k.clone(), v as u64))
            .collect();
        for &skill in &after.skills {
            if let Ok(skill) = u32::try_from(skill) {
                self.learn_skill(id, skill);
            }
        }
        let kinds: BTreeSet<i64> = before
            .items
            .keys()
            .chain(after.items.keys())
            .copied()
            .collect();
        for kind in kinds {
            let Ok(k) = u32::try_from(kind) else { continue };
            let change = after.item_count(kind) - before.item_count(kind);
            let count = u32::try_from(change.unsigned_abs()).unwrap_or(u32::MAX);
            if change > 0 {
                self.give_items(id, k, count);
            } else if change < 0 {
                self.take_items(id, k, count);
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

/// A notice of `amount` of `currency` received (or paid, when negative).
fn money_text(amount: i64, currency: &str) -> String {
    if amount >= 0 {
        format!("You received {amount} {currency}.")
    } else {
        format!("You paid {} {currency}.", -amount)
    }
}
