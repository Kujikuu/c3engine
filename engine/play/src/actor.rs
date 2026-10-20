//! 3D characters on the map: a body model plus parts attached to its sockets, playing an
//! action and walking cell to cell (one cell per walk or run action, position eased linearly
//! over the action's length, as the classic `CRole::Walk`/`Step`). The game decides how they
//! look and move ([`crate::Looks`]).
//!
//! Effects ride along: each part starts its action effects when an action begins (weapon
//! trails on attacks, `C3DRolePart::Set3DEffect`) and draws them in its own space; role
//! effects (level-up, hits) stand at the actor's feet (`CRole::Show`, `m_objEffect`).

use crate::character::{Character, CharacterMotions, Look, PartModel};
use crate::module::{Act, GameModule};
use c3_assets::effect::Effect;
use c3_render::actor::{self, ActorPose};
use c3_render::effect::{self as effects_render, EffectMesh};
use c3_render::effect_library::{EffectLibrary, RunningEffect};
use c3_render::scene_view::{PosedIndices, Poser};
use c3_render::{MeshVertex, Renderer, TextureId};
use c3_scene::path::{self, PathRules};
use c3_scene::{CellPos, Scene};
use glam::{Mat4, Vec2};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

/// Classic `CRole::s_fBaseScale`.
const BASE_SCALE: f32 = 0.75;

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
    /// Stays on the last frame after one run through (dying).
    hold: bool,
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
    /// A jump: how high its arc peaks (pixels), and its facing along its path (degrees).
    peak: f32,
    facing: Option<f32>,
}

/// An action beginning or ending on a cell, for the effect it leaves on the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapMark {
    pub act: u32,
    pub cell: CellPos,
    pub at_end: bool,
}

/// The shortest frame of a jump (`CRole::GetFrameInterval`'s clamp, ms).
const JUMP_FRAME_MS: u32 = 33;

pub struct Actor {
    /// Cell the actor stands on, or is stepping from.
    pub cell: CellPos,
    /// 0..8, clockwise; the classic facing is `-45 · dir - 45` degrees.
    pub direction: u8,
    /// Drawing size in percent.
    pub zoom_percent: f32,
    /// Runs instead of walking (the client's Run toggle).
    pub running: bool,
    /// Drawn this far (map pixels) from its cell, and this high above the ground (a dice
    /// table's desk and dice).
    pub offset: Vec2,
    pub rise: f32,
    /// Sorted in the painter's order as if this far from where it is drawn (dice drawn over
    /// their desk).
    pub order_offset: Vec2,
    /// Shown on this frame of its model, not animated (a resting die's face).
    pub hold_frame: Option<u32>,
    /// Not drawn.
    pub hidden: bool,
    character: Character,
    look: Look,
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
    /// Actions begun and ended since last taken ([`Self::take_map_marks`]).
    map_marks: Vec<MapMark>,
    /// A jump asked mid-jump, made on landing (TQ queues the command).
    next_jump: Option<CellPos>,
}

impl Actor {
    /// An actor of `look` already built (NPCs share one character per look), standing.
    pub fn new(
        game: &dyn GameModule,
        character: Character,
        look: Look,
        cell: CellPos,
        direction: u8,
    ) -> Self {
        let looks = game.looks();
        let act = looks.action(Act::Stand);
        let frame_ms = looks.frame_ms(&look, act);
        Self::build(character, look, act, frame_ms, cell, direction)
    }

    /// Frees the textures this actor uploaded.
    pub fn release_textures(&mut self, renderer: &mut Renderer) {
        for texture in self.textures.iter_mut() {
            if let Some(Some(id)) = texture.take() {
                renderer.release_texture(id);
            }
        }
    }

    /// An actor of `look` in action `act`, its models built by the game.
    pub fn from_look(
        game: &dyn GameModule,
        look: &Look,
        act: u32,
        cell: CellPos,
        direction: u8,
    ) -> Result<Self, String> {
        let looks = game.looks();
        let character = looks.character(look, act)?;
        let frame_ms = looks.frame_ms(look, act);
        Ok(Self::build(
            character,
            look.clone(),
            act,
            frame_ms,
            cell,
            direction,
        ))
    }

    fn build(
        character: Character,
        look: Look,
        act: u32,
        frame_ms: u32,
        cell: CellPos,
        direction: u8,
    ) -> Self {
        Self {
            cell,
            direction: direction % 8,
            zoom_percent: look.zoom_percent,
            running: false,
            offset: Vec2::ZERO,
            rise: 0.0,
            order_offset: Vec2::ZERO,
            hold_frame: None,
            hidden: false,
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
                frame_ms,
                once: false,
                hold: false,
            },
            step: None,
            path: VecDeque::new(),
            map_marks: Vec::new(),
            next_jump: None,
        }
    }

    /// What the actor looks like.
    pub fn look(&self) -> &Look {
        &self.look
    }

    /// The models the actor is drawn with.
    pub fn character(&self) -> &Character {
        &self.character
    }

    pub fn turn(&mut self, steps: i8) {
        self.direction = (self.direction as i8 + steps).rem_euclid(8) as u8;
    }

    /// Sets off towards `goal` (or the reachable cell nearest it), after the current step.
    pub fn walk_to(&mut self, goal: CellPos, scene: &Scene) {
        let from = self.step.as_ref().map_or(self.cell, |s| s.to);
        self.path = path::find_path(&scene.grid, from, goal, &PathRules::default()).into();
    }

    /// Puts the actor on `cell` at once (a teleport), dropping its steps.
    pub fn place(&mut self, cell: CellPos) {
        self.cell = cell;
        self.step = None;
        self.path.clear();
    }

    /// Jumps to `to` (TQ's `CRole::Jump`): the jump action played once over the way, its
    /// frames longer the farther it goes (the action's frame time × distance / 300 + 1, 33 ms
    /// at least), along an arc peaking a third of the distance plus 20 pixels up midway
    /// (`GetJumpHeight` over flat ground).
    pub fn jump(
        &mut self,
        to: CellPos,
        now_ms: u64,
        scene: &Scene,
        game: &dyn GameModule,
        library: &mut EffectLibrary,
    ) {
        self.path.clear();
        if self.step.as_ref().is_some_and(|s| s.peak > 0.0) {
            self.next_jump = Some(to);
            return;
        }
        let from = self.position(scene, now_ms);
        let land = Vec2::from(scene.cell_to_world(to));
        self.cell = scene.world_to_cell((from.x, from.y));
        self.face(to);
        let jump = game.looks().action(Act::Jump);
        self.play(jump, now_ms, game, library);
        let far = from.distance(land);
        let frame_ms = (self.playing.frame_ms as f32 * far / 300.0 + 1.0) as u32;
        self.playing.frame_ms = frame_ms.max(JUMP_FRAME_MS);
        self.step = Some(Step {
            from: self.cell,
            to,
            started_ms: now_ms,
            duration_ms: self.action_ms(),
            peak: far / 3.0 + 20.0,
            // The body turns to the path itself, not one of eight ways (`CRole::Show`).
            facing: Some(-270.0 - (land.y - from.y).atan2(land.x - from.x).to_degrees()),
        });
    }

    /// Notes the playing action beginning or ending where the actor stands.
    fn mark(&mut self, at_end: bool) {
        let cell = self.step.as_ref().map_or(self.cell, |s| s.from);
        self.map_marks.push(MapMark {
            act: self.playing.action,
            cell,
            at_end,
        });
    }

    /// The actions begun and ended since last asked.
    pub fn take_map_marks(&mut self) -> Vec<MapMark> {
        std::mem::take(&mut self.map_marks)
    }

    /// Its shadow, a `size` picture on the ground under it (TQ's `CRole::Show`: scaled by
    /// 1.5% of the zoom, a quarter of its height above the feet, at 70%; it stays on the
    /// ground through a jump). Roles cast one, plain models don't.
    pub fn shadow(&self, scene: &Scene, now_ms: u64, size: Vec2) -> Option<[f32; 4]> {
        use crate::character::LookKind;
        let casts = matches!(
            self.look.kind,
            LookKind::Player | LookKind::Monster | LookKind::Npc
        );
        let feet = self.position(scene, now_ms);
        let size = size * self.zoom_percent * 0.015;
        casts.then_some([feet.x - size.x / 2.0, feet.y - size.y / 4.0, size.x, size.y])
    }

    /// How high above the ground the actor is drawn at `now_ms` (mid-jump).
    pub fn lift(&self, now_ms: u64) -> f32 {
        self.rise
            + match &self.step {
                Some(s) if s.peak > 0.0 => {
                    let t =
                        now_ms.saturating_sub(s.started_ms) as f32 / s.duration_ms.max(1) as f32;
                    s.peak * (std::f32::consts::PI * t.min(1.0)).sin()
                }
                _ => 0.0,
            }
    }

    /// Whether the actor is stepping or has steps planned.
    pub fn is_moving(&self) -> bool {
        self.step.is_some() || !self.path.is_empty()
    }

    /// Turns to face `cell` (when standing elsewhere).
    pub fn face(&mut self, cell: CellPos) {
        if let Some(d) = path::facing(self.cell, cell) {
            self.direction = d;
        }
    }

    /// Stops the effects the actor carries.
    pub fn clear_effects(&mut self) {
        for effects in self.part_effects.iter_mut() {
            effects.clear();
        }
        self.role_effects.clear();
    }

    /// Stops the effects named `name` it carries.
    pub fn remove_effect(&mut self, name: &str) {
        self.role_effects.retain(|e| e.name() != name);
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
        game: &dyn GameModule,
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
            self.mark(true);
            if let Some(to) = self.next_jump.take() {
                return self.jump(to, now_ms, scene, game, library);
            }
        }
        match self.path.pop_front() {
            // The map may have changed under a planned path: stop at a blocked step.
            Some(next) if path::can_step(&scene.grid, self.cell, next, &PathRules::default()) => {
                self.direction = path::direction(self.cell, next).unwrap_or(self.direction);
                let act = self.next_move_action(game);
                self.play(act, now_ms, game, library);
                self.step = Some(Step {
                    from: self.cell,
                    to: next,
                    started_ms: now_ms,
                    duration_ms: self.action_ms(),
                    peak: 0.0,
                    facing: None,
                });
            }
            Some(_) => self.path.clear(),
            None => {}
        }
        let run_through = now_ms >= self.playing.started_ms + self.action_ms();
        if self.step.is_none() && !self.playing.hold {
            if run_through && !stepped {
                self.mark(true);
            }
            if stepped || (self.playing.once && run_through) {
                let stand = game.looks().action(Act::Stand);
                self.play(stand, now_ms, game, library);
            } else if run_through {
                // Each run through restarts the action, as repeated actions do in the classic
                // client (an attack's weapon trail swings again).
                self.play(self.playing.action, now_ms, game, library);
            }
        }
    }

    /// Walking always uses the one walk; running alternates feet (`CRole::Step`, `RunForward`).
    fn next_move_action(&self, game: &dyn GameModule) -> u32 {
        let looks = game.looks();
        let run = looks.action(Act::Run);
        match (self.running, self.playing.action) {
            (false, _) => looks.action(Act::Walk),
            (true, a) if a == run => looks.action(Act::RunAlt),
            (true, _) => run,
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
        game: &dyn GameModule,
        library: &mut EffectLibrary,
    ) {
        if self.step.is_none() {
            self.play(act, now_ms, game, library);
            self.playing.once = true;
        }
    }

    /// Plays action `act` once and stays on its last frame (dying).
    pub fn play_and_hold(
        &mut self,
        act: u32,
        now_ms: u64,
        game: &dyn GameModule,
        library: &mut EffectLibrary,
    ) {
        self.play(act, now_ms, game, library);
        self.playing.hold = true;
    }

    /// Starts action `act` from its first frame (looping), with its part effects.
    pub fn play(
        &mut self,
        act: u32,
        now_ms: u64,
        game: &dyn GameModule,
        library: &mut EffectLibrary,
    ) {
        let looks = game.looks();
        let motions = self
            .motions
            .entry(act)
            .or_insert_with(|| looks.motions(&self.character, &self.look, act));
        self.character.apply_motions(motions);
        self.playing.frame_ms = looks.frame_ms(&self.look, act);
        self.playing.action = act;
        self.playing.started_ms = now_ms;
        self.playing.once = false;
        self.playing.hold = false;
        self.mark(false);
        for group in 0..self.part_effects.len() {
            let Some(key) = self.model(group).effect_key else {
                continue;
            };
            // As in the classic client: an effect keeps playing until it ends, "none" stops
            // them all.
            for name in looks.part_effects(key, act) {
                let effects = &mut self.part_effects[group];
                if name.eq_ignore_ascii_case(STOP_EFFECTS) {
                    effects.clear();
                } else if !effects.iter().any(|e| e.name() == name) {
                    effects.extend(library.start(&name, game.assets(), now_ms));
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
        let world = |c| Vec2::from(scene.cell_to_world(c)) + self.offset;
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
        let p = self.position(scene, now_ms) + self.order_offset;
        scene.world_to_cell((p.x, p.y))
    }

    pub fn pose(&self, scene: &Scene, tilt_deg: f32, now_ms: u64) -> ActorPose {
        let p = self.position(scene, now_ms);
        ActorPose {
            position: (p.x, p.y - self.lift(now_ms)),
            facing_deg: (self.step.as_ref().and_then(|s| s.facing))
                .unwrap_or(-45.0 * self.direction as f32 - 45.0),
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
        if let Some(frame) = self.hold_frame {
            return frame as f32;
        }
        let elapsed = now_ms.saturating_sub(self.playing.started_ms) as f32;
        let frames = self.character.body.model.frame_count().max(1) as f32;
        let frame = elapsed / self.playing.frame_ms.max(1) as f32;
        if self.playing.hold {
            frame.min(frames - 1.0)
        } else {
            frame % frames
        }
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
        let socket = &self.character.attachments[r.group - 1].socket;
        let bone = self
            .character
            .body
            .model
            .socket_transform(socket, self.frame(now_ms))
            .unwrap_or(Mat4::IDENTITY);
        role * bone
    }

    /// The world rectangle (min x, min y, max x, max y) its posed meshes cover on screen
    /// (a role's world z is the screen's y; see `actor_view`).
    pub fn world_bounds(&self, scene: &Scene, tilt_deg: f32, now_ms: u64) -> Option<[f32; 4]> {
        let role = self.pose(scene, tilt_deg, now_ms).transform();
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for part in self.drawables() {
            let m = self.transform(part, role, now_ms);
            for v in self.posed(part, now_ms) {
                let p = m.transform_point3(glam::Vec3::from(v.position));
                b = [b[0].min(p.x), b[1].min(p.z), b[2].max(p.x), b[3].max(p.z)];
            }
        }
        (b[0] <= b[2]).then_some(b)
    }

    /// Texture of `r`, uploaded on first use.
    pub fn texture(
        &mut self,
        r: PartRef,
        renderer: &mut Renderer,
        game: &dyn GameModule,
    ) -> Option<TextureId> {
        if let Some(texture) = self.textures[r.group] {
            return texture;
        }
        let texture = (self.model(r.group).texture.as_deref())
            .and_then(|p| game.assets().image(p))
            .map(|img| renderer.upload_texture(&img));
        self.textures[r.group] = Some(texture);
        texture
    }
}

/// An actor's mesh whose indices stay in the actor's model (the scene view's external key).
#[derive(Debug, Clone, Copy)]
pub struct ActorPart {
    actor: usize,
    part: PartRef,
}

impl ActorPart {
    /// The indices `key` stands for, in `actors`.
    pub fn indices<'a>(actors: &'a [Actor], key: &ActorPart) -> &'a [u32] {
        actors[key.actor].indices(key.part)
    }
}

/// Poses actor `a` into the scene view: its parts, then the effects it carries, which share
/// its depth slice so the body hides what is behind it.
#[allow(clippy::too_many_arguments)]
pub fn pose_actor(
    actors: &mut [Actor],
    a: usize,
    poser: &mut Poser<ActorPart>,
    scene: &Scene,
    game: &dyn GameModule,
    tilt_deg: f32,
    now_ms: u64,
    eye: glam::Vec3,
) {
    if actors[a].hidden {
        return;
    }
    let role = actors[a].pose(scene, tilt_deg, now_ms).transform();
    for part in actors[a].drawables() {
        let actor = &mut actors[a];
        let texture = actor.texture(part, poser.renderer(), game);
        poser.tinted(
            actor.posed(part, now_ms),
            PosedIndices::External(ActorPart { actor: a, part }),
            texture,
            actor.transform(part, role, now_ms),
            actor.model(part.group).tint,
        );
    }
    for m in actors[a].effect_meshes(scene, tilt_deg, now_ms, eye) {
        poser.effect(&m.effect, m.mesh);
    }
}
