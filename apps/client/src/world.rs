//! The loaded world: current map scene, its streamed art and the actors on it.
//!
//! Each frame: [`World::prepare`] builds sprites, posed actor meshes and the painter's
//! order; [`World::mesh_draws`] + [`World::frame`] then describe it to the renderer.

use crate::actors::{Actor, PartRef};
use crate::camera::Camera2d;
use c3_render::actor::actor_view;
use c3_render::ground::{self, GroundCache};
use c3_render::objects::{self, ObjectCache, ObjectDraw};
use c3_render::parallax::{self, ParallaxCache};
use c3_render::{DrawCmd, Frame, MeshDraw, MeshVertex, Renderer, Sprite, TextureId, View};
use c3_scene::Scene;
use game_conquer::Content;
use glam::{Mat4, Vec2};
use std::time::Instant;

/// Object frame images decoded per frame while streaming.
const OBJECT_DECODE_BUDGET: usize = 12;

/// Clear colour (linear) behind the map.
const BACKGROUND: [f64; 4] = [0.004, 0.004, 0.005, 1.0];

/// One posed actor part for this frame.
struct PosedMesh {
    actor: usize,
    part: PartRef,
    vertices: Vec<MeshVertex>,
    texture: Option<TextureId>,
    transform: Mat4,
}

pub struct World {
    pub content: Content,
    pub map_path: String,
    pub scene: Scene,
    pub actors: Vec<Actor>,
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
        Self {
            content,
            map_path,
            scene,
            actors: Vec::new(),
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

    /// Builds this frame's sprites, posed actors and painter's order.
    pub fn prepare(&mut self, renderer: &mut Renderer, camera: &Camera2d, viewport: Vec2) {
        let view = View {
            rect: camera.view_rect(viewport),
            center: (camera.center.x, camera.center.y),
            zoom: camera.zoom,
            time_ms: self.started.elapsed().as_millis() as u64,
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
        let actor_cells: Vec<_> = self.actors.iter().map(|a| a.cell).collect();
        let objects = objects::sprites(
            &self.scene,
            &view,
            &actor_cells,
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
        let secs = view.time_ms as f32 / 1000.0;
        for draw in objects.order {
            match draw {
                ObjectDraw::Sprite(i) => match self.order.last_mut() {
                    Some(DrawCmd::Sprites(r)) if r.end == base + i => r.end += 1,
                    _ => self.order.push(DrawCmd::Sprites(base + i..base + i + 1)),
                },
                ObjectDraw::Actor(a) => {
                    let a = a as usize;
                    let role = self.actors[a].pose(&self.scene, self.tilt_deg).transform();
                    for part in self.actors[a].drawables() {
                        self.order.push(DrawCmd::Mesh(self.posed.len() as u32));
                        let actor = &mut self.actors[a];
                        let texture = actor.texture(part, renderer, &self.content);
                        let vertices = actor.posed(part, secs);
                        let transform = actor.transform(part, role, secs);
                        self.posed.push(PosedMesh {
                            actor: a,
                            part,
                            vertices,
                            texture,
                            transform,
                        });
                    }
                }
            }
        }
        self.streaming = background.streaming || ground.streaming || objects.streaming;
    }

    /// Mesh draws for the posed actors (borrowing this world).
    pub fn mesh_draws(&self) -> Vec<MeshDraw<'_>> {
        self.posed
            .iter()
            .map(|p| MeshDraw {
                vertices: &p.vertices,
                indices: self.actors[p.actor].indices(p.part),
                texture: p.texture,
                transform: p.transform,
                tint: [1.0; 4],
                group: Some(p.actor as u32),
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
