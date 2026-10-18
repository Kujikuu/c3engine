//! Play mode: the open map played inside the Game view, like Unity's Play button. The game
//! is built from the map as it is now (unsaved edits included) and Stop drops it, so playing
//! never changes the map. Pause freezes game time; Step runs one frame.
//!
//! The game is `game_conquer::screen::GameScreen` (simulation, scripts, world, in-game UI,
//! camera and controls), the same the client runs; Play mode gives it a renderer and pausing.

use crate::doc::MapDoc;
use crate::gpu::GpuView;
use crate::theme;
use crate::tq_plugin::Client;
use c3_scene::CellPos;
use c3_ui::LogLine;
use eframe::egui::{self, Color32, Pos2, Rect};
use eframe::egui_wgpu;
use game_conquer::screen::{GameScreen, MAX_FRAME_SECONDS};
use game_conquer::session::{Session, SessionSetup};
use std::path::PathBuf;

/// Game time one Step runs.
const STEP_SECONDS: f32 = 1.0 / 30.0;

/// What a play session reads from the project.
pub struct PlaySetup {
    pub scripts: Option<PathBuf>,
    pub tables: Option<PathBuf>,
    /// Game map ids of the map being played (script teleports within it work).
    pub map_ids: Vec<u32>,
}

/// A game being played in the Game view.
pub struct PlaySession {
    screen: GameScreen,
    pub paused: bool,
    /// Run one frame while paused.
    step: bool,
    /// The game's own renderer: its textures live there.
    gpu: GpuView,
}

impl PlaySession {
    /// Starts playing `doc` as it is now, the hero on the walkable cell nearest `start`.
    pub fn start(
        client: &Client,
        doc: &MapDoc,
        start: CellPos,
        render_state: &egui_wgpu::RenderState,
        setup: PlaySetup,
    ) -> Result<Self, String> {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_secs());
        let session = Session::start(
            client.shared_content(),
            doc.name().to_owned(),
            doc.scene().clone(),
            SessionSetup {
                scripts: setup.scripts,
                tables: setup.tables,
                map_ids: setup.map_ids,
                start,
                seed,
            },
        )?;
        Ok(Self {
            screen: GameScreen::new(session),
            paused: false,
            step: false,
            gpu: GpuView::new(render_state),
        })
    }

    /// Walks the hero up to NPC `id` and talks to it, as if it was clicked.
    pub fn talk_to(&mut self, id: u32) {
        self.screen.session.talk_to(id);
    }

    /// Sends the hero after the nearest monster it can reach.
    pub fn attack_nearest(&mut self) {
        self.screen.session.attack_nearest();
    }

    /// Opens or closes the bag, as B does.
    pub fn toggle_bag(&mut self) {
        self.screen.session.toggle_bag();
    }

    /// Lines for the Studio's console since the last call.
    pub fn take_log(&mut self) -> Vec<LogLine> {
        self.screen.session.take_log()
    }

    /// Runs one frame of game time (while paused).
    pub fn step(&mut self) {
        self.step = true;
    }

    /// Draws the game into the Game view: the world, the in-game UI and the controls hint.
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

        let ppp = ui.ctx().pixels_per_point();
        let size = [
            (rect.width() * ppp).round() as u32,
            (rect.height() * ppp).round() as u32,
        ];
        let (camera, viewport) = (&self.screen.camera, self.screen.viewport());
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
        self.hints(&painter, rect);
        self.screen.overlay(ui, rect, hovered);
        ui.ctx().request_repaint();
    }

    /// Controls over the game, like a game's own HUD hints.
    fn hints(&self, painter: &egui::Painter, rect: Rect) {
        let run = if self.screen.session.running() {
            "run"
        } else {
            "walk"
        };
        let follow = if self.screen.follow {
            ""
        } else {
            " · Space: follow hero"
        };
        let hint = format!(
            "Click: {run} · Click a monster: attack · Click an NPC: talk · Click a drop: pick up · \
             Tab: attack nearest · R: walk/run · B: bag · Enter: chat · Wheel: zoom{follow}"
        );
        let font = egui::FontId::proportional(12.0);
        let galley = painter.layout_no_wrap(hint, font, theme::TEXT);
        let pad = egui::vec2(8.0, 5.0);
        let at = rect.left_top() + egui::vec2(10.0, 10.0);
        painter.rect_filled(
            Rect::from_min_size(at, galley.size() + pad * 2.0),
            4.0,
            Color32::from_black_alpha(150),
        );
        painter.galley(at + pad, galley, theme::TEXT);
        if self.paused {
            let galley = painter.layout_no_wrap(
                "Paused".into(),
                egui::FontId::proportional(14.0),
                Color32::WHITE,
            );
            let at = rect.right_top() + egui::vec2(-galley.size().x - 26.0, 10.0);
            painter.rect_filled(
                Rect::from_min_size(at, galley.size() + pad * 2.0),
                4.0,
                theme::WARNING.gamma_multiply(0.8),
            );
            painter.galley(at + pad, galley, Color32::WHITE);
        }
    }
}
