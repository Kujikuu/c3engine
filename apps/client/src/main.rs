//! C3 Client: the game runtime window.
//! Usage: c3-client [client_dir | --project <folder>] [--map map/map/xxx.DMap | <project map>
//!                   | --map-id 1002] [--cell x,y] [--zoom 1.0]
//!                   [--actor <3dobj id> | --player look,hair,armor,armet,rweapon,lweapon]
//!                   [--action 100] [--dir 0-7] [--effect <3DEffect.ini name>] [--walk-to x,y] [--run]
//!
//! A hero (the `--player`/`--actor`, else a default player) stands on the start cell.
//! Controls: click to walk there · R toggles run · Enter attacks · Space follows the hero · drag
//! or WASD/arrows to pan · wheel to zoom · F to fit the map · 1 for 100% · Q/E turn the hero ·
//! Esc to quit. `--effect` plays on the hero.

mod actors;
mod camera;
mod effects;
mod gfx;
mod world;

use c3_scene::path;
use camera::Camera2d;
use game_conquer::Content;
use game_conquer::character::{Look, action};
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
/// Screen pixels the pointer may move before a click becomes a drag.
const CLICK_SLOP: f32 = 4.0;
/// Hero when none is given: a newbie with hair, armor and a blade.
const DEFAULT_HERO: Look = Look {
    look: 3,
    hair: 310,
    armor: 130003,
    armet: 0,
    r_weapon: 410003,
    l_weapon: 0,
};

struct App {
    world: World,
    gfx: Option<gfx::Gfx>,
    camera: Camera2d,
    fit_pending: bool,
    /// The camera keeps the hero centred.
    follow: bool,
    /// Left button: where it went down, and whether it has turned into a drag.
    press: Option<(Vec2, bool)>,
    /// Right button drag anchor.
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
        if dir != Vec2::ZERO {
            self.follow = false;
            self.camera.center += dir.normalize_or_zero() * PAN_SPEED * dt / self.camera.zoom;
        }
        self.world.tick();
        if self.fit_pending {
            self.camera.fit(self.world.bounds(), self.viewport());
            self.fit_pending = false;
            self.follow = false;
        }
        if self.follow
            && let Some(hero) = self.world.hero()
        {
            self.camera.center = hero.position(&self.world.scene, self.world.now_ms());
        }
    }

    fn pan(&mut self, by: Vec2) {
        self.camera.center -= by / self.camera.zoom;
        self.follow = false;
    }

    /// Status shown in the title bar.
    fn status(&self) -> String {
        let Some(hero) = self.world.hero() else {
            return String::new();
        };
        format!(
            " — {}{}",
            if hero.running { "Run (R)" } else { "Walk (R)" },
            if self.follow {
                ""
            } else {
                " — Space: follow hero"
            }
        )
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
            let status = self.status();
            let Some(gfx) = &self.gfx else { return };
            gfx.window.set_title(&format!(
                "C3 Client — {name} — {:.0}% — {fps:.0} fps{status}",
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
                KeyCode::Space => self.follow = self.world.hero().is_some(),
                KeyCode::Enter => self.world.hero_action(action::ATTACK_0, true),
                KeyCode::KeyR => {
                    if let Some(hero) = self.world.hero_mut() {
                        hero.running = !hero.running;
                    }
                }
                KeyCode::KeyQ | KeyCode::KeyE => {
                    if let Some(hero) = self.world.hero_mut() {
                        hero.turn(if code == KeyCode::KeyQ { -1 } else { 1 });
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
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => self.press = Some((self.cursor, false)),
                ElementState::Released => {
                    // A click (not a drag) sends the hero there.
                    if let Some((at, false)) = self.press.take() {
                        let target = self.camera.screen_to_world(at, self.viewport());
                        self.world.walk_hero_to(target);
                    }
                }
            },
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Right,
                ..
            } => {
                self.drag_from = (state == ElementState::Pressed).then_some(self.cursor);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let previous = self.cursor;
                self.cursor = Vec2::new(position.x as f32, position.y as f32);
                if let Some((at, dragging)) = &mut self.press
                    && (*dragging || at.distance(self.cursor) > CLICK_SLOP)
                {
                    *dragging = true;
                    self.pan(self.cursor - previous);
                }
                if let Some(from) = self.drag_from {
                    self.pan(self.cursor - from);
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
    /// Game project to play: its maps, with art from its Conquer client source.
    project: Option<String>,
    map: Option<String>,
    map_id: Option<u32>,
    /// Start centred on this cell instead of fitting the whole map.
    cell: Option<(i32, i32)>,
    zoom: Option<f32>,
    /// Model id (3dobj.ini) of an actor to place at the start cell.
    actor: Option<u64>,
    /// Player look + equipment to assemble at the start cell.
    player: Option<Look>,
    action: u32,
    direction: u8,
    /// Effect to play at the start cell.
    effect: Option<String>,
    /// Cell the hero sets off to at start.
    walk_to: Option<(i32, i32)>,
    /// Start with running on.
    run: bool,
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
        project: flag("--project"),
        cell: flag("--cell").and_then(|v| parse_cell(&v)),
        zoom: flag("--zoom").and_then(|v| v.parse().ok()),
        actor: flag("--actor").and_then(|v| v.parse().ok()),
        player: flag("--player").and_then(|v| parse_look(&v)),
        action: flag("--action")
            .and_then(|v| v.parse().ok())
            .unwrap_or(action::STANDBY),
        direction: flag("--dir").and_then(|v| v.parse().ok()).unwrap_or(0),
        effect: flag("--effect"),
        walk_to: flag("--walk-to").and_then(|v| parse_cell(&v)),
        run: args.iter().any(|a| a == "--run"),
    }
}

/// `x,y`.
fn parse_cell(v: &str) -> Option<(i32, i32)> {
    let (x, y) = v.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// `look,hair,armor,armet,rweapon,lweapon` (trailing fields may be omitted).
fn parse_look(v: &str) -> Option<Look> {
    let n: Vec<u32> = v
        .split(',')
        .map(|f| f.trim().parse().ok())
        .collect::<Option<_>>()?;
    let at = |i: usize| n.get(i).copied().unwrap_or(0);
    Some(Look {
        look: at(0),
        hair: at(1),
        armor: at(2),
        armet: at(3),
        r_weapon: at(4),
        l_weapon: at(5),
    })
}

fn load_world(args: &Args) -> Result<World, String> {
    if let Some(dir) = &args.project {
        return load_project_world(dir, args);
    }
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

/// Plays a project map: `--map` names an engine scene or a map file of the project's content
/// (e.g. `map/map/newplain.DMap`); otherwise `--map-id` (default Twin City) from its catalog.
fn load_project_world(dir: &str, args: &Args) -> Result<World, String> {
    let project = c3_project::Project::open(dir).map_err(|e| e.to_string())?;
    let (_, content_dir) = project
        .tq_client()
        .ok_or("the project has no Conquer content to play")?;
    let content = Content::open(content_dir)?;
    if let Some(name) = &args.map
        && project.map_names().contains(name)
    {
        let scene = c3_scene::native::load_scene(&project.map_file(name))
            .map_err(|e| format!("map {name}: {e}"))?;
        return Ok(World::new(content, name.clone(), scene));
    }
    let path = match &args.map {
        Some(path) => path.clone(),
        None => {
            let id = args.map_id.unwrap_or(DEFAULT_MAP_ID);
            content
                .map_path(id)
                .ok_or(format!("map {id} is not in GameMap.dat"))?
        }
    };
    let imported = content.load_map(&path)?;
    Ok(World::new(content, path, imported.scene))
}

fn main() {
    let args = parse_args();
    let mut world = load_world(&args).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });
    let g = &world.scene.grid;
    let cell = args
        .cell
        .unwrap_or((g.width as i32 / 2, g.height as i32 / 2));
    // The hero stands on the walkable cell nearest the start.
    let hero_cell = path::nearest_walkable(&world.scene.grid, cell, 50).unwrap_or(cell);
    let hero = match (&args.player, args.actor) {
        (None, Some(id)) => {
            actors::Actor::from_model(&world.content, id, hero_cell, args.direction)
        }
        (look, _) => actors::Actor::from_look(
            &world.content,
            look.as_ref().unwrap_or(&DEFAULT_HERO),
            args.action,
            hero_cell,
            args.direction,
        ),
    };
    match hero {
        Ok(mut a) => {
            a.running = args.run;
            if let Some(goal) = args.walk_to {
                a.walk_to(goal, &world.scene);
            }
            world.actors.push(a);
        }
        Err(e) => eprintln!("warning: hero: {e}"),
    }
    // Starts the hero's action properly, with its part effects.
    world.hero_action(args.action, false);
    if let Some(name) = &args.effect
        && let Err(e) = world.hero_effect(name, true)
    {
        eprintln!("warning: {e}");
    }
    let has_hero = !world.actors.is_empty();
    let start = Vec2::from(world.scene.cell_to_world(hero_cell));
    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App {
        world,
        gfx: None,
        camera: Camera2d {
            center: start,
            zoom: args.zoom.unwrap_or(1.0),
        },
        fit_pending: !has_hero && args.cell.is_none() && args.zoom.is_none(),
        follow: has_hero,
        press: None,
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
