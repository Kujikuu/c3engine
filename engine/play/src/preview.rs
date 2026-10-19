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
/// How far above its cell the camera looks (the role's middle, not its feet).
const LIFT: f32 = 50.0;

pub struct LookPreview {
    world: World,
    camera: Camera2d,
    target: Option<RenderTarget>,
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
        self.world.prepare(renderer, &self.camera, viewport);
        renderer.ensure_target(&mut self.target, size);
        let target = self.target.as_ref().expect("target ensured");
        let meshes = self.world.mesh_draws();
        let frame = self.world.frame(&meshes, self.camera.view_rect(viewport));
        renderer.render(target, &frame);
        target
    }
}
