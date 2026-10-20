//! A map being played: its scene, drawn by the engine's [`SceneView`] (sky, ground, objects,
//! map effects), and the actors walking on it, as the game ([`GameModule`]) dresses them. The
//! game client and the Studio's Play mode both run it.
//!
//! The host advances the world's clock ([`World::advance`], so it can pause and step), then
//! each frame [`World::prepare`] builds the draw lists with the actors posed into them and
//! [`World::mesh_draws`] + [`World::frame`] describe it to the renderer.

use crate::actor::{self, Actor, ActorPart};
use crate::module::GameModule;
use c3_render::camera::Camera2d;
use c3_render::effect::camera_eye;
use c3_render::scene_view::{ActorSpot, SceneView};
use c3_render::{Frame, Light, MeshDraw, Renderer, View};
use c3_scene::Scene;
use glam::Vec2;
use std::rc::Rc;

/// Clickable box of an NPC around its feet, in world pixels.
const NPC_PICK_HALF_WIDTH: f32 = 32.0;
const NPC_PICK_HEIGHT: f32 = 110.0;

pub struct World {
    pub game: Rc<dyn GameModule>,
    pub map_path: String,
    pub scene: Scene,
    pub actors: Vec<Actor>,
    /// Removed actors whose textures are freed on the next prepare.
    retired: Vec<Actor>,
    view: SceneView<ActorPart>,
    /// Game time in milliseconds: the clock for animation and movement.
    clock_ms: f64,
    /// Colour (linear) around and behind the map.
    pub background: [f64; 4],
    pub light: Light,
    /// The game's role shadow, uploaded (`None` until the first prepare) with its size.
    shadow: Option<Option<(c3_render::TextureId, Vec2)>>,
}

/// The scene light `graphics` asks for.
pub fn scene_light(graphics: &c3_core::settings::GraphicsSettings) -> Light {
    let [r, g, b, _] = linear_background(graphics.light);
    Light {
        ambient: [r as f32, g as f32, b as f32],
        shade_models: graphics.shade_models,
        ..Light::default()
    }
}

/// An sRGB colour as the renderer's linear clear colour.
pub fn linear_background(srgb: [u8; 3]) -> [f64; 4] {
    let lin = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    [lin(srgb[0]), lin(srgb[1]), lin(srgb[2]), 1.0]
}

impl World {
    /// A world of `scene` (map `map_path` of `game`), its 3D roles and effects tilted by
    /// `tilt_deg`.
    pub fn new(game: Rc<dyn GameModule>, map_path: String, scene: Scene, tilt_deg: f32) -> Self {
        let view = SceneView::new(&scene, game.assets(), tilt_deg, 0);
        let mut world = Self {
            game,
            map_path,
            scene,
            actors: Vec::new(),
            retired: Vec::new(),
            view,
            clock_ms: 0.0,
            background: linear_background(
                c3_core::settings::GraphicsSettings::default().background,
            ),
            light: Light::default(),
            shadow: None,
        };
        world.mark_portals();
        world
    }

    /// Shows the game's portal effect on every portal (decoration: a game without one, or a
    /// client missing it, still plays).
    fn mark_portals(&mut self) {
        let Some(effect) = self.game.portal_effect() else {
            return;
        };
        let cells: Vec<_> = self.scene.portals.iter().map(|p| p.cell).collect();
        for cell in cells {
            let _ = self.place_effect(&effect, cell, true);
        }
    }

    /// Moves game time on by `seconds` (the host's frame time; 0 while paused).
    pub fn advance(&mut self, seconds: f32) {
        self.clock_ms += f64::from(seconds.max(0.0)) * 1000.0;
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
            .place_effect(name, cell, forever, &self.scene, self.game.assets(), now_ms)
    }

    /// Plays effect `name` at the hero's feet, once or `forever`.
    pub fn hero_effect(&mut self, name: &str, forever: bool) -> Result<(), String> {
        self.actor_effect(0, name, forever)
    }

    /// Plays effect `name` on actor `index`, once or `forever`.
    pub fn actor_effect(&mut self, index: usize, name: &str, forever: bool) -> Result<(), String> {
        let now_ms = self.now_ms();
        let effect = self
            .view
            .library
            .start(name, self.game.assets(), now_ms)
            .ok_or_else(|| format!("effect {name} could not be loaded"))?;
        let actor = self.actors.get_mut(index).ok_or("there is no such actor")?;
        actor.add_effect(if forever { effect.endless() } else { effect });
        Ok(())
    }

    /// The hero starts action `act`: looping, or `once` then standing.
    pub fn hero_action(&mut self, act: u32, once: bool) {
        self.actor_action(0, act, once);
    }

    /// Actor `index` starts action `act`: looping, or `once` then standing.
    pub fn actor_action(&mut self, index: usize, act: u32, once: bool) {
        let now_ms = self.now_ms();
        let Some(actor) = self.actors.get_mut(index) else {
            return;
        };
        if once {
            actor.perform(act, now_ms, &*self.game, &mut self.view.library);
        } else {
            actor.play(act, now_ms, &*self.game, &mut self.view.library);
        }
    }

    /// Actor `index` jumps to `to` along its arc.
    pub fn actor_jump(&mut self, index: usize, to: c3_scene::CellPos) {
        let now_ms = self.now_ms();
        if let Some(actor) = self.actors.get_mut(index) {
            actor.jump(to, now_ms, &self.scene, &*self.game, &mut self.view.library);
        }
    }

    /// Actor `index` plays action `act` once and stays on its last frame (dying).
    pub fn actor_hold(&mut self, index: usize, act: u32) {
        let now_ms = self.now_ms();
        if let Some(actor) = self.actors.get_mut(index) {
            actor.play_and_hold(act, now_ms, &*self.game, &mut self.view.library);
        }
    }

    /// Puts `actor` in place of actor `index` (a new look).
    pub fn replace_actor(&mut self, index: usize, actor: Actor) {
        if let Some(old) = self.actors.get_mut(index) {
            self.retired.push(std::mem::replace(old, actor));
        }
    }

    /// Removes actor `index` (the last actor takes its place, as `Vec::swap_remove`).
    pub fn remove_actor(&mut self, index: usize) {
        if index < self.actors.len() {
            self.retired.push(self.actors.swap_remove(index));
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

    /// Game time in milliseconds: the clock for animation and movement.
    pub fn now_ms(&self) -> u64 {
        self.clock_ms as u64
    }

    /// Advances actors (movement, actions) to now and drops finished effects.
    pub fn tick(&mut self) {
        let now = self.now_ms();
        let mut left = Vec::new();
        for actor in &mut self.actors {
            actor.update(now, &self.scene, &*self.game, &mut self.view.library);
            for mark in actor.take_map_marks() {
                let terrain = self.scene.grid.get(mark.cell).map_or(0, |c| c.surface);
                let looks = self.game.looks();
                if let Some(effect) = looks.map_effect(actor.look(), mark.act, terrain, mark.at_end)
                {
                    left.push((effect, mark.cell));
                }
            }
        }
        for (effect, cell) in left {
            let _ = self.place_effect(&effect, cell, false);
        }
        self.view.tick(now);
        for error in self.view.library.take_errors() {
            eprintln!("{error}");
        }
    }

    /// An actor for each of the scene's NPCs (after the hero), each look loaded once.
    pub fn add_npcs(&mut self) {
        let npcs = self.scene.npcs.clone();
        for npc in &npcs {
            self.add_npc(npc);
        }
    }

    /// An actor for `npc` playing its stand-by; its index, or `None` when its look can't be
    /// made (said on the console).
    pub fn add_npc(&mut self, npc: &c3_scene::Npc) -> Option<usize> {
        let looks = self.game.looks();
        let stand = looks.action(crate::Act::Stand);
        let look = looks.npc(npc);
        match looks.character(&look, stand) {
            Ok(c) => {
                let actor = Actor::new(&*self.game, c, look, npc.cell, npc.direction);
                self.actors.push(actor);
                Some(self.actors.len() - 1)
            }
            Err(e) => {
                eprintln!("warning: NPC {} ({}): {e}", npc.name, npc.id);
                None
            }
        }
    }

    /// Stops every effect: the map's and those the actors carry.
    /// Frees the world's GPU textures; call before dropping it.
    pub fn release(&mut self, renderer: &mut c3_render::Renderer) {
        for actor in self.actors.iter_mut().chain(&mut self.retired) {
            actor.release_textures(renderer);
        }
        self.view.release(renderer);
    }

    /// Reads the game's art again (the project's assets changed on disk); effects playing on
    /// roles stop.
    pub fn reload_assets(&mut self, renderer: &mut c3_render::Renderer) {
        let now_ms = self.now_ms();
        for actor in &mut self.actors {
            actor.clear_effects();
        }
        self.view
            .reload_assets(&self.scene, self.game.assets(), renderer, now_ms);
        self.mark_portals();
    }

    pub fn clear_effects(&mut self) {
        self.view.clear_effects();
        for actor in &mut self.actors {
            actor.clear_effects();
        }
    }

    /// The scene NPC drawn under world point `point` (the nearest when several are), from
    /// its feet up to about its head.
    pub fn npc_at(&self, point: Vec2) -> Option<&c3_scene::Npc> {
        self.scene
            .npcs
            .iter()
            .filter_map(|npc| {
                let feet = Vec2::from(self.scene.cell_to_world(npc.cell));
                let d = point - feet;
                let hit =
                    d.x.abs() <= NPC_PICK_HALF_WIDTH && (-NPC_PICK_HEIGHT..=16.0).contains(&d.y);
                hit.then(|| {
                    (
                        npc,
                        (point - (feet - Vec2::Y * NPC_PICK_HEIGHT / 2.0)).length(),
                    )
                })
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(npc, _)| npc)
    }

    /// The player's character: the first actor.
    pub fn hero(&self) -> Option<&Actor> {
        self.actors.first()
    }

    pub fn hero_mut(&mut self) -> Option<&mut Actor> {
        self.actors.first_mut()
    }

    /// Sends the hero towards world point `target`, marking a walkable target cell with the
    /// game's click effect.
    pub fn walk_hero_to(&mut self, target: Vec2) {
        let cell = self.scene.world_to_cell((target.x, target.y));
        let Some(hero) = self.actors.first_mut() else {
            return;
        };
        hero.walk_to(cell, &self.scene);
        self.mark_click(cell);
    }

    /// Marks walkable `cell` with the game's click effect (decoration: a game without one
    /// still walks).
    pub fn mark_click(&mut self, cell: c3_scene::CellPos) {
        if self.scene.grid.get(cell).is_some_and(|c| !c.is_blocked())
            && let Some(effect) = self.game.click_effect()
        {
            let _ = self.place_effect(&effect, cell, false);
        }
    }

    /// Builds this frame's sprites, posed actors and painter's order.
    pub fn prepare(&mut self, renderer: &mut Renderer, camera: &Camera2d, viewport: Vec2) {
        for mut actor in self.retired.drain(..) {
            actor.release_textures(renderer);
        }
        let view = View {
            rect: camera.view_rect(viewport),
            center: (camera.center.x, camera.center.y),
            zoom: camera.zoom,
            time_ms: self.now_ms(),
        };
        let now = view.time_ms;
        let game = &*self.game;
        let shadow = *self.shadow.get_or_insert_with(|| {
            let image = game.role_shadow()?;
            let size = Vec2::new(image.width as f32, image.height as f32);
            Some((renderer.upload_texture(&image), size))
        });
        let spots: Vec<ActorSpot> = (self.actors.iter())
            .map(|a| ActorSpot {
                cell: a.draw_cell(&self.scene, now),
                shadow: shadow.and_then(|(texture, size)| {
                    Some(c3_render::Sprite {
                        rect: a.shadow(&self.scene, now, size)?,
                        uv: [0.0, 0.0, 1.0, 1.0],
                        texture: Some(texture),
                        tint: [1.0, 1.0, 1.0, 0.7],
                    })
                }),
            })
            .collect();
        let (scene, actors) = (&self.scene, &mut self.actors);
        let tilt_deg = self.view.tilt_deg();
        let eye = camera_eye(view.center);
        self.view
            .prepare(scene, game.assets(), renderer, &view, &spots, |a, poser| {
                actor::pose_actor(actors, a, poser, scene, game, tilt_deg, now, eye);
            });
    }

    /// Mesh draws for the posed actors and effects (borrowing this world).
    pub fn mesh_draws(&self) -> Vec<MeshDraw<'_>> {
        self.view
            .mesh_draws(|p| ActorPart::indices(&self.actors, p))
    }

    /// The frame description for `meshes` (from [`World::mesh_draws`]) seen through `rect`.
    pub fn frame<'a>(&'a self, meshes: &'a [MeshDraw<'a>], rect: [f32; 4]) -> Frame<'a> {
        Frame {
            light: self.light,
            ..self.view.frame(meshes, rect, self.background)
        }
    }
}
