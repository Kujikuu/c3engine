//! The loaded world: current map scene and its streamed ground.

use crate::camera::Camera2d;
use c3_render::ground::{self, GroundCache};
use c3_render::objects::{self, ObjectCache};
use c3_render::parallax::{self, ParallaxCache};
use c3_render::{Frame, Renderer, Sprite, View};
use c3_scene::Scene;
use game_conquer::Content;
use glam::Vec2;
use std::time::Instant;

/// Object frame images decoded per frame while streaming.
const OBJECT_DECODE_BUDGET: usize = 12;

/// Clear colour (linear) behind the map.
const BACKGROUND: [f64; 4] = [0.004, 0.004, 0.005, 1.0];

pub struct World {
    pub content: Content,
    pub map_path: String,
    pub scene: Scene,
    ground: GroundCache,
    objects: ObjectCache,
    parallax: ParallaxCache,
    /// Clock for sprite animation.
    started: Instant,
    sprites: Vec<Sprite>,
    /// Tiles still streaming in (keep redrawing).
    pub streaming: bool,
}

impl World {
    pub fn new(content: Content, map_path: String, scene: Scene) -> Self {
        Self {
            content,
            map_path,
            scene,
            ground: GroundCache::default(),
            objects: ObjectCache::default(),
            parallax: ParallaxCache::default(),
            started: Instant::now(),
            sprites: Vec::new(),
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

    /// Builds this frame's draw list.
    pub fn frame(
        &mut self,
        renderer: &mut Renderer,
        camera: &Camera2d,
        viewport: Vec2,
    ) -> Frame<'_> {
        let view = View {
            rect: camera.view_rect(viewport),
            center: (camera.center.x, camera.center.y),
            zoom: camera.zoom,
            time_ms: self.started.elapsed().as_millis() as u64,
        };
        let result = match self.scene.ground.as_ref() {
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
        let objects = objects::sprites(
            &self.scene,
            &view,
            &mut self.objects,
            renderer,
            &self.content,
            OBJECT_DECODE_BUDGET,
        );
        // Back to front: parallax background, ground, objects.
        self.sprites = background.sprites;
        self.sprites.extend(result.sprites);
        self.sprites.extend(objects.sprites);
        self.streaming = background.streaming || result.streaming || objects.streaming;
        Frame {
            clear: BACKGROUND,
            sprite_view: camera.view_proj(viewport),
            sprites: &self.sprites,
            ..Default::default()
        }
    }
}
