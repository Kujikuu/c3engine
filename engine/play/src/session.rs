//! A game being played: the engine's simulation with the game's rules and the project's
//! scripts, shown in a [`World`] with the in-game UI. The simulation runs here
//! ([`Session::start`]) or on a game server this session mirrors ([`Session::join`]); either
//! way the view and UI read the same `Sim`. The Studio's Play mode and the client host it;
//! they own the window, camera and renderer and pass input in.

use crate::audio::GameAudio;
use crate::cursor::CursorKind;
use crate::game::{self, GameSetup};
use crate::module::GameModule;
use crate::settings;
use crate::sim_view::SimView;
use crate::world::{self, World};
use c3_game::{EntityId, Event, Input, Sim, WorldUpdate};
use c3_online::{Connection, RemoteEvent, Welcome};
use c3_project::{Project, ProjectSettings};
use c3_render::camera::Camera2d;
use c3_scene::{CellPos, Scene, path};
use c3_ui::{ControlsView, GameUi, GameView, Icons, LogLine, UiAction, style};
use egui::{Pos2, Rect, TextureHandle, TextureId};
use glam::Vec2;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

/// How far (cells) the hero may start from the asked cell.
const START_SEARCH: i32 = 50;
/// World pixels above an actor's feet where its name and numbers go.
const HEAD_HEIGHT: f32 = 110.0;

/// Where the game runs.
enum Link {
    Local,
    /// On a server; the session's `Sim` mirrors it.
    Remote(Connection),
}

/// A session's parts, gathered before it opens.
struct Opening {
    map_path: String,
    scene: Scene,
    sim: Sim,
    hero: EntityId,
    map_ids: Vec<u32>,
    problems: Vec<String>,
    link: Link,
    settings: ProjectSettings,
}

/// Where a session finds the project's game and where the hero starts.
pub struct SessionSetup {
    pub scripts: Option<PathBuf>,
    pub tables: Option<PathBuf>,
    /// Game map ids of the map (script teleports within it work).
    pub map_ids: Vec<u32>,
    pub start: CellPos,
    pub seed: u64,
    /// The project's settings.
    pub settings: ProjectSettings,
}

/// How the game view sits on screen this frame.
pub struct Screen<'a> {
    pub rect: Rect,
    pub camera: &'a Camera2d,
}

impl Screen<'_> {
    fn viewport(&self) -> Vec2 {
        Vec2::new(self.rect.width(), self.rect.height())
    }

    pub fn to_world(&self, p: Pos2) -> Vec2 {
        let local = Vec2::new(p.x - self.rect.left(), p.y - self.rect.top());
        self.camera.screen_to_world(local, self.viewport())
    }

    pub fn to_screen(&self, world: Vec2) -> Pos2 {
        let p = self.camera.world_to_screen(world, self.viewport());
        self.rect.min + egui::vec2(p.x, p.y)
    }
}

pub struct Session {
    pub sim: Sim,
    pub hero: EntityId,
    pub world: World,
    view: SimView,
    ui: GameUi,
    icons: ItemIcons,
    /// The game's equipment slots and their names.
    slots: Vec<(u8, String)>,
    audio: GameAudio,
    log: Vec<LogLine>,
    link: Link,
    settings: ProjectSettings,
    /// When (UI seconds) the hero was last hit.
    hit_at: Option<f64>,
    /// The monster the hero has in its sights (gamepad and touch targeting).
    target: Option<EntityId>,
}

impl Session {
    /// Starts a game on `scene` (map `map_path` of `game`), simulated here.
    pub fn start(
        game: Rc<dyn GameModule>,
        map_path: String,
        scene: Scene,
        setup: SessionSetup,
    ) -> Result<Self, String> {
        let start = path::nearest_walkable(&scene.grid, setup.start, START_SEARCH)
            .ok_or("there is no walkable cell near the start")?;
        let game_setup = GameSetup {
            scripts: setup.scripts,
            tables: setup.tables,
            map_ids: setup.map_ids.clone(),
            seed: setup.seed,
            settings: setup.settings.gameplay.clone(),
        };
        let (mut sim, problems) = game::new_sim(&scene, &game_setup, &*game.rules());
        let hero = sim.add_saved_player(&settings::new_character(
            &setup.settings.gameplay,
            "Hero",
            start,
        ));
        Self::open(
            game,
            Opening {
                map_path,
                scene,
                sim,
                hero,
                map_ids: setup.map_ids,
                problems,
                link: Link::Local,
                settings: setup.settings,
            },
        )
    }

    /// Joins the game server at `addr` as `account` (a new name makes the account), showing
    /// the map of `project` it puts the player on.
    pub fn join(
        game: Rc<dyn GameModule>,
        project: &Project,
        settings: ProjectSettings,
        addr: &str,
        account: &str,
        password: &str,
    ) -> Result<Self, String> {
        let timeout = join_timeout(&settings);
        let (conn, welcome, updates) = Connection::join(addr, account, password, timeout)?;
        let scene = game.load_map(project, &welcome.map)?;
        Self::joined(
            game,
            scene,
            Some(project.tables_dir()),
            settings,
            (conn, welcome, updates),
        )
    }

    /// Joins the game server at `addr`, which serves `scene` (the Studio's local server
    /// plays the map as it is being edited); `tables` are the project's.
    pub fn join_scene(
        game: Rc<dyn GameModule>,
        scene: Scene,
        tables: Option<PathBuf>,
        settings: ProjectSettings,
        addr: &str,
        account: &str,
        password: &str,
    ) -> Result<Self, String> {
        let joined = Connection::join(addr, account, password, join_timeout(&settings))?;
        Self::joined(game, scene, tables, settings, joined)
    }

    fn joined(
        game: Rc<dyn GameModule>,
        scene: Scene,
        tables: Option<PathBuf>,
        settings: ProjectSettings,
        (conn, welcome, updates): (Connection, Welcome, Vec<WorldUpdate>),
    ) -> Result<Self, String> {
        let (mut sim, problems) =
            game::mirror_sim(&scene, tables.as_ref(), &*game.rules(), &settings.gameplay);
        for update in updates {
            sim.apply_update(update);
        }
        Self::open(
            game,
            Opening {
                map_path: welcome.map,
                scene,
                sim,
                hero: welcome.hero,
                map_ids: welcome.map_ids,
                problems,
                link: Link::Remote(conn),
                settings,
            },
        )
    }

    fn open(game: Rc<dyn GameModule>, o: Opening) -> Result<Self, String> {
        let Opening {
            map_path,
            scene,
            mut sim,
            hero,
            map_ids,
            problems,
            link,
            settings,
        } = o;
        let mut log: Vec<LogLine> = problems.into_iter().map(LogLine::Error).collect();
        // What happened while setting up is shown by attaching the view; keep the warnings.
        for event in sim.take_events() {
            if let Event::Warning(text) = event {
                log.push(LogLine::Error(text));
            }
        }
        let mut audio = GameAudio::new(Rc::clone(&game), map_ids);
        audio.apply(&settings.audio);
        let slots = game.slot_names();
        let tilt = settings::tilt_deg(&settings, &*game);
        let mut world = World::new(Rc::clone(&game), map_path, scene, tilt);
        world.background = world::linear_background(settings.graphics.background);
        let view = SimView::attach(&sim, hero, &mut world)?;
        let mut ui = GameUi::default();
        ui.system(
            "Welcome! Walk around, fight monsters and talk to NPCs: the line at the top names your controls. /help lists the commands.",
            style::GOLD,
        );
        if matches!(link, Link::Remote(_)) {
            ui.system("Connected to the server.", style::GOOD);
        }
        Ok(Self {
            sim,
            hero,
            world,
            view,
            ui,
            icons: ItemIcons::new(game),
            slots,
            audio,
            log,
            link,
            settings,
            hit_at: None,
            target: None,
        })
    }

    pub fn settings(&self) -> &ProjectSettings {
        &self.settings
    }

    /// Plays by `settings` from now on: the simulation's numbers, sound and look. (The tilt
    /// and the start apply to the next session.)
    pub fn set_settings(&mut self, settings: ProjectSettings) {
        self.sim.set_settings(settings.gameplay.clone());
        self.audio.apply(&settings.audio);
        self.world.background = world::linear_background(settings.graphics.background);
        self.settings = settings;
    }

    /// When (UI seconds) the hero was last hit, for the camera's shake.
    pub fn hit_at(&self) -> Option<f64> {
        self.hit_at
    }

    /// Whether the game runs on a server: `Some(connected)`, or `None` when local.
    pub fn online(&self) -> Option<bool> {
        match &self.link {
            Link::Local => None,
            Link::Remote(conn) => Some(conn.is_open()),
        }
    }

    /// Asks the game for `input`: the local simulation, or the server.
    fn input(&mut self, input: Input) {
        match &self.link {
            Link::Local => self.sim.input(self.hero, input),
            Link::Remote(conn) => conn.send(input),
        }
    }

    /// Shows `problems` in the host's console.
    pub fn report(&mut self, problems: Vec<String>) {
        self.log.extend(problems.into_iter().map(LogLine::Error));
    }

    /// What the cursor shows over world point `point` (monsters aside: the host asks
    /// [`Self::monster_at`] first).
    pub fn hover(&self, point: Vec2) -> CursorKind {
        if self.world.npc_at(point).is_some() {
            return CursorKind::Talk;
        }
        if self.view.item_at(&self.sim, &self.world, point).is_some() {
            return CursorKind::PickUp;
        }
        let cell = self.world.scene.world_to_cell((point.x, point.y));
        if self.world.scene.portals.iter().any(|p| p.cell == cell) {
            return CursorKind::Portal;
        }
        CursorKind::Normal
    }

    /// Lines for the host's console since the last call.
    pub fn take_log(&mut self) -> Vec<LogLine> {
        let mut log = std::mem::take(&mut self.log);
        log.extend(self.ui.take_log());
        log
    }

    /// Whether the player is typing in chat (game keys should wait).
    pub fn typing(&self) -> bool {
        self.ui.typing()
    }

    pub fn running(&self) -> bool {
        self.sim.player(self.hero).is_some_and(|p| p.running)
    }

    pub fn toggle_running(&mut self) {
        let running = self.running();
        self.input(Input::SetRunning(!running));
    }

    pub fn toggle_bag(&mut self) {
        self.ui.toggle_bag();
    }

    /// Opens or closes the quest log, as L does.
    pub fn toggle_quests(&mut self) {
        self.ui.toggle_quests();
    }

    pub fn toggle_menu(&mut self) {
        self.ui.toggle_menu();
    }

    /// Reads the game's art and sounds again (the project's assets changed on disk).
    pub fn reload_assets(&mut self, renderer: &mut c3_render::Renderer) {
        self.world.reload_assets(renderer);
        self.audio.reload(None);
    }

    /// Opens the menu on its Controls page.
    pub fn open_controls(&mut self) {
        self.ui.open_controls();
    }

    pub fn open_chat(&mut self) {
        self.ui.open_chat();
    }

    /// Whether a game window is open (its buttons take the menu controls).
    pub fn window_open(&self) -> bool {
        self.ui.window_open(&self.sim, self.hero)
    }

    /// Closes the topmost game window; `false` when none was open.
    pub fn back(&mut self) -> bool {
        let (closed, action) = self.ui.back(&self.sim, self.hero);
        if let Some(action) = action {
            self.act(action);
        }
        closed
    }

    /// The living monster the hero has in its sights.
    pub fn target(&self) -> Option<EntityId> {
        self.target
            .filter(|t| self.sim.monster(*t).is_some_and(|m| m.is_alive()))
    }

    /// Puts the next (`step` 1) or previous (-1) monster around the hero in its sights,
    /// nearest first.
    pub fn cycle_target(&mut self, step: i32) {
        let Some(here) = self.sim.cell_of(self.hero) else {
            return;
        };
        let range = self.settings.gameplay.attack_search;
        let mut near: Vec<(i32, EntityId)> = self
            .sim
            .monsters()
            .filter(|m| m.is_alive() && c3_game::distance(m.cell, here) <= range)
            .map(|m| (c3_game::distance(m.cell, here), m.id))
            .collect();
        near.sort();
        if near.is_empty() {
            self.target = None;
            return;
        }
        let at = self
            .target()
            .and_then(|t| near.iter().position(|(_, id)| *id == t));
        let next = match at {
            Some(i) => (i as i32 + step).rem_euclid(near.len() as i32) as usize,
            None => 0,
        };
        self.target = Some(near[next].1);
    }

    /// Acts on what is nearest the hero: the target, else the nearest NPC (talk), drop
    /// (pick up) or monster (attack).
    pub fn interact(&mut self) {
        if let Some(target) = self.target() {
            return self.input(Input::Attack(target));
        }
        let Some(here) = self.sim.cell_of(self.hero) else {
            return;
        };
        let range = self.settings.gameplay.attack_search;
        let d = |cell: CellPos| c3_game::distance(cell, here);
        let npc = (self.sim.npcs().iter())
            .map(|n| (d(n.cell), Input::Talk(n.id)))
            .min_by_key(|(dist, _)| *dist);
        let drop = (self.sim.ground_items())
            .map(|i| (d(i.cell), Input::PickUp(i.id)))
            .min_by_key(|(dist, _)| *dist);
        let monster = (self.sim.monsters())
            .filter(|m| m.is_alive())
            .map(|m| (d(m.cell), Input::Attack(m.id)))
            .min_by_key(|(dist, _)| *dist);
        let nearest = [npc, drop, monster]
            .into_iter()
            .flatten()
            .filter(|(dist, _)| *dist <= range)
            .min_by_key(|(dist, _)| *dist);
        if let Some((_, input)) = nearest {
            self.input(input);
        }
    }

    /// Walks the hero a step towards screen direction `dir` (a stick or joystick lean).
    pub fn step_hero(&mut self, dir: Vec2) {
        let (Some(feet), Some(here)) = (self.hero_position(), self.sim.cell_of(self.hero)) else {
            return;
        };
        let reach = self.world.scene.projection.cell_w * 1.5;
        let to = feet + dir.normalize_or_zero() * reach;
        let cell = self.world.scene.world_to_cell((to.x, to.y));
        if cell != here {
            self.input(Input::Walk(cell));
        }
    }

    /// Says `line` in chat, as typed (`/` commands run).
    pub fn say(&mut self, line: &str) {
        self.act(UiAction::Say(line.to_owned()));
    }

    /// Where the hero's feet are now.
    pub fn hero_position(&self) -> Option<Vec2> {
        self.view.feet(&self.world, self.hero)
    }

    /// Walks the hero up to NPC `id` and talks to it, as if it was clicked.
    pub fn talk_to(&mut self, npc: u32) {
        self.input(Input::Talk(npc));
    }

    /// Sends the hero after the nearest monster it can reach.
    pub fn attack_nearest(&mut self) {
        let search = self.settings.gameplay.attack_search;
        if let Some(target) = self.sim.nearest_monster(self.hero, search) {
            self.input(Input::Attack(target));
        }
    }

    /// The living monster under world point `point`.
    pub fn monster_at(&self, point: Vec2) -> Option<EntityId> {
        self.view.monster_at(&self.sim, &self.world, point)
    }

    /// A click on world point `point`: talk to an NPC, attack a monster, pick up a drop, or
    /// walk there.
    pub fn click(&mut self, point: Vec2) {
        if let Some(npc) = self.world.npc_at(point).map(|n| n.id) {
            self.input(Input::Talk(npc));
        } else if let Some(monster) = self.monster_at(point) {
            self.input(Input::Attack(monster));
        } else if let Some(item) = self.view.item_at(&self.sim, &self.world, point) {
            self.input(Input::PickUp(item));
        } else {
            let cell = self.world.scene.world_to_cell((point.x, point.y));
            self.input(Input::Walk(cell));
            self.world.mark_click(cell);
        }
    }

    /// Runs `seconds` of game and shows what happened (`now`: UI time in seconds).
    pub fn advance(&mut self, seconds: f32, now: f64) {
        match &mut self.link {
            Link::Local => self.sim.advance((seconds * 1000.0).round() as u64),
            Link::Remote(conn) => {
                for event in conn.poll() {
                    match event {
                        RemoteEvent::Update(update) => self.sim.apply_update(update),
                        RemoteEvent::Closed(why) => {
                            let why =
                                why.unwrap_or_else(|| "the server closed the connection".into());
                            self.ui.system(format!("Disconnected: {why}"), style::BAD);
                        }
                        RemoteEvent::Welcome(_) | RemoteEvent::Refused(_) => {}
                    }
                }
            }
        }
        let events = self.sim.take_events();
        let hero = self.hero;
        if events
            .iter()
            .any(|e| matches!(e, Event::Attacked { target, .. } if *target == hero))
        {
            self.hit_at = Some(now);
        }
        self.view.apply(&events, &self.sim, &mut self.world);
        self.ui.events(&events, &self.sim, self.hero, now);
        self.audio.events(&events, &self.sim, self.hero);
        self.audio.update(&self.sim, self.hero, now);
        self.hero_effects(&events);
        self.world.advance(seconds);
        self.world.tick();
    }

    /// Effects on the hero for what happened to it: a potion's glow, a level up.
    fn hero_effects(&mut self, events: &[Event]) {
        for event in events {
            let effect = match *event {
                Event::Used { player, kind } if player == self.hero => {
                    self.world.game.item_use(kind).0
                }
                Event::LeveledUp { player, .. } if player == self.hero => {
                    self.world.game.level_up_effect()
                }
                _ => None,
            };
            // Decoration: a client without the effect still plays.
            if let Some(name) = effect {
                let _ = self.world.hero_effect(&name, false);
            }
        }
    }

    /// Draws the in-game UI over the game view and carries out what the player asks of the
    /// game; returns what is the host's to do (controls, leaving).
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        screen: &Screen,
        hovered: Option<EntityId>,
        (hint, controls): (&str, &ControlsView),
    ) -> Vec<UiAction> {
        let head = |id: EntityId| {
            let feet = self.view.feet(&self.world, id)?;
            Some(screen.to_screen(feet - Vec2::Y * HEAD_HEIGHT))
        };
        let cell = |c: CellPos| screen.to_screen(Vec2::from(self.world.scene.cell_to_world(c)));
        let npc_head = |c: CellPos| {
            screen.to_screen(Vec2::from(self.world.scene.cell_to_world(c)) - Vec2::Y * HEAD_HEIGHT)
        };
        let slots: Vec<(u8, &str)> = self.slots.iter().map(|(s, n)| (*s, n.as_str())).collect();
        let view = GameView {
            sim: &self.sim,
            hero: self.hero,
            rect: screen.rect,
            head: &head,
            cell: &cell,
            npc_head: &npc_head,
            hovered: hovered.or(self.target()),
            slots: &slots,
            sound: self.audio.muted().map(|m| !m),
            hint,
            controls,
        };
        let actions = self.ui.show(ui, &view, &mut self.icons);
        let mut host = Vec::new();
        for action in actions {
            match action {
                UiAction::Rebind(_) | UiAction::ResetControls | UiAction::Leave => {
                    host.push(action)
                }
                action => self.act(action),
            }
        }
        host
    }

    fn act(&mut self, action: UiAction) {
        let input = match action {
            UiAction::Answer(i) => Input::Answer(i),
            UiAction::CloseDialogue => Input::CloseDialogue,
            UiAction::Buy { npc, kind } => Input::Buy { npc, kind },
            UiAction::Sell { npc, item } => Input::Sell { npc, item },
            UiAction::CloseShop => Input::CloseShop,
            UiAction::Use(item) => Input::Use(item),
            UiAction::Unequip(item) => Input::Unequip(item),
            UiAction::Say(text) => Input::Say(text),
            UiAction::ToggleSound => return self.audio.toggle_muted(),
            UiAction::Rebind(_) | UiAction::ResetControls | UiAction::Leave => return,
        };
        self.input(input);
    }
}

/// Item icons as the game draws them, loaded once each.
struct ItemIcons {
    game: Rc<dyn GameModule>,
    cache: HashMap<u32, Option<TextureHandle>>,
}

impl ItemIcons {
    fn new(game: Rc<dyn GameModule>) -> Self {
        Self {
            game,
            cache: HashMap::new(),
        }
    }
}

impl Icons for ItemIcons {
    fn item(&mut self, ctx: &egui::Context, kind: u32) -> Option<TextureId> {
        let game = &self.game;
        self.cache
            .entry(kind)
            .or_insert_with(|| {
                let image = game.item_icon(kind)?;
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    [image.width as usize, image.height as usize],
                    &image.rgba,
                );
                Some(ctx.load_texture(format!("item {kind}"), color, egui::TextureOptions::LINEAR))
            })
            .as_ref()
            .map(TextureHandle::id)
    }
}

/// How long joining a server may take.
fn join_timeout(settings: &ProjectSettings) -> std::time::Duration {
    std::time::Duration::from_secs_f64(settings.network.join_timeout_seconds.max(1.0))
}
