//! 3D characters on the map: a body model plus parts attached to its sockets, playing an
//! action and walking cell to cell (`CRole::Walk`/`Step`: one cell per walk or run action,
//! position eased linearly over the action's length).
//!
//! Effects ride along: each part starts its action effects when an action begins (weapon
//! trails on attacks, `C3DRolePart::Set3DEffect`) and draws them in its own space; role
//! effects (level-up, hits) stand at the actor's feet (`CRole::Show`, `m_objEffect`).

use c3_assets::effect::Effect;
use c3_render::actor::{self, ActorPose};
use c3_render::effect::{self as effects_render, EffectMesh};
use c3_render::effect_library::{EffectLibrary, RunningEffect};
use c3_render::{MeshVertex, Renderer, TextureId};
use c3_scene::path::{self, PathRules};
use c3_scene::{CellPos, Scene};
use game_conquer::Content;
use game_conquer::character::{Character, CharacterMotions, Look, PartModel, action};
use glam::{Mat4, Vec2};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

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
    /// Returns to standing after one run through (attacks, emotes).
    once: bool,
}

/// Part action effect that stops all of the part's effects.
const STOP_EFFECTS: &str = "none";

/// Effect geometry carried by an actor, with the effect it belongs to.
pub struct ActorEffectMesh {
    pub mesh: EffectMesh,
    pub effect: Rc<Effect>,
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
    /// Per model (body, then attachments): effects started by actions.
    part_effects: Vec<Vec<RunningEffect>>,
    /// Effects standing at the actor's feet.
    role_effects: Vec<RunningEffect>,
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
            body: PartModel {
                model,
                texture,
                type_id: None,
            },
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
            part_effects: (0..=character.attachments.len())
                .map(|_| Vec::new())
                .collect(),
            role_effects: Vec::new(),
            character,
            look,
            motions: HashMap::new(),
            playing: Playing {
                action: act,
                started_ms: 0,
                frame_ms: DEFAULT_FRAME_MS,
                once: false,
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

    /// Plays `effect` at the actor's feet.
    pub fn add_effect(&mut self, effect: RunningEffect) {
        self.role_effects.push(effect);
    }

    /// Advances movement, actions and effects to `now_ms`.
    pub fn update(
        &mut self,
        now_ms: u64,
        scene: &Scene,
        content: &Content,
        library: &mut EffectLibrary,
    ) {
        for effects in self.part_effects.iter_mut().chain([&mut self.role_effects]) {
            effects.retain(|e| !e.is_finished(now_ms));
        }
        let mut stepped = false;
        if let Some(step) = &self.step {
            if now_ms < step.started_ms + step.duration_ms {
                return;
            }
            self.cell = step.to;
            self.step = None;
            stepped = true;
        }
        match self.path.pop_front() {
            // The map may have changed under a planned path: stop at a blocked step.
            Some(next) if path::can_step(&scene.grid, self.cell, next, &PathRules::default()) => {
                self.direction = path::direction(self.cell, next).unwrap_or(self.direction);
                let act = self.next_move_action();
                self.play(act, now_ms, content, library);
                self.step = Some(Step {
                    from: self.cell,
                    to: next,
                    started_ms: now_ms,
                    duration_ms: self.action_ms(),
                });
            }
            Some(_) => self.path.clear(),
            None => {}
        }
        let run_through = now_ms >= self.playing.started_ms + self.action_ms();
        if self.step.is_none() {
            if stepped || (self.playing.once && run_through) {
                self.play(action::STANDBY, now_ms, content, library);
            } else if run_through {
                // Each run through restarts the action, as repeated actions do in the client
                // (an attack's weapon trail swings again).
                self.play(self.playing.action, now_ms, content, library);
            }
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

    /// Length of one run through the current action.
    fn action_ms(&self) -> u64 {
        u64::from(self.character.body.model.frame_count().max(1) * self.playing.frame_ms)
    }

    /// Plays action `act` once, then stands (when not walking).
    pub fn perform(
        &mut self,
        act: u32,
        now_ms: u64,
        content: &Content,
        library: &mut EffectLibrary,
    ) {
        if self.step.is_none() {
            self.play(act, now_ms, content, library);
            self.playing.once = true;
        }
    }

    /// Starts action `act` from its first frame (looping), with its part effects.
    pub fn play(&mut self, act: u32, now_ms: u64, content: &Content, library: &mut EffectLibrary) {
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
        self.playing.once = false;
        for group in 0..self.part_effects.len() {
            let Some(type_id) = self.model(group).type_id else {
                continue;
            };
            // Like 5065: an effect keeps playing until it ends, "none" stops them all.
            for name in content.part_action_effects(type_id, act) {
                let effects = &mut self.part_effects[group];
                if name.eq_ignore_ascii_case(STOP_EFFECTS) {
                    effects.clear();
                } else if !effects.iter().any(|e| e.name() == name) {
                    effects.extend(library.start(&name, content, now_ms));
                }
            }
        }
    }

    /// This frame's effect geometry: part effects in their part's space (`role` · socket),
    /// role effects at the feet (`CGame3DEffectEx::Show(posWorld)`).
    pub fn effect_meshes(
        &mut self,
        scene: &Scene,
        tilt_deg: f32,
        now_ms: u64,
        eye: glam::Vec3,
    ) -> Vec<ActorEffectMesh> {
        let role = self.pose(scene, tilt_deg, now_ms).transform();
        let feet = self.position(scene, now_ms);
        let placements: Vec<Mat4> = (0..self.part_effects.len())
            .map(|group| self.transform(PartRef { group, part: 0 }, role, now_ms))
            .collect();
        let mut out = Vec::new();
        for (effects, placement) in self.part_effects.iter_mut().zip(placements) {
            for e in effects {
                let effect = e.shared();
                out.extend(e.meshes(now_ms, placement, eye).into_iter().map(|mesh| {
                    ActorEffectMesh {
                        mesh,
                        effect: Rc::clone(&effect),
                    }
                }));
            }
        }
        for e in &mut self.role_effects {
            let effect = e.shared();
            let placement = effects_render::ground_placement(
                &effect,
                (feet.x, feet.y),
                0.0,
                tilt_deg,
                glam::Vec3::ZERO,
                glam::Vec3::ONE,
            );
            out.extend(
                e.meshes(now_ms, placement, eye)
                    .into_iter()
                    .map(|mesh| ActorEffectMesh {
                        mesh,
                        effect: Rc::clone(&effect),
                    }),
            );
        }
        out
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
