//! Playing a game project: the project's map with its NPCs and monsters, its rules and
//! scripts, and the in-game UI — the same game the Studio's Play mode runs, in its own window,
//! simulated here or played on a game server. Any project plays: the game comes from
//! `c3-games`, its window, look and numbers from Project Settings.

use crate::gfx::Gfx;
use crate::ui_layer::UiLayer;
use c3_core::ProjectSettings;
use c3_play::GameModule;
use c3_play::screen::{GameScreen, MAX_FRAME_SECONDS};
use c3_play::session::{Session, SessionSetup};
use c3_scene::CellPos;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Fullscreen, Icon, Window, WindowId};

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
    pub cell: Option<CellPos>,
    /// Play on a server (which picks the map) instead of here.
    pub online: Option<Online>,
}

/// Opens the game window and plays until it is closed.
pub fn run(args: GameArgs) -> Result<(), String> {
    let project = c3_project::Project::open(&args.project).map_err(|e| e.to_string())?;
    let game = c3_games::open_project(&project)?;
    let (settings, problem) = c3_play::settings::load(&project, &*game);
    if let Some(problem) = problem {
        eprintln!("warning: {problem}");
    }
    let icon = c3_play::settings::window_icon(&settings, &*game);
    let (fonts, font_problems) = c3_play::settings::game_fonts(&settings, &*game);
    for problem in font_problems {
        eprintln!("warning: {problem}");
    }
    let session = match &args.online {
        Some(o) => Session::join(
            Rc::clone(&game),
            &project,
            settings.clone(),
            &o.server,
            &o.account,
            &o.password,
        )?,
        None => local_session(&project, Rc::clone(&game), &settings, &args)?,
    };
    let title = match settings.player.game_name.as_str() {
        "" => project.name().to_owned(),
        name => name.to_owned(),
    };
    let mut app = GameApp {
        screen: GameScreen::new(session),
        window: WindowSetup {
            title,
            icon,
            fonts: Some(fonts),
        },
        settings,
        gfx: None,
        ui: None,
        last_frame: Instant::now(),
    };
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}

/// A game simulated here on the map `--map` names (an engine scene or a content map file),
/// else the game's first map.
fn local_session(
    project: &c3_project::Project,
    game: Rc<dyn GameModule>,
    settings: &ProjectSettings,
    args: &GameArgs,
) -> Result<Session, String> {
    let key = match &args.map {
        Some(map) => map.clone(),
        None => c3_play::settings::start_map(&settings.gameplay, project, &*game)
            .ok_or("the project has no map to play; make one in the Studio")?,
    };
    let scene = game.load_map(project, &key)?;
    let start =
        (args.cell).unwrap_or_else(|| c3_play::settings::start_cell(&settings.gameplay, &scene));
    let map_ids = game.map_ids(&key);
    Session::start(
        game,
        key,
        scene,
        SessionSetup {
            scripts: Some(project.scripts_dir()),
            tables: Some(project.tables_dir()),
            map_ids,
            start,
            seed: c3_play::game::time_seed(),
            settings: settings.clone(),
        },
    )
}

/// What the window is opened with.
struct WindowSetup {
    title: String,
    icon: Option<c3_assets::Image>,
    /// The game's fonts, until the UI layer takes them.
    fonts: Option<Vec<c3_ui::fonts::GameFont>>,
}

struct GameApp {
    screen: GameScreen,
    window: WindowSetup,
    settings: ProjectSettings,
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
        let camera = &screen.view_camera();
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
        let player = &self.settings.player;
        let icon = (self.window.icon.as_ref())
            .and_then(|i| Icon::from_rgba(i.rgba.clone(), i.width, i.height).ok());
        let attrs = Window::default_attributes()
            .with_title(&self.window.title)
            .with_window_icon(icon)
            .with_resizable(player.resizable)
            .with_fullscreen(player.fullscreen.then_some(Fullscreen::Borderless(None)))
            .with_inner_size(winit::dpi::LogicalSize::new(
                player.window_width,
                player.window_height,
            ));
        let vsync = self.settings.graphics.vsync;
        let made = event_loop
            .create_window(attrs)
            .map_err(|e| format!("cannot create window: {e}"))
            .and_then(|w| {
                Gfx::new(Arc::new(w), vsync).map_err(|e| format!("graphics init failed: {e}"))
            });
        match made {
            Ok(gfx) => {
                let ui = UiLayer::new(&gfx.window, gfx.renderer.device(), gfx.format());
                let fonts = self.window.fonts.take().unwrap_or_default();
                c3_ui::fonts::install(&ui.ctx, fonts);
                self.ui = Some(ui);
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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(g) = &self.gfx else { return };
        match self.settings.graphics.frame_cap {
            0 => g.window.request_redraw(),
            cap => {
                // Draw again once a frame's time has passed since the last one.
                let next = self.last_frame + Duration::from_secs_f64(1.0 / f64::from(cap));
                if Instant::now() >= next {
                    g.window.request_redraw();
                } else {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(next));
                }
            }
        }
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        if let (winit::event::StartCause::ResumeTimeReached { .. }, Some(g)) = (cause, &self.gfx) {
            event_loop.set_control_flow(ControlFlow::Poll);
            g.window.request_redraw();
        }
    }
}
