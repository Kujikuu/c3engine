//! Parallax background layers (sky, moving water) drawn behind the ground.
//!
//! Classic C3 (`CSceneLayer::Show`, `C2DMapPuzzleObj::Show`, `CPuzzleBmp::Show`): a layer
//! is centred on the map centre `(w·cell_w/2, h·cell_h/2)` and its view centre moves at
//! `rate`% of the camera's offset from that point. Layers with a scroll speed drift by
//! `speed · rate · t / 100 000` pixels and repeat endlessly; others are drawn once.
//! Drawn in world space: a layer point `L` lands at `L + (camera − centre)·(1 − rate/100)`.

use crate::assets::SceneAssets;
use crate::ground::{self, GroundCache, GroundSprites};
use crate::{Renderer, Sprite, View};
use c3_scene::{LayerItem, Scene, TileLayer};
use std::collections::HashMap;

/// Loaded layers and their tile caches, by asset path.
#[derive(Default)]
pub struct ParallaxCache {
    layers: HashMap<String, Option<TileLayer>>,
    tiles: HashMap<String, GroundCache>,
}

impl ParallaxCache {
    pub fn clear(&mut self, renderer: &mut Renderer) {
        for c in self.tiles.values_mut() {
            c.clear(renderer);
        }
        self.tiles.clear();
        self.layers.clear();
    }
}

/// Sprites for all parallax layers of `scene`, back to front.
pub fn sprites(
    scene: &Scene,
    view: &View,
    cache: &mut ParallaxCache,
    renderer: &mut Renderer,
    assets: &dyn SceneAssets,
    budget: usize,
) -> GroundSprites {
    let (rect, camera, seam) = (view.rect, view.center, view.seam());
    let p = scene.projection;
    let centre = (
        scene.grid.width as f32 * p.cell_w / 2.0,
        scene.grid.height as f32 * p.cell_h / 2.0,
    );
    let mut out = GroundSprites {
        sprites: Vec::new(),
        streaming: false,
    };
    let mut layers: Vec<_> = scene.layers.iter().collect();
    layers.sort_by_key(|l| l.order);
    for layer in layers {
        let (rx, ry) = (
            layer.scroll_rate.0 as f32 / 100.0,
            layer.scroll_rate.1 as f32 / 100.0,
        );
        let shift = (
            (camera.0 - centre.0) * (1.0 - rx),
            (camera.1 - centre.1) * (1.0 - ry),
        );
        for item in &layer.items {
            let LayerItem::Tiles { asset } = item else {
                continue;
            };
            let loaded = cache
                .layers
                .entry(asset.clone())
                .or_insert_with(|| assets.tile_layer(asset));
            let Some(tiles) = loaded.as_mut() else {
                continue;
            };
            let (w, h) = tiles.world_size();
            let (w, h) = (w as f32, h as f32);
            let secs = view.time_ms as f32 / 1000.0;
            let roll = (
                tiles.scroll.0 as f32 * rx * secs,
                tiles.scroll.1 as f32 * ry * secs,
            );
            let base = (
                centre.0 - w / 2.0 + shift.0 + roll.0,
                centre.1 - h / 2.0 + shift.1 + roll.1,
            );
            let repeats = |lo: f32, hi: f32, start: f32, size: f32, wrap: bool| -> Vec<f32> {
                if !wrap {
                    return vec![start];
                }
                let first = start + ((lo - start) / size).floor() * size;
                (0..)
                    .map(|k| first + k as f32 * size)
                    .take_while(|&x| x < hi)
                    .collect()
            };
            let wrap = tiles.scroll != (0, 0);
            let tile_cache = cache.tiles.entry(asset.clone()).or_default();
            for oy in repeats(rect[1], rect[3], base.1, h, wrap) {
                for ox in repeats(rect[0], rect[2], base.0, w, wrap) {
                    tiles.origin = (ox.round() as i32, oy.round() as i32);
                    let part =
                        ground::sprites(tiles, rect, seam, tile_cache, renderer, budget, |p| {
                            assets.image(p)
                        });
                    out.streaming |= part.streaming;
                    out.sprites.extend::<Vec<Sprite>>(part.sprites);
                }
            }
        }
    }
    out
}
