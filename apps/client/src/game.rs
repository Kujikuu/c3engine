//! Playing a game project: the project's map with its NPCs and monsters, its rules and
//! scripts, and the in-game UI — the same game the Studio's Play mode runs, in its own window,
//! simulated here or played on a game server.

use crate::gfx::Gfx;
use crate::ui_layer::UiLayer;
use c3_scene::CellPos;
use game_conquer::Content;
use game_conquer::screen::{GameScreen, MAX_FRAME_SECONDS};
use game_conquer::session::{Session, SessionSetup};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

/// Twin City.
const DEFAULT_MAP_ID: u32 = 1002;

/// A game server to play on.
pub struct Online {
    /// `host:port`.
    pub server: String,
    pub account: String,
    pub password: String,
}

/// What to play.
pub struct GameArgs {
    pub project: String,
    /// An engine scene of the project, or a map file of its content.
    pub map: Option<String>,
    pub map_id: Option<u32>,
    pub cell: Option<CellPos>,
    /// Play on a server (which picks the map) instead of here.
    pub online: Option<Online>,
}

/// Opens the game window and plays until it is closed.
pub fn run(args: GameArgs) -> Result<(), String> {
    let project = c3_project::Project::open(&args.project).map_err(|e| e.to_string())?;
    let (_, content_dir) = project
        .tq_client()
        .ok_or("the project has no Conquer content to play")?;
    let content = Rc::new(Content::open(content_dir)?);
    let session = match &args.online {
        Some(o) => Session::join(content, &project, &o.server, &o.account, &o.password)?,
        None => local_session(&project, content, &args)?,
    };
    let mut app = GameApp {
        screen: GameScreen::new(session),
        title: format!("{} — C3 Client", project.name()),
        gfx: None,
        ui: None,
        last_frame: Instant::now(),
    };
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}

/// A game simulated here on the map `--map` names (an engine scene or a content map file),
/// else `--map-id` (default Twin City) from the content's catalog.
fn local_session(
    project: &c3_project::Project,
    content: Rc<Content>,
    args: &GameArgs,
) -> Result<Session, String> {
    let key = match &args.map {
        Some(map) => map.clone(),
        None => {
            let id = args.map_id.unwrap_or(DEFAULT_MAP_ID);
            content
                .map_path(id)
                .ok_or(format!("map {id} is not in GameMap.dat"))?
        }
    };
    let scene = game_conquer::game::load_map(project, &content, &key)?;
    let g = &scene.grid;
    let start = args
        .cell
        .unwrap_or((g.width as i32 / 2, g.height as i32 / 2));
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_secs());
    let map_ids = content.map_ids_for_path(&key);
    Session::start(
        content,
        key,
        scene,
        SessionSetup {
            scripts: Some(project.scripts_dir()),
            tables: Some(project.tables_dir()),
            map_ids,
            start,
            seed,
        },
    )
}

struct GameApp {
    screen: GameScreen,
    title: String,
    gfx: Option<Gfx>,
    ui: Option<UiLayer>,
    last_frame: Instant,
}

impl GameApp {
    fn redraw(&mut self) {
        let now = Instant::now();
        let seconds = now
            .duration_since(self.last_frame)
            .as_secs_f32()
            .min(MAX_FRAME_SECONDS);
        self.last_frame = now;
        let (Some(gfx), Some(ui)) = (&mut self.gfx, &mut self.ui) else {
            return;
        };
        let screen = &mut self.screen;
        ui.run(&gfx.window, |ui| {
            let rect = ui.max_rect();
            let hovered = screen.update(ui, rect, seconds);
            screen.overlay(ui, rect, hovered);
        });
        let viewport = screen.viewport();
        let camera = &screen.camera;
        let view_rect = camera.view_rect(viewport);
        let world = &mut screen.session.world;
        world.prepare(&mut gfx.renderer, camera, viewport);
        let meshes = world.mesh_draws();
        gfx.draw(&world.frame(&meshes, view_rect), Some(ui));
        for line in screen.session.take_log() {
            match line {
                c3_ui::LogLine::Info(text) => println!("{text}"),
                c3_ui::LogLine::Error(text) => eprintln!("warning: {text}"),
            }
        }
    }
}

impl ApplicationHandler for GameApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(&self.title)
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        let made = event_loop
            .create_window(attrs)
            .map_err(|e| format!("cannot create window: {e}"))
            .and_then(|w| Gfx::new(Arc::new(w)).map_err(|e| format!("graphics init failed: {e}")));
        match made {
            Ok(gfx) => {
                self.ui = Some(UiLayer::new(
                    &gfx.window,
                    gfx.renderer.device(),
                    gfx.format(),
                ));
                self.gfx = Some(gfx);
            }
            Err(e) => {
                eprintln!("{e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let (Some(gfx), Some(ui)) = (&self.gfx, &mut self.ui) {
            ui.on_event(&gfx.window, &event);
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(g) = &mut self.gfx {
                    g.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }
}
