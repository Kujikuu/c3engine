//! Shows a simulation ([`c3_game::Sim`]) in a [`World`]: an actor for each player and
//! monster, moved, animated and removed as the simulation's events say. Ground items are
//! left to the host's overlay. The game client and the Studio's Play mode both use it.

use crate::actor::Actor;
use crate::character::{DEFAULT_HERO, Look, action};
use crate::world::World;
use c3_game::{EntityId, Event, Sim};
use glam::Vec2;
use std::collections::HashMap;

/// Action of a dying role (`_ACTION_DIE`).
const DIE: u32 = 330;
/// Clickable box of a role around its feet, in world pixels.
const PICK_HALF_WIDTH: f32 = 32.0;
const PICK_HEIGHT: f32 = 100.0;

#[derive(Default)]
pub struct SimView {
    /// Actor index of each player and monster.
    actors: HashMap<EntityId, usize>,
}

impl SimView {
    /// Makes the world's actors: `hero` first (the world's hero), then the scene's NPCs, then
    /// every other player and monster.
    pub fn attach(sim: &Sim, hero: EntityId, world: &mut World) -> Result<Self, String> {
        let mut view = Self::default();
        let cell = sim
            .player(hero)
            .ok_or("the hero is not in the simulation")?
            .cell;
        world.actors.clear();
        world.actors.push(Actor::from_look(
            &world.content,
            &DEFAULT_HERO,
            action::STANDBY,
            cell,
            0,
        )?);
        view.actors.insert(hero, 0);
        world.add_npcs();
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

    /// Plays `events` (from [`Sim::take_events`]) on the world's actors.
    pub fn apply(&mut self, events: &[Event], sim: &Sim, world: &mut World) {
        for event in events {
            match *event {
                Event::Appeared(id) => self.appear(sim, id, world),
                Event::Vanished(id) => self.vanish(id, world),
                Event::Moved { id, to, running } => {
                    if let Some(a) = self.actor_mut(id, world) {
                        a.running = running;
                        let scene = &world.scene;
                        world.actors[self.actors[&id]].walk_to(to, scene);
                    }
                }
                Event::Placed { id, cell } => {
                    if let Some(a) = self.actor_mut(id, world) {
                        a.place(cell);
                    }
                }
                Event::Attacked {
                    attacker, target, ..
                } => {
                    let goal = sim.cell_of(target);
                    if let (Some(i), Some(goal)) = (self.actor(attacker), goal) {
                        world.actors[i].face(goal);
                        world.actor_action(i, action::ATTACK_0, true);
                    }
                }
                Event::Died { id, .. } => {
                    if let Some(i) = self.actor(id) {
                        world.actor_hold(i, DIE);
                    }
                }
                Event::Revived(id) => {
                    if let Some(i) = self.actor(id) {
                        world.actor_action(i, action::STANDBY, false);
                    }
                }
                _ => {}
            }
        }
    }

    fn actor_mut<'w>(&self, id: EntityId, world: &'w mut World) -> Option<&'w mut Actor> {
        world.actors.get_mut(*self.actors.get(&id)?)
    }

    fn appear(&mut self, sim: &Sim, id: EntityId, world: &mut World) {
        if self.actors.contains_key(&id) {
            return;
        }
        let made = if let Some(m) = sim.monster(id) {
            let Some(kind) = sim.rules().monster(m.kind) else {
                return;
            };
            let look = Look {
                look: kind.look % 1000,
                ..Default::default()
            };
            Actor::from_look(&world.content, &look, action::STANDBY, m.cell, m.direction).map(
                |mut actor| {
                    actor.zoom_percent = world.content.monster_zoom(m.kind);
                    actor
                },
            )
        } else if let Some(p) = sim.player(id) {
            Actor::from_look(
                &world.content,
                &DEFAULT_HERO,
                action::STANDBY,
                p.cell,
                p.direction,
            )
        } else {
            return; // ground items are drawn by the host
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
        let last = world.actors.len() - 1;
        world.remove_actor(index);
        // The last actor moved into the freed place.
        if index != last
            && let Some(moved) = self.actors.values_mut().find(|i| **i == last)
        {
            *moved = index;
        }
    }

    /// The living monster drawn under world point `point` (the nearest when several are).
    pub fn monster_at(&self, sim: &Sim, world: &World, point: Vec2) -> Option<EntityId> {
        let now = world.now_ms();
        sim.monsters()
            .filter(|m| m.is_alive())
            .filter_map(|m| {
                let feet = world
                    .actors
                    .get(self.actor(m.id)?)?
                    .position(&world.scene, now);
                let d = point - feet;
                let hit = d.x.abs() <= PICK_HALF_WIDTH && (-PICK_HEIGHT..=16.0).contains(&d.y);
                hit.then(|| {
                    (
                        m.id,
                        (point - (feet - Vec2::Y * PICK_HEIGHT / 2.0)).length(),
                    )
                })
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
