//! The loaded world: current map scene, drawn by the engine's [`SceneView`] (sky, ground,
//! objects, map effects), and the actors walking on it.
//!
//! Each frame: [`World::prepare`] builds the draw lists with the actors posed into them;
//! [`World::mesh_draws`] + [`World::frame`] then describe it to the renderer.

use crate::camera::Camera2d;
use c3_render::effect::camera_eye;
use c3_render::scene_view::SceneView;
use c3_render::{Frame, MeshDraw, Renderer, View};
use c3_scene::Scene;
use game_conquer::Content;
use game_conquer::actor::{self, Actor, ActorPart};
use glam::Vec2;
use std::time::Instant;

/// Clear colour (linear) behind the map.
const BACKGROUND: [f64; 4] = [0.004, 0.004, 0.005, 1.0];

/// Effect marking where the player clicked to walk.
const CLICK_EFFECT: &str = "mouse";

pub struct World {
    pub content: Content,
    pub map_path: String,
    pub scene: Scene,
    pub actors: Vec<Actor>,
    view: SceneView<ActorPart>,
    /// Clock for sprite and motion animation.
    started: Instant,
}

impl World {
    pub fn new(content: Content, map_path: String, scene: Scene) -> Self {
        let view = SceneView::new(&scene, &content, content.oblique_angle(), 0);
        Self {
            content,
            map_path,
            scene,
            actors: Vec::new(),
            view,
            started: Instant::now(),
        }
    }

    /// Places effect `name` on `cell`, playing once or `forever`.
    pub fn place_effect(
        &mut self,
        name: &str,
        cell: c3_scene::CellPos,
        forever: bool,
    ) -> Result<(), String> {
        let now_ms = self.now_ms();
        self.view
            .place_effect(name, cell, forever, &self.scene, &self.content, now_ms)
    }

    /// Plays effect `name` at the hero's feet, once or `forever`.
    pub fn hero_effect(&mut self, name: &str, forever: bool) -> Result<(), String> {
        let now_ms = self.now_ms();
        let effect = self
            .view
            .library
            .start(name, &self.content, now_ms)
            .ok_or_else(|| format!("effect {name} could not be loaded"))?;
        let hero = self.actors.first_mut().ok_or("there is no hero")?;
        hero.add_effect(if forever { effect.endless() } else { effect });
        Ok(())
    }

    /// The hero starts action `act`: looping, or `once` then standing.
    pub fn hero_action(&mut self, act: u32, once: bool) {
        let now_ms = self.now_ms();
        let Some(hero) = self.actors.first_mut() else {
            return;
        };
        if once {
            hero.perform(act, now_ms, &self.content, &mut self.view.library);
        } else {
            hero.play(act, now_ms, &self.content, &mut self.view.library);
        }
    }

    /// World rect of the map's ground (`[x, y, w, h]`), for framing.
    pub fn bounds(&self) -> [f32; 4] {
        match self.scene.ground.as_ref().filter(|g| g.width > 0) {
            Some(g) => {
                let (w, h) = g.world_size();
                [g.origin.0 as f32, g.origin.1 as f32, w as f32, h as f32]
            }
            None => {
                let p = self.scene.projection;
                let w = self.scene.grid.width.max(self.scene.grid.height) as f32 * p.cell_w;
                [0.0, 0.0, w, w * p.cell_h / p.cell_w]
            }
        }
    }

    /// Milliseconds since the world was loaded: the clock for animation and movement.
    pub fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// Advances actors (movement, actions) to now and drops finished effects.
    pub fn tick(&mut self) {
        let now = self.now_ms();
        for actor in &mut self.actors {
            actor.update(now, &self.scene, &self.content, &mut self.view.library);
        }
        self.view.tick(now);
        for error in self.view.library.take_errors() {
            eprintln!("{error}");
        }
    }

    /// An actor for each of the scene's NPCs (after the hero), each look loaded once.
    pub fn add_npcs(&mut self) {
        let mut characters = std::collections::HashMap::new();
        for npc in &self.scene.npcs {
            let character = characters
                .entry(npc.look)
                .or_insert_with(|| self.content.npc_character(npc.look));
            match character {
                Ok(c) => {
                    self.actors
                        .push(Actor::from_character(c.clone(), npc.cell, npc.direction))
                }
                Err(e) => eprintln!("warning: NPC {} ({}): {e}", npc.name, npc.id),
            }
        }
    }

    /// The player's character: the first actor.
    pub fn hero(&self) -> Option<&Actor> {
        self.actors.first()
    }

    pub fn hero_mut(&mut self) -> Option<&mut Actor> {
        self.actors.first_mut()
    }

    /// Sends the hero towards world point `target`, marking a walkable target cell with the
    /// click effect (`CMyShellDlg::SetMouseMove`).
    pub fn walk_hero_to(&mut self, target: Vec2) {
        let cell = self.scene.world_to_cell((target.x, target.y));
        let Some(hero) = self.actors.first_mut() else {
            return;
        };
        hero.walk_to(cell, &self.scene);
        if self.scene.grid.get(cell).is_some_and(|c| !c.is_blocked()) {
            // The marker is decoration: a client without it still walks.
            let _ = self.place_effect(CLICK_EFFECT, cell, false);
        }
    }

    /// Builds this frame's sprites, posed actors and painter's order.
    pub fn prepare(&mut self, renderer: &mut Renderer, camera: &Camera2d, viewport: Vec2) {
        let view = View {
            rect: camera.view_rect(viewport),
            center: (camera.center.x, camera.center.y),
            zoom: camera.zoom,
            time_ms: self.now_ms(),
        };
        let now = view.time_ms;
        let cells: Vec<_> = (self.actors.iter())
            .map(|a| a.draw_cell(&self.scene, now))
            .collect();
        let (scene, content, actors) = (&self.scene, &self.content, &mut self.actors);
        let tilt_deg = self.view.tilt_deg();
        let eye = camera_eye(view.center);
        self.view
            .prepare(scene, content, renderer, &view, &cells, |a, poser| {
                actor::pose_actor(actors, a, poser, scene, content, tilt_deg, now, eye);
            });
    }

    /// Mesh draws for the posed actors and effects (borrowing this world).
    pub fn mesh_draws(&self) -> Vec<MeshDraw<'_>> {
        self.view
            .mesh_draws(|p| ActorPart::indices(&self.actors, p))
    }

    /// The frame description for `meshes` (from [`World::mesh_draws`]) seen through `rect`.
    pub fn frame<'a>(&'a self, meshes: &'a [MeshDraw<'a>], rect: [f32; 4]) -> Frame<'a> {
        self.view.frame(meshes, rect, BACKGROUND)
    }
}
