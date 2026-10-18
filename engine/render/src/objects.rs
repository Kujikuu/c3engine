//! Map objects → sorted, animated [`Sprite`]s: sprite entities and prefab parts, culled
//! to the view and ordered with the classic isometric footprint sort.

use crate::assets::SceneAssets;
use crate::{Renderer, Sprite, TextureId, View};
use c3_assets::sprite::{Prefab, SpriteSet};
use c3_scene::draw_order::{Footprint, draw_order};
use c3_scene::{CellPos, EntityKind, Placement, Scene};
use std::collections::HashMap;

/// Extra world pixels around the view in which objects are still considered, since a
/// large sprite can reach far from its anchor cell.
const CULL_MARGIN: f32 = 1200.0;

#[derive(Clone, Copy)]
struct FrameTexture {
    id: TextureId,
    size: (f32, f32),
}

/// Loaded sprite sets, prefabs and frame textures. Clear it when the scene changes.
#[derive(Default)]
pub struct ObjectCache {
    sets: HashMap<String, Option<SpriteSet>>,
    prefabs: HashMap<String, Option<Prefab>>,
    frames: HashMap<String, Option<FrameTexture>>,
}

impl ObjectCache {
    pub fn clear(&mut self, renderer: &mut Renderer) {
        for t in self.frames.drain().filter_map(|(_, t)| t) {
            renderer.release_texture(t.id);
        }
        self.sets.clear();
        self.prefabs.clear();
    }

    pub fn loaded_frames(&self) -> usize {
        self.frames.len()
    }
}

/// One drawable object instance this frame.
struct Item<'a> {
    footprint: Footprint,
    set: &'a str,
    animation: &'a str,
    offset: (i32, i32),
    frame_ms: u32,
}

/// One entry of the objects' painter's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectDraw {
    /// Index into [`ObjectSprites::sprites`].
    Sprite(u32),
    /// Index into the `actors` passed to [`sprites`].
    Actor(u32),
}

pub struct ObjectSprites {
    pub sprites: Vec<Sprite>,
    /// Back-to-front order of sprites and actors.
    pub order: Vec<ObjectDraw>,
    /// Some visible frames are still loading.
    pub streaming: bool,
}

/// Sprites for every object near the view, animated at `view.time_ms`, sorted together
/// with `actors` (cells of 3D characters, 1×1 footprints, as the client sorts roles).
/// At most `budget` frame images are decoded per call.
pub fn sprites(
    scene: &Scene,
    view: &View,
    actors: &[CellPos],
    cache: &mut ObjectCache,
    renderer: &mut Renderer,
    assets: &dyn SceneAssets,
    mut budget: usize,
) -> ObjectSprites {
    let (rect, time_ms) = (view.rect, view.time_ms);
    let near = |cell: CellPos| {
        let (x, y) = scene.cell_to_world(cell);
        x >= rect[0] - CULL_MARGIN
            && x <= rect[2] + CULL_MARGIN
            && y >= rect[1] - CULL_MARGIN
            && y <= rect[3] + CULL_MARGIN
    };

    // Prefabs referenced by visible entities must be loaded before items can borrow them.
    for e in &scene.entities {
        if let (EntityKind::Prefab { asset }, Placement::Cell(cell)) = (&e.kind, e.placement)
            && near(cell)
            && !cache.prefabs.contains_key(asset)
        {
            cache.prefabs.insert(asset.clone(), assets.prefab(asset));
        }
    }

    let mut items = Vec::new();
    for e in &scene.entities {
        let Placement::Cell(cell) = e.placement else {
            continue;
        };
        if !near(cell) {
            continue;
        }
        match &e.kind {
            EntityKind::Sprite {
                asset,
                animation,
                footprint,
                offset,
                frame_ms,
            } => items.push(Item {
                footprint: Footprint {
                    cell,
                    size: *footprint,
                },
                set: asset,
                animation,
                offset: *offset,
                frame_ms: *frame_ms,
            }),
            EntityKind::Prefab { asset } => {
                let Some(Some(prefab)) = cache.prefabs.get(asset) else {
                    continue;
                };
                for p in &prefab.parts {
                    items.push(Item {
                        footprint: Footprint {
                            cell: (cell.0 + p.cell_offset.0, cell.1 + p.cell_offset.1),
                            size: p.footprint,
                        },
                        set: &p.sprite_set,
                        animation: &p.animation,
                        offset: p.offset,
                        frame_ms: p.frame_ms,
                    });
                }
            }
            EntityKind::Sound { .. } | EntityKind::Effect { .. } => {}
        }
    }

    let footprints: Vec<Footprint> = items
        .iter()
        .map(|i| i.footprint)
        .chain(actors.iter().map(|&cell| Footprint { cell, size: (1, 1) }))
        .collect();
    let mut out = ObjectSprites {
        sprites: Vec::with_capacity(items.len()),
        order: Vec::with_capacity(footprints.len()),
        streaming: false,
    };
    for index in draw_order(&footprints) {
        let Some(item) = items.get(index) else {
            out.order
                .push(ObjectDraw::Actor((index - items.len()) as u32));
            continue;
        };
        let set = cache
            .sets
            .entry(item.set.to_owned())
            .or_insert_with(|| assets.sprite_set(item.set));
        let Some(frames) = set
            .as_ref()
            .and_then(|s| s.frames(item.animation))
            .filter(|f| !f.is_empty())
        else {
            continue;
        };
        let frame = &frames[(time_ms / item.frame_ms.max(1) as u64) as usize % frames.len()];
        let texture = match cache.frames.get(frame) {
            Some(t) => *t,
            None if budget > 0 => {
                budget -= 1;
                let t = assets.image(frame).map(|img| FrameTexture {
                    size: (img.width as f32, img.height as f32),
                    id: renderer.upload_texture(&img),
                });
                cache.frames.insert(frame.clone(), t);
                t
            }
            None => {
                out.streaming = true;
                continue;
            }
        };
        let Some(t) = texture else { continue };
        let (cx, cy) = scene.cell_to_world(item.footprint.cell);
        out.order.push(ObjectDraw::Sprite(out.sprites.len() as u32));
        out.sprites.push(Sprite {
            rect: [
                cx + item.offset.0 as f32,
                cy + item.offset.1 as f32,
                t.size.0,
                t.size.1,
            ],
            uv: [0.0, 0.0, 1.0, 1.0],
            texture: Some(t.id),
            tint: [1.0; 4],
        });
    }
    out
}
