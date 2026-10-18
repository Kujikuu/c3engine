//! 3D characters standing on the map.

use c3_assets::model::Model;
use c3_render::actor::{self, ActorPose};
use c3_render::{MeshVertex, Renderer, TextureId};
use c3_scene::{CellPos, Scene};
use game_conquer::Content;

/// C3 motions play at 30 frames per second.
const MOTION_FPS: f32 = 30.0;
/// Classic `CRole::s_fBaseScale`.
const BASE_SCALE: f32 = 0.75;

pub struct Actor {
    pub cell: CellPos,
    /// 0..8, clockwise; the classic facing is `-45 · dir - 45` degrees.
    pub direction: u8,
    /// Monster.dat-style zoom percentage.
    pub zoom_percent: f32,
    model: Model,
    texture_path: Option<String>,
    /// Uploaded texture: outer `None` = not attempted yet.
    texture: Option<Option<TextureId>>,
}

impl Actor {
    pub fn load(
        content: &Content,
        model_id: u64,
        cell: CellPos,
        direction: u8,
    ) -> Result<Self, String> {
        let (model, texture_path) = content.load_model(model_id)?;
        Ok(Self {
            cell,
            direction: direction % 8,
            zoom_percent: 100.0,
            model,
            texture_path,
            texture: None,
        })
    }

    pub fn turn(&mut self, steps: i8) {
        self.direction = (self.direction as i8 + steps).rem_euclid(8) as u8;
    }

    pub fn pose(&self, scene: &Scene, tilt_deg: f32) -> ActorPose {
        ActorPose {
            position: scene.cell_to_world(self.cell),
            facing_deg: -45.0 * self.direction as f32 - 45.0,
            tilt_deg,
            scale: BASE_SCALE * self.zoom_percent / 100.0,
            height: 0.0,
        }
    }

    pub fn part_count(&self) -> usize {
        self.model.parts.len()
    }

    /// Triangle indices of part `part`.
    pub fn indices(&self, part: usize) -> &[u32] {
        &self.model.parts[part].mesh.indices
    }

    /// The actor's texture, uploading it on first use.
    pub fn texture(&mut self, renderer: &mut Renderer, content: &Content) -> Option<TextureId> {
        *self.texture.get_or_insert_with(|| {
            let img = self
                .texture_path
                .as_deref()
                .and_then(|p| content.load_image(p))?;
            Some(renderer.upload_texture(&img))
        })
    }

    /// Vertices of part `part` posed at `time_secs` of its animation.
    pub fn posed_part(&self, part: usize, time_secs: f32) -> Vec<MeshVertex> {
        let p = &self.model.parts[part];
        let frame = p.clip.as_ref().map_or(0.0, |c| {
            (time_secs * MOTION_FPS) % c.frame_count.max(1) as f32
        });
        actor::posed_vertices(&p.mesh, p.clip.as_ref(), frame)
    }
}
