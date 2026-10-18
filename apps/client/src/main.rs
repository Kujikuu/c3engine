//! C3 Client: the game runtime window.
//! Usage: c3-client [client_dir] [--map map/map/xxx.DMap | --map-id 1002] [--cell x,y] [--zoom 1.0]
//!                   [--actor <3dobj id> [--dir 0-7]]
//!
//! Controls: drag or WASD/arrows to pan · wheel to zoom · F to fit · 1 for 100% · Q/E turn the actor ·
//! Esc to quit.

mod actors;
mod camera;
mod gfx;
mod world;

use camera::Camera2d;
use game_conquer::Content;
use glam::Vec2;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};
use world::World;

const DEFAULT_CLIENT: &str = r"D:\Conquer2\Tools\eosource\client5065";
/// Twin City.
const DEFAULT_MAP_ID: u32 = 1002;
/// World pixels per second when panning with the keyboard, at zoom 1.
const PAN_SPEED: f32 = 900.0;

struct App {
    world: World,
    gfx: Option<gfx::Gfx>,
    camera: Camera2d,
    fit_pending: bool,
    drag_from: Option<Vec2>,
    cursor: Vec2,
    keys_down: Vec<KeyCode>,
    last_frame: Instant,
    fps: FpsCounter,
}

#[derive(Default)]
struct FpsCounter {
    frames: u32,
    since: Option<Instant>,
}

impl FpsCounter {
    /// Counts a frame; returns the FPS about once per second.
    fn tick(&mut self, now: Instant) -> Option<f32> {
        let since = *self.since.get_or_insert(now);
        self.frames += 1;
        let elapsed = now.duration_since(since).as_secs_f32();
        if elapsed < 1.0 {
            return None;
        }
        let fps = self.frames as f32 / elapsed;
        self.frames = 0;
        self.since = Some(now);
        Some(fps)
    }
}

impl App {
    fn viewport(&self) -> Vec2 {
        self.gfx.as_ref().map_or(Vec2::ONE, |g| {
            let [w, h] = g.size();
            Vec2::new(w as f32, h as f32)
        })
    }

    fn update(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        let mut dir = Vec2::ZERO;
        for k in &self.keys_down {
            match k {
                KeyCode::KeyA | KeyCode::ArrowLeft => dir.x -= 1.0,
                KeyCode::KeyD | KeyCode::ArrowRight => dir.x += 1.0,
                KeyCode::KeyW | KeyCode::ArrowUp => dir.y -= 1.0,
                KeyCode::KeyS | KeyCode::ArrowDown => dir.y += 1.0,
                _ => {}
            }
        }
        self.camera.center += dir.normalize_or_zero() * PAN_SPEED * dt / self.camera.zoom;
        if self.fit_pending {
            self.camera.fit(self.world.bounds(), self.viewport());
            self.fit_pending = false;
        }
    }

    fn redraw(&mut self) {
        self.update();
        let viewport = self.viewport();
        let Some(gfx) = &mut self.gfx else { return };
        self.world
            .prepare(&mut gfx.renderer, &self.camera, viewport);
        let meshes = self.world.mesh_draws();
        gfx.draw(&self.world.frame(&meshes, &self.camera, viewport));
        if let Some(fps) = self.fps.tick(Instant::now()) {
            let name = self
                .world
                .map_path
                .rsplit('/')
                .next()
                .unwrap_or(&self.world.map_path);
            gfx.window.set_title(&format!(
                "C3 Client — {name} — {:.0}% — {fps:.0} fps",
                self.camera.zoom * 100.0
            ));
        }
    }

    fn key(&mut self, code: KeyCode, pressed: bool, event_loop: &ActiveEventLoop) {
        if pressed {
            match code {
                KeyCode::Escape => event_loop.exit(),
                KeyCode::KeyF => self.fit_pending = true,
                KeyCode::Digit1 => self.camera.zoom = 1.0,
                KeyCode::KeyQ | KeyCode::KeyE => {
                    if let Some(a) = self.world.actors.first_mut() {
                        a.turn(if code == KeyCode::KeyQ { -1 } else { 1 });
                    }
                }
                _ => {}
            }
            if !self.keys_down.contains(&code) {
                self.keys_down.push(code);
            }
        } else {
            self.keys_down.retain(|k| *k != code);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("C3 Client")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("cannot create window: {e}");
                event_loop.exit();
                return;
            }
        };
        match gfx::Gfx::new(window) {
            Ok(g) => self.gfx = Some(g),
            Err(e) => {
                eprintln!("graphics init failed: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(g) = &mut self.gfx {
                    g.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    self.key(code, event.state == ElementState::Pressed, event_loop);
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left | MouseButton::Right,
                ..
            } => {
                self.drag_from = (state == ElementState::Pressed).then_some(self.cursor);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Vec2::new(position.x as f32, position.y as f32);
                if let Some(from) = self.drag_from {
                    self.camera.center -= (self.cursor - from) / self.camera.zoom;
                    self.drag_from = Some(self.cursor);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 60.0,
                };
                self.camera
                    .zoom_at(1.15f32.powf(steps), self.cursor, self.viewport());
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }
}

struct Args {
    client: String,
    map: Option<String>,
    map_id: Option<u32>,
    /// Start centred on this cell instead of fitting the whole map.
    cell: Option<(i32, i32)>,
    zoom: Option<f32>,
    /// Model id (3dobj.ini) of an actor to place at the start cell.
    actor: Option<u64>,
    direction: u8,
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    Args {
        client: args
            .first()
            .filter(|a| !a.starts_with("--"))
            .cloned()
            .unwrap_or_else(|| DEFAULT_CLIENT.to_owned()),
        map: flag("--map"),
        map_id: flag("--map-id").and_then(|v| v.parse().ok()),
        cell: flag("--cell").and_then(|v| {
            let (x, y) = v.split_once(',')?;
            Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
        }),
        zoom: flag("--zoom").and_then(|v| v.parse().ok()),
        actor: flag("--actor").and_then(|v| v.parse().ok()),
        direction: flag("--dir").and_then(|v| v.parse().ok()).unwrap_or(0),
    }
}

fn load_world(args: &Args) -> Result<World, String> {
    let content = Content::open(&args.client)?;
    let path = match &args.map {
        Some(p) => p.clone(),
        None => {
            let id = args.map_id.unwrap_or(DEFAULT_MAP_ID);
            content
                .map_path(id)
                .ok_or(format!("map {id} is not in GameMap.dat"))?
        }
    };
    let imported = content.load_map(&path)?;
    for w in &imported.warnings {
        eprintln!("warning: {w}");
    }
    Ok(World::new(content, path, imported.scene))
}

fn main() {
    let args = parse_args();
    let world = load_world(&args).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });
    let mut world = world;
    if let Some(id) = args.actor {
        let g = &world.scene.grid;
        let cell = args
            .cell
            .unwrap_or((g.width as i32 / 2, g.height as i32 / 2));
        match actors::Actor::load(&world.content, id, cell, args.direction) {
            Ok(a) => world.actors.push(a),
            Err(e) => eprintln!("warning: actor {id}: {e}"),
        }
    }
    let start = args.cell.map(|c| Vec2::from(world.scene.cell_to_world(c)));
    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App {
        world,
        gfx: None,
        camera: Camera2d {
            center: start.unwrap_or(Vec2::ZERO),
            zoom: args.zoom.unwrap_or(1.0),
        },
        fit_pending: start.is_none() && args.zoom.is_none(),
        drag_from: None,
        cursor: Vec2::ZERO,
        keys_down: Vec::new(),
        last_frame: Instant::now(),
        fps: FpsCounter::default(),
    };
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("event loop error: {e}");
    }
}
