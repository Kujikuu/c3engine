//! A look shown by itself (character creation's bodies): a tiny world with the one role
//! standing in it, drawn into its own target for a UI to show.

use crate::actor::Actor;
use crate::module::Act;
use crate::world::World;
use crate::{GameModule, Look};
use c3_render::camera::Camera2d;
use c3_render::{RenderTarget, Renderer};
use c3_scene::{CellPos, Grid, IsoProjection, Scene};
use glam::Vec2;
use std::rc::Rc;

/// Cells on a side of the preview's floor.
const CELLS: u32 = 9;
/// How far above its cell the camera looks before the role is measured.
const LIFT: f32 = 50.0;
/// Of the picture, how much the role fills on its longer side.
const FILL: f32 = 0.85;

pub struct LookPreview {
    world: World,
    camera: Camera2d,
    target: Option<RenderTarget>,
    /// Whether the camera has been fitted to the role yet.
    fitted: bool,
    tilt_deg: f32,
    /// Why the look couldn't be made, if it couldn't.
    pub problem: Option<String>,
}

impl LookPreview {
    /// `look` of `game`, standing and facing the viewer.
    pub fn new(game: Rc<dyn GameModule>, look: &Look, tilt_deg: f32) -> Self {
        let scene = Scene {
            grid: Grid::new(CELLS, CELLS),
            projection: IsoProjection::for_grid(64.0, 32.0, CELLS),
            ground: None,
            portals: Vec::new(),
            entities: Vec::new(),
            layers: Vec::new(),
            npcs: Vec::new(),
            spawns: Vec::new(),
            regions: Vec::new(),
            traps: Vec::new(),
            meta: Default::default(),
        };
        let mid = (CELLS / 2) as i32;
        let cell: CellPos = (mid, mid);
        let center = Vec2::from(scene.cell_to_world(cell)) - Vec2::new(0.0, LIFT);
        let stand = game.looks().action(Act::Stand);
        let mut world = World::new(Rc::clone(&game), "Preview".into(), scene, tilt_deg);
        let problem = match Actor::from_look(&*game, look, stand, cell, 1) {
            Ok(actor) => {
                world.actors.push(actor);
                world.hero_action(stand, false);
                None
            }
            Err(e) => Some(e),
        };
        Self {
            world,
            camera: Camera2d { center, zoom: 1.6 },
            target: None,
            fitted: false,
            tilt_deg,
            problem,
        }
    }

    /// Advances it `seconds` and draws it at `size` pixels; the target to show.
    pub fn render(
        &mut self,
        renderer: &mut Renderer,
        size: [u32; 2],
        seconds: f32,
    ) -> &RenderTarget {
        self.world.advance(seconds);
        self.world.tick();
        let viewport = Vec2::new(size[0] as f32, size[1] as f32);
        if !self.fitted {
            self.fit(viewport);
        }
        self.world.prepare(renderer, &self.camera, viewport);
        renderer.ensure_target(&mut self.target, size);
        let target = self.target.as_ref().expect("target ensured");
        let meshes = self.world.mesh_draws();
        let mut frame = self.world.frame(&meshes, self.camera.view_rect(viewport));
        // Clear, so the role stands on whatever the UI draws behind it.
        frame.clear = [0.0; 4];
        renderer.render(target, &frame);
        target
    }

    /// Plays action `act`, looping (a statue's pose).
    pub fn play(&mut self, act: u32) {
        let now = self.world.now_ms();
        if let Some(actor) = self.world.actors.first_mut() {
            actor.hold_frame = None;
        }
        self.world.actor_action(0, act, false);
        let _ = now;
    }

    /// Stops the role on the frame it shows, or plays on.
    pub fn set_paused(&mut self, paused: bool) {
        let now = self.world.now_ms();
        if let Some(actor) = self.world.actors.first_mut() {
            actor.hold_frame = paused.then(|| actor.shown_frame(now));
        }
    }

    /// Turns the role `steps` eighths.
    pub fn turn(&mut self, steps: i8) {
        if let Some(actor) = self.world.actors.first_mut() {
            actor.turn(steps);
        }
    }

    /// The role's facing, action and the frame it shows.
    pub fn pose(&self) -> (u8, u32, u32) {
        let now = self.world.now_ms();
        (self.world.actors.first())
            .map_or((0, 0, 0), |a| (a.direction, a.action(), a.shown_frame(now)))
    }

    /// Frames the role: centred, filling `FILL` of the picture whatever its model's size.
    fn fit(&mut self, viewport: Vec2) {
        self.fitted = true;
        let Some(actor) = self.world.actors.first() else {
            return;
        };
        let now = self.world.now_ms();
        let Some(b) = actor.world_bounds(&self.world.scene, self.tilt_deg, now) else {
            return;
        };
        let extent = Vec2::new(b[2] - b[0], b[3] - b[1]).max(Vec2::splat(1.0));
        self.camera.center = Vec2::new(b[0] + b[2], b[1] + b[3]) / 2.0;
        self.camera.zoom = (viewport * FILL / extent).min_element();
    }
}
