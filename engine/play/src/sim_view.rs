//! Shows a simulation ([`c3_game::Sim`]) in a [`World`]: an actor for each player and
//! monster, moved, animated, dressed and removed as the simulation's events say. Ground
//! items are left to the UI's overlay. The game client and the Studio's Play mode use it.

use crate::actor::Actor;
use crate::module::Act;
use crate::world::World;
use c3_game::{EntityId, Event, Sim};
use c3_scene::CellPos;
use glam::Vec2;
use std::collections::HashMap;
/// Clickable box of a role around its feet, in world pixels.
const PICK_HALF_WIDTH: f32 = 32.0;
const PICK_HEIGHT: f32 = 100.0;

#[derive(Default)]
pub struct SimView {
    /// Actor index of each player and monster.
    actors: HashMap<EntityId, usize>,
    /// Actor index of each NPC.
    npcs: HashMap<u32, usize>,
}

impl SimView {
    /// Makes the world's actors: `hero` first (the world's hero), then the scene's NPCs, then
    /// every other player and monster.
    pub fn attach(sim: &Sim, hero: EntityId, world: &mut World) -> Result<Self, String> {
        let mut view = Self::default();
        let p = sim
            .player(hero)
            .ok_or("the hero is not in the simulation")?;
        let looks = world.game.looks();
        let stand = looks.action(Act::Stand);
        let hero_actor = Actor::from_look(&*world.game, &looks.player(p), stand, p.cell, 0)?;
        world.actors.clear();
        world.actors.push(hero_actor);
        view.actors.insert(hero, 0);
        for npc in sim.npcs() {
            if let Some(index) = world.add_npc(npc) {
                view.npcs.insert(npc.id, index);
            }
        }
        let others: Vec<EntityId> = (sim.players().map(|p| p.id))
            .chain(sim.monsters().map(|m| m.id))
            .filter(|&id| id != hero)
            .collect();
        for id in others {
            view.appear(sim, id, world);
        }
        Ok(view)
    }

    /// The actor showing `id`.
    pub fn actor(&self, id: EntityId) -> Option<usize> {
        self.actors.get(&id).copied()
    }

    /// A cast: the caster turns to where it lands and strikes; the skill's effect plays on
    /// each one it reaches, or where it lands when it reached no one.
    fn show_cast(
        &self,
        caster: EntityId,
        skill: u32,
        cell: CellPos,
        targets: &[EntityId],
        sim: &Sim,
        world: &mut World,
    ) {
        if let Some(i) = self.actor(caster) {
            world.actors[i].face(cell);
            let attack = world.game.looks().action(Act::Attack);
            world.actor_action(i, attack, true);
        }
        let Some(t) = sim.rules().skill(skill) else {
            return;
        };
        let Some(effect) = world.game.skill_media(t).0 else {
            return;
        };
        // Decoration: a client without the effect still plays.
        if targets.is_empty() {
            let _ = world.place_effect(&effect, cell, false);
        }
        for i in targets.iter().filter_map(|&t| self.actor(t)) {
            let _ = world.actor_effect(i, &effect, false);
        }
    }

    /// Plays `events` (from [`Sim::take_events`]) on the world's actors.
    pub fn apply(&mut self, events: &[Event], sim: &Sim, world: &mut World) {
        for event in events {
            match *event {
                Event::Appeared(id) => self.appear(sim, id, world),
                Event::Vanished(id) => self.vanish(id, world),
                Event::Moved { id, to, running } => {
                    if let Some(i) = self.actor(id) {
                        world.actors[i].running = running;
                        let scene = &world.scene;
                        world.actors[i].walk_to(to, scene);
                    }
                }
                Event::Emoted { player, emote } => {
                    if let Some(i) = self.actor(player) {
                        use crate::module::EmotePlay;
                        match world.game.looks().emote_play(emote) {
                            EmotePlay::Once => world.actor_action(i, emote, true),
                            EmotePlay::Hold => world.actor_hold(i, emote),
                            EmotePlay::Loop => world.actor_action(i, emote, false),
                        }
                    }
                }
                Event::Placed { id, cell } | Event::Jumped { id, to: cell } => {
                    if let Some(i) = self.actor(id) {
                        world.actors[i].place(cell);
                    }
                }
                Event::Attacked {
                    attacker, target, ..
                } => {
                    let goal = sim.cell_of(target);
                    if let (Some(i), Some(goal)) = (self.actor(attacker), goal) {
                        world.actors[i].face(goal);
                        let attack = world.game.looks().action(Act::Attack);
                        world.actor_action(i, attack, true);
                    }
                }
                Event::Cast {
                    caster,
                    skill,
                    cell,
                    ref targets,
                } => self.show_cast(caster, skill, cell, targets, sim, world),
                Event::StatusStarted { target, status } | Event::StatusEnded { target, status } => {
                    let effect =
                        (sim.rules().status(status)).and_then(|t| world.game.status_effect(t));
                    if let (Some(i), Some(effect)) = (self.actor(target), effect) {
                        world.actors[i].remove_effect(&effect);
                        if matches!(event, Event::StatusStarted { .. }) {
                            // Decoration: a client without the effect still plays.
                            let _ = world.actor_effect(i, &effect, true);
                        }
                    }
                }
                Event::CellEffect { cell, ref effect } => {
                    // Decoration: a client without the effect still plays.
                    let _ = world.place_effect(effect, cell, false);
                }
                Event::Effect { entity, ref effect } => {
                    if let Some(i) = self.actor(entity) {
                        // Decoration: a client without the effect still plays.
                        let _ = world.actor_effect(i, effect, false);
                    }
                }
                Event::Died { id, .. } => {
                    if let Some(i) = self.actor(id) {
                        let die = world.game.looks().action(Act::Die);
                        world.actor_hold(i, die);
                    }
                }
                Event::Revived(id) => {
                    if let Some(i) = self.actor(id) {
                        let stand = world.game.looks().action(Act::Stand);
                        world.actor_action(i, stand, false);
                    }
                }
                Event::Dressed { player } => self.dress(sim, player, world),
                Event::NpcMoved { .. } | Event::NpcArrived(_) => self.npc_event(event, world),
                _ => {}
            }
        }
    }

    /// Gives player `id`'s actor the look of what it wears.
    fn dress(&mut self, sim: &Sim, id: EntityId, world: &mut World) {
        let (Some(i), Some(p)) = (self.actor(id), sim.player(id)) else {
            return;
        };
        let a = &world.actors[i];
        let (cell, direction) = (a.cell, a.direction);
        let looks = world.game.looks();
        let stand = looks.action(Act::Stand);
        match Actor::from_look(&*world.game, &looks.player(p), stand, cell, direction) {
            Ok(actor) => world.replace_actor(i, actor),
            Err(e) => eprintln!("warning: can't dress {id:?}: {e}"),
        }
    }

    fn appear(&mut self, sim: &Sim, id: EntityId, world: &mut World) {
        if self.actors.contains_key(&id) {
            return;
        }
        let looks = world.game.looks();
        let stand = looks.action(Act::Stand);
        let made = if let Some(m) = sim.monster(id) {
            if sim.rules().monster(m.kind).is_none() {
                return;
            }
            let look = looks.monster(m.kind, sim.rules());
            Actor::from_look(&*world.game, &look, stand, m.cell, m.direction)
        } else if let Some(p) = sim.player(id) {
            Actor::from_look(&*world.game, &looks.player(p), stand, p.cell, p.direction)
        } else {
            return; // ground items are drawn by the UI
        };
        match made {
            Ok(actor) => {
                world.actors.push(actor);
                self.actors.insert(id, world.actors.len() - 1);
            }
            Err(e) => eprintln!("warning: can't show {id:?}: {e}"),
        }
    }

    fn vanish(&mut self, id: EntityId, world: &mut World) {
        let Some(index) = self.actors.remove(&id) else {
            return;
        };
        self.remove_actor(index, world);
    }

    /// Removes actor `index`; the actor that took its place (the last) is followed.
    fn remove_actor(&mut self, index: usize, world: &mut World) {
        let last = world.actors.len() - 1;
        world.remove_actor(index);
        if index == last {
            return;
        }
        let moved = (self.actors.values_mut())
            .chain(self.npcs.values_mut())
            .find(|i| **i == last);
        if let Some(moved) = moved {
            *moved = index;
        }
    }

    /// An NPC moved, left or came.
    fn npc_event(&mut self, event: &Event, world: &mut World) {
        match event {
            Event::NpcMoved {
                npc,
                cell: Some(cell),
            } => {
                if let Some(&i) = self.npcs.get(npc) {
                    world.actors[i].place(*cell);
                }
            }
            Event::NpcMoved { npc, cell: None } => {
                if let Some(i) = self.npcs.remove(npc) {
                    self.remove_actor(i, world);
                }
            }
            Event::NpcArrived(npc) => {
                if let Some(i) = self.npcs.remove(&npc.id) {
                    self.remove_actor(i, world);
                }
                if let Some(i) = world.add_npc(npc) {
                    self.npcs.insert(npc.id, i);
                }
            }
            _ => {}
        }
    }

    /// Where `id`'s actor's feet are now (between cells while it steps).
    pub fn feet(&self, world: &World, id: EntityId) -> Option<Vec2> {
        let actor = world.actors.get(self.actor(id)?)?;
        Some(actor.position(&world.scene, world.now_ms()))
    }

    /// The living monster drawn under world point `point` (the nearest when several are).
    pub fn monster_at(&self, sim: &Sim, world: &World, point: Vec2) -> Option<EntityId> {
        let monsters = (sim.monsters()).filter(|m| m.is_alive()).map(|m| m.id);
        // On PK maps other players are foes too.
        let players = (sim.players())
            .filter(|p| sim.pk() && self.actor(p.id) != Some(0) && p.is_alive())
            .map(|p| p.id);
        self.nearest_at(world, point, monsters.chain(players))
    }

    /// The living player other than the hero drawn under `point`.
    pub fn player_at(&self, sim: &Sim, world: &World, point: Vec2) -> Option<EntityId> {
        let players = (sim.players())
            .filter(|p| self.actor(p.id) != Some(0) && p.is_alive())
            .map(|p| p.id);
        self.nearest_at(world, point, players)
    }

    /// Of `ids`, the one drawn under `point` nearest its middle.
    fn nearest_at(
        &self,
        world: &World,
        point: Vec2,
        ids: impl Iterator<Item = EntityId>,
    ) -> Option<EntityId> {
        ids.filter_map(|id| {
            let feet = self.feet(world, id)?;
            let d = point - feet;
            let hit = d.x.abs() <= PICK_HALF_WIDTH && (-PICK_HEIGHT..=16.0).contains(&d.y);
            hit.then(|| (id, (point - (feet - Vec2::Y * PICK_HEIGHT / 2.0)).length()))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
    }

    /// The ground item on the cell under `point`, or on a cell next to it.
    pub fn item_at(&self, sim: &Sim, world: &World, point: Vec2) -> Option<EntityId> {
        let cell = world.scene.world_to_cell((point.x, point.y));
        sim.ground_items()
            .filter(|i| c3_game::distance(i.cell, cell) <= 1)
            .min_by_key(|i| c3_game::distance(i.cell, cell))
            .map(|i| i.id)
    }
}
