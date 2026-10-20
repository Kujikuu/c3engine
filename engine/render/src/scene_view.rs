//! A scene drawn the way the classic client draws a map: parallax sky and water behind the
//! ground, then objects, 3D effects and actors interleaved in the isometric painter's order.
//! The game client and the Studio's Scene view both draw through it, so the editor shows
//! exactly what the game shows.
//!
//! Each frame: [`SceneView::prepare`] builds sprites, posed meshes and the painter's order;
//! [`SceneView::mesh_draws`] + [`SceneView::frame`] then describe it to the renderer.
//! Actors belong to the game: the caller passes their cells and poses them in a callback,
//! their meshes may borrow indices from the caller's models (`K`, see [`PosedIndices`]).

use crate::actor::actor_view;
use crate::assets::SceneAssets;
use crate::effect::{EffectMesh, camera_eye};
use crate::effect_library::{EffectLibrary, MapEffect, Spot};
use crate::ground::{self, GroundCache, GroundSprites};
use crate::objects::{self, ObjectCache, ObjectDraw, ObjectSprites};
use crate::parallax::{self, ParallaxCache};
use crate::{DrawCmd, Frame, MeshDraw, MeshStyle, MeshVertex, Renderer, Sprite, TextureId, View};
use c3_assets::effect::Effect;
use c3_scene::draw_order::{Footprint, draw_order};
use c3_scene::{CellPos, Scene};
use glam::Mat4;
use glam::camera::rh::proj::directx;

/// Object frame images decoded per frame while streaming.
const OBJECT_DECODE_BUDGET: usize = 12;

/// World pixels around the view in which effects still play (large effects reach out).
const EFFECT_CULL_MARGIN: f32 = 400.0;

/// Which parts of the scene are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layers {
    /// Parallax backgrounds (sky, moving water).
    pub sky: bool,
    pub ground: bool,
    /// Cover sprites and terrain objects.
    pub objects: bool,
    /// The map's 3D effects (not actors' own effects).
    pub effects: bool,
}

impl Default for Layers {
    fn default() -> Self {
        Self {
            sky: true,
            ground: true,
            objects: true,
            effects: true,
        }
    }
}

/// Indices of a posed mesh: its own, or borrowed from the caller's model by key.
pub enum PosedIndices<K> {
    Owned(Vec<u32>),
    External(K),
}

/// One posed mesh (actor part or effect element) for this frame.
pub struct PosedMesh<K> {
    vertices: Vec<MeshVertex>,
    indices: PosedIndices<K>,
    texture: Option<TextureId>,
    transform: Mat4,
    tint: [f32; 4],
    /// The 3D item it belongs to; its meshes share a depth slice.
    group: u32,
    style: MeshStyle,
}

/// Collects one 3D item's meshes in painter's order (given to the actor callback).
pub struct Poser<'a, K> {
    renderer: &'a mut Renderer,
    assets: &'a dyn SceneAssets,
    library: &'a mut EffectLibrary,
    meshes: &'a mut Vec<PosedMesh<K>>,
    order: &'a mut Vec<DrawCmd>,
    group: u32,
}

impl<K> Poser<'_, K> {
    pub fn renderer(&mut self) -> &mut Renderer {
        self.renderer
    }

    fn push(&mut self, mesh: PosedMesh<K>) {
        self.order.push(DrawCmd::Mesh(self.meshes.len() as u32));
        self.meshes.push(mesh);
    }

    /// An opaque, lit mesh (an actor's part).
    pub fn solid(
        &mut self,
        vertices: Vec<MeshVertex>,
        indices: PosedIndices<K>,
        texture: Option<TextureId>,
        transform: Mat4,
    ) {
        self.tinted(vertices, indices, texture, transform, [1.0; 4]);
    }

    /// A solid mesh drawn in `tint` (times its texture).
    pub fn tinted(
        &mut self,
        vertices: Vec<MeshVertex>,
        indices: PosedIndices<K>,
        texture: Option<TextureId>,
        transform: Mat4,
        tint: [f32; 4],
    ) {
        let group = self.group;
        self.push(PosedMesh {
            vertices,
            indices,
            texture,
            transform,
            tint,
            group,
            style: MeshStyle::SOLID,
        });
    }

    /// One element of `effect`, with its layer's texture and blend.
    pub fn effect(&mut self, effect: &Effect, m: EffectMesh) {
        let texture = self
            .library
            .texture(effect, m.layer, self.renderer, self.assets);
        let group = self.group;
        self.push(PosedMesh {
            vertices: m.vertices,
            indices: PosedIndices::Owned(m.indices),
            texture,
            transform: m.transform,
            tint: [1.0, 1.0, 1.0, m.opacity],
            group,
            style: MeshStyle::effect(effect.layers[m.layer].blend),
        });
    }
}

/// A scene's streamed art, its playing effects and this frame's draw lists.
pub struct SceneView<K = ()> {
    pub layers: Layers,
    /// Effects and their textures, shared with the actors' effects.
    pub library: EffectLibrary,
    effects: Vec<MapEffect>,
    /// Camera tilt for 3D items, in degrees.
    tilt_deg: f32,
    ground: GroundCache,
    objects: ObjectCache,
    parallax: ParallaxCache,
    sprites: Vec<Sprite>,
    meshes: Vec<PosedMesh<K>>,
    order: Vec<DrawCmd>,
    streaming: bool,
}

impl<K> SceneView<K> {
    /// A view of `scene` whose map effects start playing at `now_ms`.
    pub fn new(scene: &Scene, assets: &dyn SceneAssets, tilt_deg: f32, now_ms: u64) -> Self {
        let mut library = EffectLibrary::default();
        let effects = library.map_effects(scene, assets, tilt_deg, now_ms);
        Self {
            layers: Layers::default(),
            library,
            effects,
            tilt_deg,
            ground: GroundCache::default(),
            objects: ObjectCache::default(),
            parallax: ParallaxCache::default(),
            sprites: Vec::new(),
            meshes: Vec::new(),
            order: Vec::new(),
            streaming: false,
        }
    }

    pub fn tilt_deg(&self) -> f32 {
        self.tilt_deg
    }

    /// Effects standing on the map.
    pub fn effect_count(&self) -> usize {
        self.effects.len()
    }

    /// Ground tiles uploaded so far.
    pub fn loaded_tiles(&self) -> usize {
        self.ground.loaded()
    }

    /// Art near the view is still streaming in (keep redrawing).
    pub fn streaming(&self) -> bool {
        self.streaming
    }

    /// Places effect `name` on `cell` from `now_ms`, playing once or `forever`.
    pub fn place_effect(
        &mut self,
        name: &str,
        cell: CellPos,
        forever: bool,
        scene: &Scene,
        assets: &dyn SceneAssets,
        now_ms: u64,
    ) -> Result<(), String> {
        let spot = Spot::at(scene.cell_to_world(cell));
        let mut effect = self
            .library
            .place(name, assets, scene, spot, self.tilt_deg, now_ms)
            .ok_or_else(|| format!("effect {name} could not be loaded"))?;
        if forever {
            effect.running = effect.running.endless();
        }
        self.effects.push(effect);
        Ok(())
    }

    /// Stops every effect standing on the map.
    pub fn clear_effects(&mut self) {
        self.effects.clear();
    }

    /// Drops effects that finished by `now_ms`.
    pub fn tick(&mut self, now_ms: u64) {
        self.effects.retain(|e| !e.running.is_finished(now_ms));
    }

    /// Reads the art again (assets changed on disk): frees the textures, forgets the loaded
    /// effects and restarts the map's effects from `now_ms`.
    pub fn reload_assets(
        &mut self,
        scene: &Scene,
        assets: &dyn SceneAssets,
        renderer: &mut Renderer,
        now_ms: u64,
    ) {
        self.release(renderer);
        self.library = EffectLibrary::default();
        self.effects = self
            .library
            .map_effects(scene, assets, self.tilt_deg, now_ms);
    }

    /// Frees the view's GPU textures; call before dropping it.
    pub fn release(&mut self, renderer: &mut Renderer) {
        self.ground.clear(renderer);
        self.objects.clear(renderer);
        self.parallax.clear(renderer);
        self.library.release_textures(renderer);
    }

    /// Builds this frame's sprites, posed meshes and painter's order. `actors` are the cells
    /// of the caller's 3D characters; `pose_actor(i, poser)` adds actor `i`'s meshes.
    pub fn prepare(
        &mut self,
        scene: &Scene,
        assets: &dyn SceneAssets,
        renderer: &mut Renderer,
        view: &View,
        actors: &[ActorSpot],
        mut pose_actor: impl FnMut(usize, &mut Poser<K>),
    ) {
        let layers = self.layers;
        let background = match layers.sky {
            true => parallax::sprites(
                scene,
                view,
                &mut self.parallax,
                renderer,
                assets,
                ground::DEFAULT_DECODE_BUDGET,
            ),
            false => GroundSprites::default(),
        };
        let ground = match scene.ground.as_ref().filter(|_| layers.ground) {
            Some(layer) => ground::sprites(
                layer,
                view.rect,
                view.seam(),
                &mut self.ground,
                renderer,
                ground::DEFAULT_DECODE_BUDGET,
                |path| assets.image(path),
            ),
            None => GroundSprites::default(),
        };
        // 3D items sorted with the objects: actors, then effects near the view.
        let [l, t, r, b] = view.rect;
        let m = EFFECT_CULL_MARGIN;
        let effects: Vec<usize> = match layers.effects {
            true => (0..self.effects.len())
                .filter(|&e| {
                    let p = self.effects[e].position();
                    p.x >= l - m && p.x <= r + m && p.z >= t - m && p.z <= b + m
                })
                .collect(),
            false => Vec::new(),
        };
        let cells: Vec<CellPos> = (actors.iter().map(|a| a.cell))
            .chain(effects.iter().map(|&e| self.effects[e].cell))
            .collect();
        let objects = match layers.objects {
            true => objects::sprites(
                scene,
                view,
                &cells,
                &mut self.objects,
                renderer,
                assets,
                OBJECT_DECODE_BUDGET,
            ),
            false => items_only(&cells),
        };

        // Back to front: parallax background, ground, then objects and 3D items interleaved.
        self.sprites = background.sprites;
        self.sprites.extend(ground.sprites);
        let base = self.sprites.len() as u32;
        self.sprites.extend(objects.sprites);
        self.order.clear();
        self.order.push(DrawCmd::Sprites(0..base));
        self.meshes.clear();
        let eye = camera_eye(view.center);
        for draw in objects.order {
            let item = match draw {
                ObjectDraw::Sprite(i) => {
                    match self.order.last_mut() {
                        Some(DrawCmd::Sprites(r)) if r.end == base + i => r.end += 1,
                        _ => self.order.push(DrawCmd::Sprites(base + i..base + i + 1)),
                    }
                    continue;
                }
                ObjectDraw::Actor(item) => item,
            };
            if let Some(shadow) = actors.get(item as usize).and_then(|a| a.shadow) {
                let at = self.sprites.len() as u32;
                self.sprites.push(shadow);
                self.order.push(DrawCmd::Sprites(at..at + 1));
            }
            let mut poser = Poser {
                renderer: &mut *renderer,
                assets,
                library: &mut self.library,
                meshes: &mut self.meshes,
                order: &mut self.order,
                group: item,
            };
            match (item as usize).checked_sub(actors.len()) {
                None => pose_actor(item as usize, &mut poser),
                Some(e) => {
                    let effect = &mut self.effects[effects[e]];
                    let shared = effect.running.shared();
                    for mesh in effect.meshes(view.time_ms, eye) {
                        poser.effect(&shared, mesh);
                    }
                }
            }
        }
        self.streaming = background.streaming || ground.streaming || objects.streaming;
    }

    /// Mesh draws for the posed meshes; `external` resolves borrowed indices.
    pub fn mesh_draws<'a>(&'a self, external: impl Fn(&K) -> &'a [u32]) -> Vec<MeshDraw<'a>> {
        self.meshes
            .iter()
            .map(|p| MeshDraw {
                vertices: &p.vertices,
                indices: match &p.indices {
                    PosedIndices::Owned(indices) => indices,
                    PosedIndices::External(key) => external(key),
                },
                texture: p.texture,
                transform: p.transform,
                tint: p.tint,
                group: Some(p.group),
                style: p.style,
            })
            .collect()
    }

    /// The frame for `meshes` (from [`SceneView::mesh_draws`]) seen through world `rect`
    /// (`[min_x, min_y, max_x, max_y]`), on `clear`.
    pub fn frame<'a>(
        &'a self,
        meshes: &'a [MeshDraw<'a>],
        rect: [f32; 4],
        clear: [f64; 4],
    ) -> Frame<'a> {
        let [l, t, r, b] = rect;
        Frame {
            clear,
            sprite_view: directx::orthographic(l, r, b, t, -1.0, 1.0),
            sprites: &self.sprites,
            mesh_view: actor_view(rect),
            meshes,
            order: &self.order,
            light: Default::default(),
        }
    }
}

/// An actor as the painter's order sees it: the cell it is drawn on, and the shadow under it
/// (drawn just before it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActorSpot {
    pub cell: CellPos,
    pub shadow: Option<Sprite>,
}

/// The painter's order of 3D items alone, when objects are hidden.
fn items_only(cells: &[CellPos]) -> ObjectSprites {
    let footprints: Vec<Footprint> = cells
        .iter()
        .map(|&cell| Footprint { cell, size: (1, 1) })
        .collect();
    ObjectSprites {
        sprites: Vec::new(),
        order: draw_order(&footprints)
            .into_iter()
            .map(|i| ObjectDraw::Actor(i as u32))
            .collect(),
        streaming: false,
    }
}
