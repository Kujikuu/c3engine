//! The loaded world: current map scene, its streamed art, its effects and the actors on it.
//!
//! Each frame: [`World::prepare`] builds sprites, posed actor and effect meshes and the
//! painter's order; [`World::mesh_draws`] + [`World::frame`] then describe it to the renderer.

use crate::actors::{Actor, PartRef};
use crate::camera::Camera2d;
use crate::effects::{EffectLibrary, MapEffect, Spot};
use c3_assets::effect::Effect;
use c3_render::actor::actor_view;
use c3_render::effect::{EffectMesh, camera_eye};
use c3_render::ground::{self, GroundCache};
use c3_render::objects::{self, ObjectCache, ObjectDraw};
use c3_render::parallax::{self, ParallaxCache};
use c3_render::{
    DrawCmd, Frame, MeshDraw, MeshStyle, MeshVertex, Renderer, Sprite, TextureId, View,
};
use c3_scene::Scene;
use game_conquer::Content;
use glam::{Mat4, Vec2};
use std::time::Instant;

/// Object frame images decoded per frame while streaming.
const OBJECT_DECODE_BUDGET: usize = 12;

/// Clear colour (linear) behind the map.
const BACKGROUND: [f64; 4] = [0.004, 0.004, 0.005, 1.0];

/// Effect marking where the player clicked to walk.
const CLICK_EFFECT: &str = "mouse";

/// World pixels around the view in which effects still play (large effects reach out).
const EFFECT_CULL_MARGIN: f32 = 400.0;

/// One posed mesh (actor part or effect element) for this frame.
struct PosedMesh {
    vertices: Vec<MeshVertex>,
    indices: Indices,
    texture: Option<TextureId>,
    transform: Mat4,
    tint: [f32; 4],
    /// The 3D item it belongs to; its meshes share a depth slice.
    group: u32,
    style: MeshStyle,
}

enum Indices {
    /// Borrowed from the actor's model.
    Actor {
        actor: usize,
        part: PartRef,
    },
    Owned(Vec<u32>),
}

pub struct World {
    pub content: Content,
    pub map_path: String,
    pub scene: Scene,
    pub actors: Vec<Actor>,
    effects: Vec<MapEffect>,
    effect_library: EffectLibrary,
    /// Camera tilt for actors (from the game's graphic config).
    tilt_deg: f32,
    ground: GroundCache,
    objects: ObjectCache,
    parallax: ParallaxCache,
    /// Clock for sprite and motion animation.
    started: Instant,
    sprites: Vec<Sprite>,
    posed: Vec<PosedMesh>,
    order: Vec<DrawCmd>,
    /// Art still streaming in (keep redrawing).
    pub streaming: bool,
}

impl World {
    pub fn new(content: Content, map_path: String, scene: Scene) -> Self {
        let tilt_deg = content.oblique_angle();
        let mut effect_library = EffectLibrary::default();
        let effects = effect_library.map_effects(&scene, &content, tilt_deg, 0);
        Self {
            content,
            map_path,
            scene,
            actors: Vec::new(),
            effects,
            effect_library,
            tilt_deg,
            ground: GroundCache::default(),
            objects: ObjectCache::default(),
            parallax: ParallaxCache::default(),
            started: Instant::now(),
            sprites: Vec::new(),
            posed: Vec::new(),
            order: Vec::new(),
            streaming: false,
        }
    }

    /// Places effect `name` on `cell`, playing once or `forever`.
    pub fn place_effect(
        &mut self,
        name: &str,
        cell: c3_scene::CellPos,
        forever: bool,
    ) -> Result<(), String> {
        let position = self.scene.cell_to_world(cell);
        let now_ms = self.now_ms();
        let effect = self
            .effect_library
            .place(
                name,
                &self.content,
                &self.scene,
                Spot::at(position),
                self.tilt_deg,
                now_ms,
            )
            .ok_or_else(|| format!("effect {name} could not be loaded"))?;
        let mut effect = effect;
        if forever {
            effect.running = effect.running.endless();
        }
        self.effects.push(effect);
        Ok(())
    }

    /// Plays effect `name` at the hero's feet, once or `forever`.
    pub fn hero_effect(&mut self, name: &str, forever: bool) -> Result<(), String> {
        let now_ms = self.now_ms();
        let effect = self
            .effect_library
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
            hero.perform(act, now_ms, &self.content, &mut self.effect_library);
        } else {
            hero.play(act, now_ms, &self.content, &mut self.effect_library);
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
            actor.update(now, &self.scene, &self.content, &mut self.effect_library);
        }
        self.effects.retain(|e| !e.running.is_finished(now));
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
        let ground = match self.scene.ground.as_ref() {
            Some(layer) => {
                let content = &self.content;
                ground::sprites(
                    layer,
                    view.rect,
                    view.seam(),
                    &mut self.ground,
                    renderer,
                    ground::DEFAULT_DECODE_BUDGET,
                    |path| content.load_image(path),
                )
            }
            None => ground::GroundSprites {
                sprites: Vec::new(),
                streaming: false,
            },
        };
        let background = parallax::sprites(
            &self.scene,
            &view,
            &mut self.parallax,
            renderer,
            &self.content,
            ground::DEFAULT_DECODE_BUDGET,
        );
        // 3D items sorted with the objects: actors, then effects near the view.
        let [l, t, r, b] = view.rect;
        let m = EFFECT_CULL_MARGIN;
        let effects: Vec<usize> = (0..self.effects.len())
            .filter(|&e| {
                let p = self.effects[e].position();
                p.x >= l - m && p.x <= r + m && p.z >= t - m && p.z <= b + m
            })
            .collect();
        let now = view.time_ms;
        let cells: Vec<_> = (self.actors.iter().map(|a| a.draw_cell(&self.scene, now)))
            .chain(effects.iter().map(|&e| self.effects[e].cell))
            .collect();
        let objects = objects::sprites(
            &self.scene,
            &view,
            &cells,
            &mut self.objects,
            renderer,
            &self.content,
            OBJECT_DECODE_BUDGET,
        );

        // Back to front: parallax background, ground, then objects and actors interleaved.
        self.sprites = background.sprites;
        self.sprites.extend(ground.sprites);
        let base = self.sprites.len() as u32;
        self.sprites.extend(objects.sprites);
        self.order.clear();
        self.order.push(DrawCmd::Sprites(0..base));
        self.posed.clear();
        let eye = camera_eye(view.center);
        for draw in objects.order {
            match draw {
                ObjectDraw::Sprite(i) => match self.order.last_mut() {
                    Some(DrawCmd::Sprites(r)) if r.end == base + i => r.end += 1,
                    _ => self.order.push(DrawCmd::Sprites(base + i..base + i + 1)),
                },
                ObjectDraw::Actor(item) if (item as usize) < self.actors.len() => {
                    self.pose_actor(item as usize, item, now, eye, renderer);
                }
                ObjectDraw::Actor(item) => {
                    let e = effects[item as usize - self.actors.len()];
                    self.pose_effect(e, item, view.time_ms, eye, renderer);
                }
            }
        }
        self.streaming = background.streaming || ground.streaming || objects.streaming;
    }

    fn push_mesh(&mut self, mesh: PosedMesh) {
        self.order.push(DrawCmd::Mesh(self.posed.len() as u32));
        self.posed.push(mesh);
    }

    fn pose_actor(
        &mut self,
        a: usize,
        group: u32,
        now_ms: u64,
        eye: glam::Vec3,
        renderer: &mut Renderer,
    ) {
        let role = self.actors[a]
            .pose(&self.scene, self.tilt_deg, now_ms)
            .transform();
        for part in self.actors[a].drawables() {
            let actor = &mut self.actors[a];
            let mesh = PosedMesh {
                texture: actor.texture(part, renderer, &self.content),
                vertices: actor.posed(part, now_ms),
                transform: actor.transform(part, role, now_ms),
                indices: Indices::Actor { actor: a, part },
                tint: [1.0; 4],
                group,
                style: MeshStyle::SOLID,
            };
            self.push_mesh(mesh);
        }
        // Its effects share the actor's depth slice, so the body hides what is behind it.
        let effects = self.actors[a].effect_meshes(&self.scene, self.tilt_deg, now_ms, eye);
        for m in effects {
            self.push_effect_mesh(&m.effect, m.mesh, group, renderer);
        }
    }

    fn push_effect_mesh(
        &mut self,
        effect: &Effect,
        m: EffectMesh,
        group: u32,
        renderer: &mut Renderer,
    ) {
        let texture = self
            .effect_library
            .texture(effect, m.layer, renderer, &self.content);
        self.push_mesh(PosedMesh {
            vertices: m.vertices,
            indices: Indices::Owned(m.indices),
            texture,
            transform: m.transform,
            tint: [1.0, 1.0, 1.0, m.opacity],
            group,
            style: MeshStyle::effect(effect.layers[m.layer].blend),
        });
    }

    fn pose_effect(
        &mut self,
        e: usize,
        group: u32,
        now_ms: u64,
        eye: glam::Vec3,
        renderer: &mut Renderer,
    ) {
        let effect = self.effects[e].running.shared();
        for m in self.effects[e].meshes(now_ms, eye) {
            self.push_effect_mesh(&effect, m, group, renderer);
        }
    }

    /// Mesh draws for the posed actors and effects (borrowing this world).
    pub fn mesh_draws(&self) -> Vec<MeshDraw<'_>> {
        self.posed
            .iter()
            .map(|p| MeshDraw {
                vertices: &p.vertices,
                indices: match &p.indices {
                    Indices::Actor { actor, part } => self.actors[*actor].indices(*part),
                    Indices::Owned(indices) => indices,
                },
                texture: p.texture,
                transform: p.transform,
                tint: p.tint,
                group: Some(p.group),
                style: p.style,
            })
            .collect()
    }

    /// The frame description for `meshes` (from [`World::mesh_draws`]).
    pub fn frame<'a>(
        &'a self,
        meshes: &'a [MeshDraw<'a>],
        camera: &Camera2d,
        viewport: Vec2,
    ) -> Frame<'a> {
        Frame {
            clear: BACKGROUND,
            sprite_view: camera.view_proj(viewport),
            sprites: &self.sprites,
            mesh_view: actor_view(camera.view_rect(viewport)),
            meshes,
            order: &self.order,
        }
    }
}
