//! 3D characters on the map: a body model plus parts attached to its sockets, playing an
//! action and walking cell to cell (`CRole::Walk`/`Step`: one cell per walk or run action,
//! position eased linearly over the action's length).

use c3_render::actor::{self, ActorPose};
use c3_render::{MeshVertex, Renderer, TextureId};
use c3_scene::path::{self, PathRules};
use c3_scene::{CellPos, Scene};
use game_conquer::Content;
use game_conquer::character::{Character, CharacterMotions, Look, PartModel, action};
use glam::{Mat4, Vec2};
use std::collections::{HashMap, VecDeque};

/// Classic `CRole::s_fBaseScale`.
const BASE_SCALE: f32 = 0.75;
/// Frame length for actors without action timing (NPCs, monsters).
const DEFAULT_FRAME_MS: u32 = 33;

/// Addresses one drawable mesh of an actor.
#[derive(Debug, Clone, Copy)]
pub struct PartRef {
    /// 0 = body, n = attachment n-1.
    group: usize,
    part: usize,
}

/// The action being played.
struct Playing {
    action: u32,
    started_ms: u64,
    frame_ms: u32,
}

/// One cell-to-cell step in progress.
struct Step {
    from: CellPos,
    to: CellPos,
    started_ms: u64,
    duration_ms: u64,
}

pub struct Actor {
    /// Cell the actor stands on, or is stepping from.
    pub cell: CellPos,
    /// 0..8, clockwise; the classic facing is `-45 · dir - 45` degrees.
    pub direction: u8,
    /// Monster.dat-style zoom percentage.
    pub zoom_percent: f32,
    /// Runs instead of walking (the client's Run toggle).
    pub running: bool,
    character: Character,
    /// Players switch motions per action; single models keep their own.
    look: Option<Look>,
    /// Per model (body, then attachments): `None` = not uploaded yet.
    textures: Vec<Option<Option<TextureId>>>,
    motions: HashMap<u32, CharacterMotions>,
    playing: Playing,
    step: Option<Step>,
    path: VecDeque<CellPos>,
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
        let character = Character {
            body: PartModel { model, texture },
            attachments: Vec::new(),
            motion_id: None,
        };
        Ok(Self::new(character, None, action::STANDBY, cell, direction))
    }

    /// A player-style character assembled from look + equipment in action `act`.
    pub fn from_look(
        content: &Content,
        look: &Look,
        act: u32,
        cell: CellPos,
        direction: u8,
    ) -> Result<Self, String> {
        let character = content.assemble_character(look, act)?;
        let mut actor = Self::new(character, Some(*look), act, cell, direction);
        actor.playing.frame_ms = content.frame_interval(look, act);
        Ok(actor)
    }

    fn new(
        character: Character,
        look: Option<Look>,
        act: u32,
        cell: CellPos,
        direction: u8,
    ) -> Self {
        Self {
            cell,
            direction: direction % 8,
            zoom_percent: 100.0,
            running: false,
            textures: vec![None; 1 + character.attachments.len()],
            character,
            look,
            motions: HashMap::new(),
            playing: Playing {
                action: act,
                started_ms: 0,
                frame_ms: DEFAULT_FRAME_MS,
            },
            step: None,
            path: VecDeque::new(),
        }
    }

    pub fn turn(&mut self, steps: i8) {
        self.direction = (self.direction as i8 + steps).rem_euclid(8) as u8;
    }

    /// Sets off towards `goal` (or the reachable cell nearest it), after the current step.
    pub fn walk_to(&mut self, goal: CellPos, scene: &Scene) {
        let from = self.step.as_ref().map_or(self.cell, |s| s.to);
        self.path = path::find_path(&scene.grid, from, goal, &PathRules::default()).into();
    }

    /// Advances movement and actions to `now_ms`.
    pub fn update(&mut self, now_ms: u64, scene: &Scene, content: &Content) {
        if let Some(step) = &self.step {
            if now_ms < step.started_ms + step.duration_ms {
                return;
            }
            self.cell = step.to;
            self.step = None;
        }
        match self.path.pop_front() {
            // The map may have changed under a planned path: stop at a blocked step.
            Some(next) if path::can_step(&scene.grid, self.cell, next, &PathRules::default()) => {
                self.direction = path::direction(self.cell, next).unwrap_or(self.direction);
                let act = self.next_move_action();
                self.play(act, now_ms, content);
                let frames = self.character.body.model.frame_count().max(1);
                self.step = Some(Step {
                    from: self.cell,
                    to: next,
                    started_ms: now_ms,
                    duration_ms: u64::from(frames * self.playing.frame_ms),
                });
            }
            Some(_) => self.path.clear(),
            None => {}
        }
        if self.step.is_none() && self.playing.action != action::STANDBY {
            self.play(action::STANDBY, now_ms, content);
        }
    }

    /// Walking always uses the left-foot walk; running alternates feet (`CRole::Step`,
    /// `RunForward`).
    fn next_move_action(&self) -> u32 {
        match (self.running, self.playing.action) {
            (false, _) => action::WALK_L,
            (true, action::RUN_L) => action::RUN_R,
            (true, _) => action::RUN_L,
        }
    }

    /// Starts action `act` from its first frame.
    fn play(&mut self, act: u32, now_ms: u64, content: &Content) {
        if let Some(look) = &self.look {
            let motions = self
                .motions
                .entry(act)
                .or_insert_with(|| content.character_motions(&self.character, look, act));
            self.character.apply_motions(motions);
            self.playing.frame_ms = content.frame_interval(look, act);
        }
        self.playing.action = act;
        self.playing.started_ms = now_ms;
    }

    /// World position of the feet at `now_ms`.
    pub fn position(&self, scene: &Scene, now_ms: u64) -> Vec2 {
        let world = |c| Vec2::from(scene.cell_to_world(c));
        match &self.step {
            Some(s) => {
                let t = now_ms.saturating_sub(s.started_ms) as f32 / s.duration_ms.max(1) as f32;
                world(s.from).lerp(world(s.to), t.min(1.0))
            }
            None => world(self.cell),
        }
    }

    /// Cell the actor is drawn on (draw order), following it mid-step.
    pub fn draw_cell(&self, scene: &Scene, now_ms: u64) -> CellPos {
        let p = self.position(scene, now_ms);
        scene.world_to_cell((p.x, p.y))
    }

    pub fn pose(&self, scene: &Scene, tilt_deg: f32, now_ms: u64) -> ActorPose {
        let p = self.position(scene, now_ms);
        ActorPose {
            position: (p.x, p.y),
            facing_deg: -45.0 * self.direction as f32 - 45.0,
            tilt_deg,
            scale: BASE_SCALE * self.zoom_percent / 100.0,
            height: 0.0,
        }
    }

    fn model(&self, group: usize) -> &PartModel {
        if group == 0 {
            &self.character.body
        } else {
            &self.character.attachments[group - 1].part
        }
    }

    /// Body animation frame at `now_ms`: actions loop from their start; attachments use the
    /// same frame (`C3DRole::SetFrame`).
    fn frame(&self, now_ms: u64) -> f32 {
        let elapsed = now_ms.saturating_sub(self.playing.started_ms) as f32;
        let frames = self.character.body.model.frame_count().max(1) as f32;
        (elapsed / self.playing.frame_ms.max(1) as f32) % frames
    }

    /// Every visible mesh, body first then attachments.
    pub fn drawables(&self) -> Vec<PartRef> {
        (0..=self.character.attachments.len())
            .flat_map(|group| {
                let model = &self.model(group).model;
                (0..model.parts.len())
                    .filter(move |&p| model.parts[p].visible)
                    .map(move |part| PartRef { group, part })
            })
            .collect()
    }

    pub fn indices(&self, r: PartRef) -> &[u32] {
        &self.model(r.group).model.parts[r.part].mesh.indices
    }

    /// Vertices of `r` posed by its own clip.
    pub fn posed(&self, r: PartRef, now_ms: u64) -> Vec<MeshVertex> {
        let part = &self.model(r.group).model.parts[r.part];
        let frame = match &part.clip {
            Some(c) => self.frame(now_ms) % c.frame_count.max(1) as f32,
            None => 0.0,
        };
        actor::posed_vertices(&part.mesh, part.clip.as_ref(), frame)
    }

    /// World transform of `r`: the role matrix, times the socket's bone for attachments.
    pub fn transform(&self, r: PartRef, role: Mat4, now_ms: u64) -> Mat4 {
        if r.group == 0 {
            return role;
        }
        let socket = self.character.attachments[r.group - 1].socket;
        let bone = self
            .character
            .body
            .model
            .socket_transform(socket, self.frame(now_ms))
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
        if let Some(texture) = self.textures[r.group] {
            return texture;
        }
        let texture = (self.model(r.group).texture.as_deref())
            .and_then(|p| content.load_image(p))
            .map(|img| renderer.upload_texture(&img));
        self.textures[r.group] = Some(texture);
        texture
    }
}
