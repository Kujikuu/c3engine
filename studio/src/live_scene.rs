//! The open map as the game draws it: the engine's scene view plus an actor (3D model) per
//! NPC. Moving an NPC moves its actor; adding, removing or changing the look of one rebuilds
//! the actors, with each look's model loaded once.

use crate::tq_plugin::Client;
use c3_render::effect::camera_eye;
use c3_render::scene_view::SceneView;
use c3_render::{Frame, MeshDraw, Renderer, View};
use c3_scene::{Npc, Scene};
use game_conquer::actor::{self, Actor, ActorPart};
use game_conquer::character::Character;
use std::collections::{HashMap, HashSet};

/// Background behind the map (linear).
const CLEAR: [f64; 4] = [0.0044, 0.0044, 0.0044, 1.0];

pub struct LiveScene {
    pub view: SceneView<ActorPart>,
    npcs: NpcActors,
}

/// One actor per NPC whose model loads.
#[derive(Default)]
struct NpcActors {
    /// `(id, look)` of each NPC the actors were built for, in scene order.
    built_for: Vec<(u32, u32)>,
    actors: Vec<Actor>,
    /// NPC id of each actor.
    ids: Vec<u32>,
    /// Models by look; `None` when the look has no loadable model.
    characters: HashMap<u32, Option<Character>>,
    /// Load problems not yet reported.
    errors: Vec<String>,
}

impl LiveScene {
    pub fn new(scene: &Scene, client: &Client, now_ms: u64) -> Self {
        Self {
            view: SceneView::new(scene, client, client.tilt_deg(), now_ms),
            npcs: NpcActors::default(),
        }
    }

    /// Frees the GPU textures; call before dropping it.
    pub fn release(&mut self, renderer: &mut Renderer) {
        self.view.release(renderer);
        for a in &mut self.npcs.actors {
            a.release_textures(renderer);
        }
    }

    /// NPCs drawn as models (the rest need a marker).
    pub fn modelled_npcs(&self) -> HashSet<u32> {
        self.npcs.ids.iter().copied().collect()
    }

    /// Load problems since the last call (effects and NPC models), each reported once.
    pub fn take_errors(&mut self) -> Vec<String> {
        let mut errors = self.view.library.take_errors();
        errors.append(&mut self.npcs.errors);
        errors
    }

    /// Builds this frame's draw lists for `view`, with the NPCs posed in.
    pub fn prepare(
        &mut self,
        scene: &Scene,
        client: &Client,
        renderer: &mut Renderer,
        view: &View,
    ) {
        self.npcs.sync(&scene.npcs, client, renderer);
        let now = view.time_ms;
        let cells: Vec<_> = (self.npcs.actors.iter())
            .map(|a| a.draw_cell(scene, now))
            .collect();
        let (actors, content) = (&mut self.npcs.actors, client.content());
        let tilt = self.view.tilt_deg();
        let eye = camera_eye(view.center);
        self.view
            .prepare(scene, client, renderer, view, &cells, |a, poser| {
                actor::pose_actor(actors, a, poser, scene, content, tilt, now, eye);
            });
    }

    pub fn mesh_draws(&self) -> Vec<MeshDraw<'_>> {
        self.view
            .mesh_draws(|key| ActorPart::indices(&self.npcs.actors, key))
    }

    pub fn frame<'a>(&'a self, meshes: &'a [MeshDraw<'a>], rect: [f32; 4]) -> Frame<'a> {
        self.view.frame(meshes, rect, CLEAR)
    }
}

impl NpcActors {
    /// Matches the actors to `npcs`: moves them, or rebuilds them when NPCs were added,
    /// removed or changed look.
    fn sync(&mut self, npcs: &[Npc], client: &Client, renderer: &mut Renderer) {
        let wanted: Vec<(u32, u32)> = npcs.iter().map(|n| (n.id, n.look)).collect();
        if wanted != self.built_for {
            self.rebuild(npcs, client, renderer);
            self.built_for = wanted;
        }
        let by_id: HashMap<u32, &Npc> = npcs.iter().map(|n| (n.id, n)).collect();
        for (actor, id) in self.actors.iter_mut().zip(&self.ids) {
            if let Some(npc) = by_id.get(id) {
                actor.cell = npc.cell;
                actor.direction = npc.direction % 8;
            }
        }
    }

    fn rebuild(&mut self, npcs: &[Npc], client: &Client, renderer: &mut Renderer) {
        for a in &mut self.actors {
            a.release_textures(renderer);
        }
        self.actors.clear();
        self.ids.clear();
        for npc in npcs {
            let character = self.characters.entry(npc.look).or_insert_with(|| {
                match client.content().npc_character(npc.look) {
                    Ok(c) => Some(c),
                    Err(e) => {
                        self.errors
                            .push(format!("NPC {} ({}): {e}", npc.name, npc.id));
                        None
                    }
                }
            });
            if let Some(character) = character {
                self.actors.push(Actor::from_character(
                    character.clone(),
                    npc.cell,
                    npc.direction,
                ));
                self.ids.push(npc.id);
            }
        }
    }
}
