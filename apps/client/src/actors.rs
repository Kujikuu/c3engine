//! 3D characters standing on the map: a body model plus parts attached to its sockets.

use c3_assets::model::Model;
use c3_render::actor::{self, ActorPose};
use c3_render::{MeshVertex, Renderer, TextureId};
use c3_scene::{CellPos, Scene};
use game_conquer::Content;
use game_conquer::character::{Character, Look, PartModel};
use glam::Mat4;

/// C3 motions play at 30 frames per second.
const MOTION_FPS: f32 = 30.0;
/// Classic `CRole::s_fBaseScale`.
const BASE_SCALE: f32 = 0.75;

/// A model with its lazily uploaded texture.
struct Part {
    model: Model,
    texture_path: Option<String>,
    /// Outer `None` = not uploaded yet; inner `None` = no texture.
    texture: Option<Option<TextureId>>,
}

impl Part {
    fn new(p: PartModel) -> Self {
        Self {
            model: p.model,
            texture_path: p.texture,
            texture: None,
        }
    }
}

/// Addresses one drawable mesh of an actor.
#[derive(Debug, Clone, Copy)]
pub struct PartRef {
    /// 0 = body, n = attachment n-1.
    group: usize,
    part: usize,
}

pub struct Actor {
    pub cell: CellPos,
    /// 0..8, clockwise; the classic facing is `-45 · dir - 45` degrees.
    pub direction: u8,
    /// Monster.dat-style zoom percentage.
    pub zoom_percent: f32,
    body: Part,
    /// (socket name in the body, attached part).
    attachments: Vec<(&'static str, Part)>,
}

impl Actor {
    /// A single-model actor (NPCs, monsters) from a `3dobj.ini` id.
    pub fn from_model(
        content: &Content,
        model_id: u64,
        cell: CellPos,
        direction: u8,
    ) -> Result<Self, String> {
        let (model, texture) = content.load_model(model_id)?;
        Ok(Self::new(
            PartModel { model, texture },
            Vec::new(),
            cell,
            direction,
        ))
    }

    /// A player-style character assembled from look + equipment in action `act`.
    pub fn from_look(
        content: &Content,
        look: &Look,
        act: u32,
        cell: CellPos,
        direction: u8,
    ) -> Result<Self, String> {
        let Character {
            body, attachments, ..
        } = content.assemble_character(look, act)?;
        let attachments = attachments
            .into_iter()
            .map(|a| (a.socket, Part::new(a.part)))
            .collect();
        Ok(Self::new(body, attachments, cell, direction))
    }

    fn new(
        body: PartModel,
        attachments: Vec<(&'static str, Part)>,
        cell: CellPos,
        direction: u8,
    ) -> Self {
        Self {
            cell,
            direction: direction % 8,
            zoom_percent: 100.0,
            body: Part::new(body),
            attachments,
        }
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

    fn group(&self, group: usize) -> &Part {
        if group == 0 {
            &self.body
        } else {
            &self.attachments[group - 1].1
        }
    }

    /// Body animation frame at `secs`; attachments use the same frame (`C3DRole::SetFrame`).
    fn frame(&self, secs: f32) -> f32 {
        secs * MOTION_FPS % self.body.model.frame_count().max(1) as f32
    }

    /// Every visible mesh, body first then attachments.
    pub fn drawables(&self) -> Vec<PartRef> {
        (0..=self.attachments.len())
            .flat_map(|group| {
                let model = &self.group(group).model;
                (0..model.parts.len())
                    .filter(move |&p| model.parts[p].visible)
                    .map(move |part| PartRef { group, part })
            })
            .collect()
    }

    pub fn indices(&self, r: PartRef) -> &[u32] {
        &self.group(r.group).model.parts[r.part].mesh.indices
    }

    /// Vertices of `r` posed by its own clip.
    pub fn posed(&self, r: PartRef, secs: f32) -> Vec<MeshVertex> {
        let part = &self.group(r.group).model.parts[r.part];
        let frame = match &part.clip {
            Some(c) => self.frame(secs) % c.frame_count.max(1) as f32,
            None => 0.0,
        };
        actor::posed_vertices(&part.mesh, part.clip.as_ref(), frame)
    }

    /// World transform of `r`: the role matrix, times the socket's bone for attachments.
    pub fn transform(&self, r: PartRef, role: Mat4, secs: f32) -> Mat4 {
        if r.group == 0 {
            return role;
        }
        let socket = self.attachments[r.group - 1].0;
        let bone = self
            .body
            .model
            .socket_transform(socket, self.frame(secs))
            .unwrap_or(Mat4::IDENTITY);
        role * bone
    }

    /// Texture of `r`, uploaded on first use.
    pub fn texture(
        &mut self,
        r: PartRef,
        renderer: &mut Renderer,
        content: &Content,
    ) -> Option<TextureId> {
        let part = if r.group == 0 {
            &mut self.body
        } else {
            &mut self.attachments[r.group - 1].1
        };
        *part.texture.get_or_insert_with(|| {
            let img = part
                .texture_path
                .as_deref()
                .and_then(|p| content.load_image(p))?;
            Some(renderer.upload_texture(&img))
        })
    }
}
