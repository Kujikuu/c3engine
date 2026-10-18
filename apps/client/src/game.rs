//! Playing a game project: the project's map with its NPCs and monsters, its rules and
//! scripts, and the in-game UI — the same game the Studio's Play mode runs, in its own window.

use crate::gfx::Gfx;
use crate::ui_layer::UiLayer;
use c3_scene::{CellPos, Scene};
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

/// What to play.
pub struct GameArgs {
    pub project: String,
    /// An engine scene of the project, or a map file of its content.
    pub map: Option<String>,
    pub map_id: Option<u32>,
    pub cell: Option<CellPos>,
}

/// Opens the game window and plays until it is closed.
pub fn run(args: GameArgs) -> Result<(), String> {
    let project = c3_project::Project::open(&args.project).map_err(|e| e.to_string())?;
    let (_, content_dir) = project
        .tq_client()
        .ok_or("the project has no Conquer content to play")?;
    let content = Rc::new(Content::open(content_dir)?);
    let (path, scene) = load_map(&project, &content, &args)?;
    let g = &scene.grid;
    let start = args
        .cell
        .unwrap_or((g.width as i32 / 2, g.height as i32 / 2));
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_secs());
    let map_ids = content.map_ids_for_path(&path);
    let session = Session::start(
        content,
        path,
        scene,
        SessionSetup {
            scripts: Some(project.scripts_dir()),
            tables: Some(project.tables_dir()),
            map_ids,
            start,
            seed,
        },
    )?;
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

/// The map to play: `--map` names an engine scene or a content map file (with the project's
/// population for it); otherwise `--map-id` (default Twin City) from the content's catalog.
fn load_map(
    project: &c3_project::Project,
    content: &Content,
    args: &GameArgs,
) -> Result<(String, Scene), String> {
    if let Some(name) = &args.map
        && project.map_names().contains(name)
    {
        let scene = c3_scene::native::load_scene(&project.map_file(name))
            .map_err(|e| format!("map {name}: {e}"))?;
        return Ok((name.clone(), scene));
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
    let mut scene = content.load_map(&path)?.scene;
    // The project keeps a content map's NPCs and spawn areas next to its maps.
    let stem = path.rsplit(['/', '\\']).next().unwrap_or(&path);
    let stem = stem.rsplit_once('.').map_or(stem, |(s, _)| s);
    let population = c3_scene::native::load_population(&project.population_file(stem))
        .map_err(|e| e.to_string())?;
    scene.set_population(population);
    Ok((path, scene))
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
