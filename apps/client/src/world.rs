//! The loaded world: current map scene and its streamed ground.

use crate::camera::Camera2d;
use c3_render::ground::{self, GroundCache};
use c3_render::{Frame, Renderer, Sprite};
use c3_scene::Scene;
use game_conquer::Content;
use glam::Vec2;

/// Clear colour (linear) behind the map.
const BACKGROUND: [f64; 4] = [0.004, 0.004, 0.005, 1.0];

pub struct World {
    pub content: Content,
    pub map_path: String,
    pub scene: Scene,
    ground: GroundCache,
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
        let view = camera.view_rect(viewport);
        let result = match self.scene.ground.as_ref() {
            Some(layer) => {
                let content = &self.content;
                ground::sprites(
                    layer,
                    view,
                    0.5 / camera.zoom,
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
        self.sprites = result.sprites;
        self.streaming = result.streaming;
        Frame {
            clear: BACKGROUND,
            sprite_view: camera.view_proj(viewport),
            sprites: &self.sprites,
            ..Default::default()
        }
    }
}
