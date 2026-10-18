//! Play mode: the open map played inside the Game view, like Unity's Play button. The game
//! is built from the map as it is now (unsaved edits included) and Stop drops it, so playing
//! never changes the map. Pause freezes game time; Step runs one frame.
//!
//! The game is `c3_play::screen::GameScreen` (simulation, scripts, world, in-game UI, camera
//! and controls) with the project's game module, the same the client runs; Play mode gives it
//! a renderer and pausing.
//! Played *online*, a local game server runs the map on its own thread and the Game view is
//! its client, as players will play it.

use crate::doc::MapDoc;
use crate::gpu::GpuView;
use crate::icons;
use crate::theme;
use c3_play::GameModule;
use c3_play::controls::PlayerControls;
use c3_play::map_list::MapList;
use c3_play::screen::{GameScreen, MAX_FRAME_SECONDS};
use c3_play::serve::{ServeSetup, ServedMap, ServerHandle};
use c3_play::session::{Session, SessionSetup};
use c3_scene::CellPos;
use c3_ui::LogLine;
use eframe::egui::{self, Color32, Pos2, Rect};
use eframe::egui_wgpu;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::rc::Rc;

/// Game time one Step runs.
const STEP_SECONDS: f32 = 1.0 / 30.0;

/// What a play session reads from the project.
pub struct PlaySetup {
    pub scripts: Option<PathBuf>,
    pub tables: Option<PathBuf>,
    /// Game map ids of the map being played (script teleports within it work).
    pub map_ids: Vec<u32>,
    /// Play through a local game server.
    pub online: bool,
    /// The project's settings.
    pub settings: c3_core::ProjectSettings,
    /// The open project (portals to its other maps take the hero there).
    pub project: Option<c3_project::Project>,
    /// The game's maps.
    pub maps: MapList,
}

/// The account Play mode logs in to its local server with.
const PLAY_ACCOUNT: &str = "player";

/// A game being played in the Game view.
pub struct PlaySession {
    screen: GameScreen,
    pub paused: bool,
    /// Run one frame while paused.
    step: bool,
    /// The game's own renderer: its textures live there.
    gpu: GpuView,
    /// The local server, when playing online.
    server: Option<ServerHandle>,
    project: Option<c3_project::Project>,
    maps: MapList,
}

impl PlaySession {
    /// Starts playing `doc` as it is now, the hero on the walkable cell nearest `start`.
    pub fn start(
        game: Rc<dyn GameModule>,
        doc: &MapDoc,
        start: CellPos,
        render_state: &egui_wgpu::RenderState,
        setup: PlaySetup,
    ) -> Result<Self, String> {
        let project = setup.project.clone();
        let maps = setup.maps.clone();
        let (session, server) = if setup.online {
            let (session, server) = Self::join_local_server(game, doc, start, setup)?;
            (session, Some(server))
        } else {
            let session = Session::start(
                game,
                doc.name().to_owned(),
                doc.scene().clone(),
                SessionSetup {
                    scripts: setup.scripts,
                    tables: setup.tables,
                    map_ids: setup.map_ids,
                    start,
                    seed: c3_play::game::time_seed(),
                    settings: setup.settings,
                },
            )?;
            (session, None)
        };
        Ok(Self {
            screen: GameScreen::new(session, PlayerControls::session_only()),
            paused: false,
            step: false,
            gpu: GpuView::new(render_state),
            server,
            project,
            maps,
        })
    }

    /// Starts a local server on `doc` (a fresh account folder each time) and joins it.
    fn join_local_server(
        game: Rc<dyn GameModule>,
        doc: &MapDoc,
        start: CellPos,
        setup: PlaySetup,
    ) -> Result<(Session, ServerHandle), String> {
        let accounts = std::env::temp_dir().join(format!("c3-play-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&accounts);
        let server = ServerHandle::spawn(
            vec![ServedMap {
                key: doc.name().to_owned(),
                scene: doc.scene().clone(),
                map_ids: setup.map_ids,
                start,
            }],
            ServeSetup {
                listen: "127.0.0.1:0".into(),
                scripts: setup.scripts,
                tables: setup.tables.clone(),
                accounts,
                rules: game.rules(),
                settings: setup.settings.clone(),
                fronts: Vec::new(),
            },
        )?;
        let session = Session::join_scene(
            game,
            doc.scene().clone(),
            setup.tables,
            setup.settings,
            &server.addr().to_string(),
            PLAY_ACCOUNT,
            PLAY_ACCOUNT,
        )?;
        Ok((session, server))
    }

    /// The local server's address, when playing online.
    pub fn server_addr(&self) -> Option<SocketAddr> {
        self.server.as_ref().map(ServerHandle::addr)
    }

    /// Reads the gamepads before the frame (they move between the game's buttons).
    pub fn before_frame(&mut self, raw: &mut egui::RawInput) {
        self.screen.before_frame(raw);
    }

    /// Whether the player chose Leave the game in the game's menu.
    pub fn left(&self) -> bool {
        self.screen.left()
    }

    /// Shows the on-screen touch controls and plays with the mouse as a finger.
    pub fn simulate_touch(&mut self, on: bool) {
        self.screen.simulate_touch = on;
    }

    /// Plays by `settings` from now on.
    pub fn set_settings(&mut self, settings: c3_core::ProjectSettings) {
        self.screen.set_settings(settings);
    }

    /// Walks the hero up to NPC `id` and talks to it, as if it was clicked.
    pub fn talk_to(&mut self, id: u32) {
        self.screen.session.talk_to(id);
    }

    /// Walks the hero to `cell`, as if it was clicked.
    pub fn walk_to(&mut self, cell: CellPos) {
        self.screen.session.walk_to(cell);
    }

    /// Sends the hero after the nearest monster it can reach.
    pub fn attack_nearest(&mut self) {
        self.screen.session.attack_nearest();
    }

    /// Opens or closes the bag, as B does.
    pub fn toggle_bag(&mut self) {
        self.screen.session.toggle_bag();
    }

    /// Opens or closes the quest log, as L does.
    pub fn toggle_quests(&mut self) {
        self.screen.session.toggle_quests();
    }

    /// Reads the game's art and sounds again (the project's assets changed on disk).
    pub fn reload_assets(&mut self) {
        self.screen.session.reload_assets(&mut self.gpu.renderer);
    }

    /// Opens the game menu on its Controls page.
    pub fn open_controls(&mut self) {
        self.screen.session.open_controls();
    }

    /// Says `line` in the game's chat, as typed.
    pub fn say(&mut self, line: &str) {
        self.screen.session.say(line);
    }

    /// Lines for the Studio's console since the last call.
    pub fn take_log(&mut self) -> Vec<LogLine> {
        let mut log = self.screen.session.take_log();
        if let Some(server) = &self.server {
            log.extend(
                server
                    .take_log()
                    .into_iter()
                    .map(|line| LogLine::Info(format!("Server: {line}"))),
            );
        }
        log
    }

    /// Runs one frame of game time (while paused).
    pub fn step(&mut self) {
        self.step = true;
    }

    /// Draws the game into the Game view: the world and the in-game UI.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        ui.allocate_rect(rect, egui::Sense::hover());
        let dt = ui.input(|i| i.stable_dt).min(MAX_FRAME_SECONDS);
        let seconds = if !self.paused {
            dt
        } else if std::mem::take(&mut self.step) {
            STEP_SECONDS
        } else {
            0.0
        };
        let hovered = self.screen.update(ui, rect, seconds);
        if let Some(project) = &self.project {
            self.screen
                .follow_map_change(project, &self.maps, &mut self.gpu.renderer);
        }

        let ppp = ui.ctx().pixels_per_point();
        let size = [
            (rect.width() * ppp).round() as u32,
            (rect.height() * ppp).round() as u32,
        ];
        let (camera, viewport) = (&self.screen.view_camera(), self.screen.viewport());
        let view_rect = camera.view_rect(viewport);
        let world = &mut self.screen.session.world;
        world.prepare(&mut self.gpu.renderer, camera, viewport);
        let meshes = world.mesh_draws();
        let frame = world.frame(&meshes, view_rect);
        let texture = self.gpu.render(size, &frame);
        let painter = ui.painter_at(rect);
        painter.image(
            texture,
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
        self.screen.overlay(ui, rect, hovered);
        self.badges(ui, rect);
        ui.ctx().request_repaint();
    }

    /// What sits over the game: Paused, and the touch simulation toggle.
    fn badges(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let pad = egui::vec2(8.0, 5.0);
        let painter = ui.painter_at(rect);
        let toggle = Rect::from_min_size(
            rect.right_top() + egui::vec2(-40.0, 8.0),
            egui::vec2(30.0, 26.0),
        );
        let resp = ui
            .put(
                toggle,
                egui::Button::new(egui::RichText::new(icons::HAND_TAP).size(16.0))
                    .selected(self.screen.simulate_touch),
            )
            .on_hover_text(
                "Simulate touch: show the on-screen controls and play with the mouse as a finger",
            );
        if resp.clicked() {
            self.screen.simulate_touch ^= true;
        }
        if self.paused {
            let galley = painter.layout_no_wrap(
                "Paused".into(),
                egui::FontId::proportional(14.0),
                Color32::WHITE,
            );
            let at = toggle.left_top() + egui::vec2(-galley.size().x - pad.x * 2.0 - 8.0, 0.0);
            painter.rect_filled(
                Rect::from_min_size(at, galley.size() + pad * 2.0),
                4.0,
                theme::WARNING.gamma_multiply(0.8),
            );
            painter.galley(at + pad, galley, Color32::WHITE);
        }
    }
}
