//! Turns a scene's ground [`TileLayer`] into [`Sprite`]s for the visible area, streaming
//! tile textures through a [`GroundCache`]. Shared by every host (Studio, game client).

use crate::{Renderer, Sprite, TextureId};
use c3_assets::Image;
use c3_scene::TileLayer;
use std::collections::HashMap;

/// Tiles decoded per call by default, so the frame rate holds while streaming.
pub const DEFAULT_DECODE_BUDGET: usize = 6;

/// GPU textures for one ground layer's tiles.
#[derive(Default)]
pub struct GroundCache {
    /// Source the cache was built for; a different layer clears it.
    source: String,
    /// `None` = the tile failed to load.
    tiles: HashMap<u16, Option<TextureId>>,
}

impl GroundCache {
    pub fn loaded(&self) -> usize {
        self.tiles.len()
    }

    /// Releases all textures.
    pub fn clear(&mut self, renderer: &mut Renderer) {
        for id in self.tiles.drain().filter_map(|(_, t)| t) {
            renderer.release_texture(id);
        }
    }
}

/// Visible-area result.
pub struct GroundSprites {
    pub sprites: Vec<Sprite>,
    /// Some visible tiles are still waiting to be decoded.
    pub streaming: bool,
}

/// Linear-space tints for placeholder tiles.
const PENDING_TINT: [f32; 4] = [0.012, 0.012, 0.012, 1.0];
const FAILED_TINT: [f32; 4] = [0.05, 0.0, 0.05, 1.0];

/// Sprites covering `view` (world rect `[min_x, min_y, max_x, max_y]`). Missing tiles are
/// decoded with `load` (path → image), at most `budget` per call. `seam` is extra size
/// added to each tile (in world units) to hide gaps between neighbours.
pub fn sprites(
    layer: &TileLayer,
    view: [f32; 4],
    seam: f32,
    cache: &mut GroundCache,
    renderer: &mut Renderer,
    mut budget: usize,
    load: impl Fn(&str) -> Option<Image>,
) -> GroundSprites {
    if cache.source != layer.source {
        cache.clear(renderer);
        cache.source = layer.source.clone();
    }
    let mut out = GroundSprites {
        sprites: Vec::new(),
        streaming: false,
    };
    if layer.width == 0 {
        return out;
    }
    let g = layer.tile_size as f32;
    let (ox, oy) = (layer.origin.0 as f32, layer.origin.1 as f32);
    let x0 = ((view[0] - ox) / g).floor().max(0.0) as u32;
    let y0 = ((view[1] - oy) / g).floor().max(0.0) as u32;
    let x1 = (((view[2] - ox) / g).ceil().max(0.0) as u32).min(layer.width);
    let y1 = (((view[3] - oy) / g).ceil().max(0.0) as u32).min(layer.height);
    for gy in y0..y1 {
        for gx in x0..x1 {
            let Some(id) = layer.tile(gx, gy) else {
                continue;
            };
            let entry = match cache.tiles.get(&id) {
                Some(t) => Some(*t),
                None if budget > 0 => {
                    budget -= 1;
                    let texture = layer
                        .tileset
                        .frames
                        .get(&id)
                        .and_then(|f| load(f))
                        .map(|img| renderer.upload_texture(&img));
                    cache.tiles.insert(id, texture);
                    Some(texture)
                }
                None => None,
            };
            let (texture, tint) = match entry {
                Some(Some(t)) => (Some(t), [1.0; 4]),
                Some(None) => (None, FAILED_TINT),
                None => {
                    out.streaming = true;
                    (None, PENDING_TINT)
                }
            };
            out.sprites.push(Sprite {
                rect: [ox + gx as f32 * g, oy + gy as f32 * g, g + seam, g + seam],
                uv: [0.0, 0.0, 1.0, 1.0],
                texture,
                tint,
            });
        }
    }
    out
}
