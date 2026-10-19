//! Playing a game project: the project's map with its NPCs and monsters, its rules and
//! scripts, and the in-game UI — the same game the Studio's Play mode runs, in its own window,
//! simulated here or played on a game server. Any project plays: the game comes from
//! `c3-games`, its window, look and numbers from Project Settings.

use crate::front::{FrontOutcome, FrontPhase};
use crate::gfx::Gfx;
use crate::ui_layer::UiLayer;
use c3_core::ProjectSettings;
use c3_play::GameModule;
use c3_play::controls::PlayerControls;
use c3_play::map_list::MapList;
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
    /// Start at the front end (log in, pick a server and a character); `online` then
    /// only types in its server and account.
    pub front: bool,
    /// Chat lines said at the start, in turn.
    pub say: Vec<String>,
}

/// Opens the game window and plays until it is closed.
pub fn run(args: GameArgs) -> Result<(), String> {
    let project = c3_project::Project::open(&args.project).map_err(|e| e.to_string())?;
    let (game, asset_problems) = c3_games::open_project(&project)?;
    for problem in asset_problems {
        eprintln!("warning: {problem}");
    }
    let (settings, problem) = c3_play::settings::load(&project, &*game);
    if let Some(problem) = problem {
        eprintln!("warning: {problem}");
    }
    let icon = c3_play::settings::window_icon(&settings, &*game);
    let (fonts, font_problems) = c3_play::settings::game_fonts(&settings, &*game);
    for problem in font_problems {
        eprintln!("warning: {problem}");
    }
    let maps = MapList::new(&project, &*game, &settings);
    let title = match settings.player.game_name.as_str() {
        "" => project.name().to_owned(),
        name => name.to_owned(),
    };
    let phase = if args.front {
        let online = args.online.as_ref();
        Phase::Front(Box::new(FrontPhase::new(
            Rc::clone(&game),
            &settings,
            title.clone(),
            online.map(|o| o.server.clone()),
            online.map(|o| (o.account.clone(), o.password.clone())),
        )))
    } else {
        let mut session = match &args.online {
            Some(o) => Session::join(
                Rc::clone(&game),
                &project,
                settings.clone(),
                &o.server,
                &o.account,
                &o.password,
            )?,
            None => local_session(&project, Rc::clone(&game), &settings, &maps, &args)?,
        };
        for line in &args.say {
            session.say(line);
        }
        Phase::Game(Box::new(GameScreen::new(
            session,
            PlayerControls::load(&title),
        )))
    };
    let mut app = GameApp {
        phase,
        game,
        title: title.clone(),
        window: WindowSetup {
            title,
            icon,
            fonts: Some(fonts),
        },
        settings,
        project,
        maps,
        gfx: None,
        ui: None,
        last_frame: Instant::now(),
        quit: false,
        fullscreen: None,
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
    maps: &MapList,
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
    let map_ids = maps
        .by_key(&key)
        .map_or_else(|| game.map_ids(&key), |m| m.ids.clone());
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
            breakpoints: Default::default(),
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

/// Before the world (the front end) or in it.
enum Phase {
    Front(Box<FrontPhase>),
    Game(Box<GameScreen>),
}

struct GameApp {
    phase: Phase,
    game: Rc<dyn GameModule>,
    /// The game's name (its controls are kept under it).
    title: String,
    window: WindowSetup,
    settings: ProjectSettings,
    project: c3_project::Project,
    /// The game's maps (portals name them by id).
    maps: MapList,
    gfx: Option<Gfx>,
    ui: Option<UiLayer>,
    last_frame: Instant,
    /// The player quit from the front end.
    quit: bool,
    /// The player's full-screen choice as last seen.
    fullscreen: Option<bool>,
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
        let screen = match &mut self.phase {
            Phase::Game(screen) => screen,
            Phase::Front(front) => {
                let mut outcome = FrontOutcome::Stay;
                let (device, queue) = (gfx.renderer.device().clone(), gfx.renderer.queue().clone());
                front.draw_previews(&device, &queue, ui, seconds);
                let input = ui.take_input(&gfx.window);
                ui.run(&gfx.window, input, |ui| {
                    outcome = front.update(ui, ui.max_rect());
                });
                gfx.draw(&c3_render::Frame::empty(), Some(ui));
                match outcome {
                    FrontOutcome::Stay => {}
                    FrontOutcome::Quit => self.quit = true,
                    FrontOutcome::Play(conn, welcome, updates) => {
                        let entered = Session::entered(
                            Rc::clone(&self.game),
                            &self.project,
                            self.settings.clone(),
                            (conn, welcome, updates),
                        );
                        match entered {
                            Ok(session) => {
                                let controls = PlayerControls::load(&self.title);
                                self.phase =
                                    Phase::Game(Box::new(GameScreen::new(session, controls)));
                            }
                            Err(e) => front.failed(e),
                        }
                    }
                }
                return;
            }
        };
        let mut input = ui.take_input(&gfx.window);
        screen.before_frame(&mut input);
        ui.run(&gfx.window, input, |ui| {
            let rect = ui.max_rect();
            let hovered = screen.update(ui, rect, seconds);
            screen.overlay(ui, rect, hovered);
        });
        screen.follow_map_change(&self.project, &self.maps, &mut gfx.renderer);
        // The player's Settings page decides the window, once they change it.
        let fullscreen = screen.prefs().fullscreen;
        if self
            .fullscreen
            .replace(fullscreen)
            .is_some_and(|was| was != fullscreen)
        {
            gfx.window
                .set_fullscreen(fullscreen.then_some(Fullscreen::Borderless(None)));
        }
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
            WindowEvent::RedrawRequested => {
                self.redraw();
                let left = matches!(&self.phase, Phase::Game(s) if s.left());
                if left || self.quit {
                    event_loop.exit();
                }
            }
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
