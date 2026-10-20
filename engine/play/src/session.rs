//! A game being played: the engine's simulation with the game's rules and the project's
//! scripts, shown in a [`World`] with the in-game UI. The simulation runs here
//! ([`Session::start`]) or on a game server this session mirrors ([`Session::join`]); either
//! way the view and UI read the same `Sim`. The Studio's Play mode and the client host it;
//! they own the window, camera and renderer and pass input in.

use crate::audio::GameAudio;
use crate::cursor::CursorKind;
use crate::game::{self, GameSetup};
use crate::map_list::{MapInfo, MapList};
use crate::module::GameModule;
use crate::settings;
use crate::sim_view::SimView;
use crate::ui_textures::GameTextures;
use crate::world::{self, World};
use c3_game::{CastAt, EntityId, Event, Input, Sim, SkillTarget, WorldUpdate};
use c3_online::{Connection, RemoteEvent, Welcome};
use c3_project::{Project, ProjectSettings};
use c3_render::camera::Camera2d;
use c3_scene::{CellPos, Scene, path};
use c3_ui::{ControlsView, GameUi, GameView, LogLine, UiAction, style};
use egui::{Pos2, Rect};
use glam::Vec2;
use std::path::PathBuf;
use std::rc::Rc;

/// How far (cells) the hero may start from the asked cell.
const START_SEARCH: i32 = 50;
/// World pixels above an actor's feet where its name and numbers go.
const HEAD_HEIGHT: f32 = 110.0;
/// How much higher names sit mid-jump (TQ's `CPlayer::ShowLife`: 130% of the zoom, not 90%).
const JUMP_HEAD_RAISE: f32 = 40.0;

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
    setup: GameSetup,
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
    /// Blueprint nodes a debugger stops after (from the first event on).
    pub breakpoints: std::collections::BTreeSet<c3_script::debug::NodeKey>,
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

/// The hero is to go to another map.
#[derive(Debug, Clone)]
pub enum MapChange {
    /// Through a portal or a teleport here: map `map` (an id), at `cell`.
    Local { map: u32, cell: CellPos },
    /// The server moved the hero to another map; its first updates wait for it.
    Remote(Welcome, Vec<WorldUpdate>),
}

pub struct Session {
    pub sim: Sim,
    pub hero: EntityId,
    pub world: World,
    view: SimView,
    ui: GameUi,
    icons: GameTextures,
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
    /// Where the pointer is over the world (skills cast where it points).
    pointer: Option<Vec2>,
    /// How the simulation was set up (the next map's is set up alike; online, its tables).
    setup: GameSetup,
    /// A map change waiting for the host to load the map.
    map_change: Option<MapChange>,
    /// The map drawn small, under the radar.
    minimap: Minimap,
    /// What falls from the sky where the hero stands.
    weather: crate::weather::WeatherOverlay,
    /// The game's auto-path destinations.
    destinations: Vec<c3_ui::Destination>,
    /// A statue of the hero being posed (the host draws its picture).
    pub effigy: Option<crate::effigy::Effigy>,
    /// The player whose equipment is shown, as a figure (`viewed.model`, CDlgNpcEquip).
    viewed_model: Option<(EntityId, crate::preview::UiModel)>,
    /// The hero as a figure while a window shows it (`hero.model`, CDlgEquip).
    hero_model: Option<crate::preview::UiModel>,
}

/// A map's minimap: the image until the UI uploads it, then its texture.
#[derive(Default)]
struct Minimap {
    image: Option<c3_assets::Image>,
    texture: Option<egui::TextureHandle>,
    /// The game's mark pictures (each kind's frames), then their textures.
    mark_images: Vec<(&'static str, Vec<c3_assets::Image>)>,
    marks: c3_ui::RadarMarks,
    /// The picture is shown as it is (CRadar::ShowFallback).
    fallback: bool,
    /// Kept so the marks' textures stay alive.
    mark_textures: Vec<egui::TextureHandle>,
}

impl Minimap {
    fn of(game: &dyn GameModule, key: &str, map_ids: &[u32]) -> Self {
        let mark_images = ["hero", "player", "leader", "tutor", "student"]
            .into_iter()
            .map(|kind| (kind, game.radar_marks(kind)))
            .collect();
        Self {
            image: game.minimap(key, map_ids),
            texture: None,
            mark_images,
            marks: c3_ui::RadarMarks::default(),
            fallback: game.radar_fallback(key, map_ids),
            mark_textures: Vec::new(),
        }
    }

    /// The texture and its size, uploaded on first use.
    fn texture(&mut self, ctx: &egui::Context) -> Option<(egui::TextureId, egui::Vec2)> {
        if let Some(image) = self.image.take() {
            let size = [image.width as usize, image.height as usize];
            let pixels = egui::ColorImage::from_rgba_unmultiplied(size, &image.rgba);
            self.texture = Some(ctx.load_texture("minimap", pixels, Default::default()));
        }
        for (kind, images) in std::mem::take(&mut self.mark_images) {
            let frames = images.iter().map(|image| {
                let size = [image.width as usize, image.height as usize];
                let pixels = egui::ColorImage::from_rgba_unmultiplied(size, &image.rgba);
                let texture = ctx.load_texture(kind, pixels, Default::default());
                let mark = (texture.id(), egui::vec2(size[0] as f32, size[1] as f32));
                self.mark_textures.push(texture);
                mark
            });
            let frames: Vec<c3_ui::RadarMark> = frames.collect();
            match kind {
                "hero" => self.marks.hero = frames,
                "player" => self.marks.player = frames,
                "leader" => self.marks.leader = frames,
                "tutor" => self.marks.tutor = frames,
                _ => self.marks.student = frames,
            }
        }
        (self.texture.as_ref()).map(|t| (t.id(), t.size_vec2()))
    }
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
            pk: settings::pk_map(&setup.settings, &setup.map_ids),
            pk_free: settings::pk_free_map(&setup.settings, &setup.map_ids),
            revive: settings::revive_point(&setup.settings, &setup.map_ids),
            breakpoints: setup.breakpoints.clone(),
        };
        let (mut sim, problems) = game::new_sim(&scene, &game_setup, &*game.rules());
        let local_setup = game_setup.clone();
        let hero = sim.add_saved_player(&settings::new_character(
            &setup.settings.gameplay,
            "Hero",
            start,
        ));
        sim.logged_in(hero);
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
                setup: local_setup,
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

    /// Plays on a server that has let the player in (the front end logged in on `conn`):
    /// the map of `project` it put the hero on.
    pub fn entered(
        game: Rc<dyn GameModule>,
        project: &Project,
        settings: ProjectSettings,
        joined: (Connection, Welcome, Vec<WorldUpdate>),
    ) -> Result<Self, String> {
        let scene = game.load_map(project, &joined.1.map)?;
        Self::joined(game, scene, Some(project.tables_dir()), settings, joined)
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
                setup: GameSetup {
                    tables,
                    settings: settings.gameplay.clone(),
                    ..GameSetup::default()
                },
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
            setup,
        } = o;
        let mut log: Vec<LogLine> = problems.into_iter().map(LogLine::Error).collect();
        // What happened while setting up is shown by attaching the view; keep the warnings,
        // and what scripts said (the events script's `on_login`) for the chat.
        let mut said = Vec::new();
        for event in sim.take_events() {
            match event {
                Event::Warning(text) => log.push(LogLine::Error(text)),
                e @ (Event::Notice { .. } | Event::Message { .. } | Event::ScriptLog(_)) => {
                    said.push(e)
                }
                _ => {}
            }
        }
        let light = settings::map_light(&settings, &map_ids);
        let minimap = Minimap::of(&*game, &map_path, &map_ids);
        let mut audio = GameAudio::new(Rc::clone(&game), map_ids);
        audio.apply(&settings.audio);
        audio.set_ui_sounds(&settings.ui);
        let slots = settings::equipment_slots(&settings, &*game);
        let tilt = settings::tilt_deg(&settings, &*game);
        let mut world = World::new(Rc::clone(&game), map_path, scene, tilt);
        world.background = world::linear_background(settings.graphics.background);
        world.light = light;
        let view = SimView::attach(&sim, hero, &mut world)?;
        let mut ui = GameUi::default();
        ui.set_gui(game_gui(&*game, &settings.ui));
        // With the game's own UI, its chat log looks as the game's does.
        if settings.ui.from_data() {
            ui.set_chat_look(game.chat_look());
        }
        ui.system(
            "Welcome! Walk around, fight monsters and talk to NPCs: the line at the top names your controls. /help lists the commands.",
            style::GOLD,
        );
        if matches!(link, Link::Remote(_)) {
            ui.system("Connected to the server.", style::GOOD);
        }
        ui.events(&said, &sim, hero, 0.0);
        let destinations = game.destinations();
        Ok(Self {
            sim,
            hero,
            world,
            view,
            ui,
            icons: GameTextures::new(game),
            slots,
            audio,
            log,
            link,
            settings,
            hit_at: None,
            pointer: None,
            target: None,
            setup,
            map_change: None,
            minimap,
            weather: Default::default(),
            destinations,
            effigy: None,
            viewed_model: None,
            hero_model: None,
        })
    }

    /// Takes the hero to the map of the waiting map change, loaded from `project` (the map
    /// ids resolved by `maps`); old textures are freed from `renderer`. Returns the map's
    /// name, or why it couldn't go; `None` when no change waits.
    pub fn follow_map_change(
        &mut self,
        project: &Project,
        maps: &MapList,
        renderer: &mut c3_render::Renderer,
    ) -> Option<Result<String, String>> {
        // The new map's first update (the hero, and what is around) comes after the welcome.
        if let Some(MapChange::Remote(_, updates)) = &self.map_change
            && updates.is_empty()
        {
            return None;
        }
        let change = self.map_change.take()?;
        let game = Rc::clone(&self.world.game);
        Some(match change {
            MapChange::Local { map, cell } => maps
                .by_id(map)
                .cloned()
                .ok_or_else(|| format!("There is no map {map}."))
                .and_then(|info| {
                    let scene = game.load_map(project, &info.key)?;
                    self.enter_map_here(&info, scene, cell, renderer)?;
                    Ok(info.name)
                }),
            MapChange::Remote(welcome, updates) => {
                let info = maps.by_key(&welcome.map);
                let name = info.map_or_else(|| welcome.map.clone(), |m| m.name.clone());
                let music = info.and_then(|m| non_empty(&m.music));
                game.load_map(project, &welcome.map).and_then(|scene| {
                    self.enter_map_online(welcome, updates, scene, music, renderer)?;
                    Ok(name)
                })
            }
        })
        .map(|moved| match moved {
            Ok(name) => {
                self.ui
                    .system(format!("You arrive in {name}."), style::GOLD);
                Ok(name)
            }
            Err(e) => {
                self.ui.system(&e, style::BAD);
                Err(e)
            }
        })
    }

    /// The hero (as it is) on `cell` of `scene`, a new simulation of map `info`.
    fn enter_map_here(
        &mut self,
        info: &MapInfo,
        scene: Scene,
        cell: CellPos,
        renderer: &mut c3_render::Renderer,
    ) -> Result<(), String> {
        let mut setup = self.setup.clone();
        setup.map_ids = info.ids.clone();
        setup.pk = info.pk;
        setup.pk_free = info.pk_free;
        setup.revive = info.revive;
        let mut save = self.sim.save_player(self.hero).ok_or("the hero is gone")?;
        save.map = info.key.clone();
        save.cell = path::nearest_walkable(&scene.grid, cell, START_SEARCH).ok_or_else(|| {
            format!(
                "{} has no walkable cell near {}, {}",
                info.name, cell.0, cell.1
            )
        })?;
        let (mut sim, problems) = game::new_sim(&scene, &setup, &*self.world.game.rules());
        self.report(problems);
        let hero = sim.add_saved_player(&save);
        sim.take_events();
        self.setup = setup;
        self.replace_world(sim, hero, info.key.clone(), &info.ids, scene, renderer)?;
        self.audio.set_map(info.ids.clone(), non_empty(&info.music));
        Ok(())
    }

    /// The server's map `welcome` names: a mirror of it, filled by the updates to come.
    fn enter_map_online(
        &mut self,
        welcome: Welcome,
        updates: Vec<WorldUpdate>,
        scene: Scene,
        music: Option<String>,
        renderer: &mut c3_render::Renderer,
    ) -> Result<(), String> {
        let (mut sim, problems) = game::mirror_sim(
            &scene,
            self.setup.tables.as_ref(),
            &*self.world.game.rules(),
            &self.settings.gameplay,
        );
        self.report(problems);
        for update in updates {
            sim.apply_update(update);
        }
        let ids = welcome.map_ids.clone();
        self.replace_world(
            sim,
            welcome.hero,
            welcome.map.clone(),
            &ids,
            scene,
            renderer,
        )?;
        self.audio.set_map(welcome.map_ids, music);
        Ok(())
    }

    /// Plays `sim` (hero `hero`) on `scene` from now on, the old world's textures freed.
    fn replace_world(
        &mut self,
        sim: Sim,
        hero: EntityId,
        map_path: String,
        map_ids: &[u32],
        scene: Scene,
        renderer: &mut c3_render::Renderer,
    ) -> Result<(), String> {
        let tilt = settings::tilt_deg(&self.settings, &*self.world.game);
        let mut world = World::new(Rc::clone(&self.world.game), map_path, scene, tilt);
        world.background = self.world.background;
        world.light = settings::map_light(&self.settings, map_ids);
        self.minimap = Minimap::of(&*self.world.game, &world.map_path, map_ids);
        let view = SimView::attach(&sim, hero, &mut world)?;
        self.world.release(renderer);
        self.world = world;
        self.view = view;
        self.sim = sim;
        self.hero = hero;
        self.target = None;
        Ok(())
    }

    pub fn settings(&self) -> &ProjectSettings {
        &self.settings
    }

    /// Plays at the player's volumes from now on.
    pub fn set_player_prefs(&mut self, prefs: c3_core::settings::PlayerPrefs) {
        self.audio.set_player(prefs);
    }

    /// Plays by `settings` from now on: the simulation's numbers, sound and look. (The tilt
    /// and the start apply to the next session.)
    pub fn set_settings(&mut self, settings: ProjectSettings) {
        self.sim.set_settings(settings.gameplay.clone());
        self.audio.apply(&settings.audio);
        self.world.background = world::linear_background(settings.graphics.background);
        self.world.light = settings::map_light(&settings, &self.setup.map_ids);
        self.slots = settings::equipment_slots(&settings, &*self.world.game);
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
        // Enter answers Yes to a question shown.
        if let Some(actions) = self.ui.answer_prompt() {
            for action in actions {
                self.act(action);
            }
            return;
        }
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

    /// Opens dialog `id` of the game's UI from data.
    pub fn open_dialog(&mut self, id: u32) {
        self.ui.open_dialog(id);
    }

    /// Runs a game UI action as a click would (a question asked: `ask:guild_leave:...`).
    pub fn gui_action(&mut self, action: &str) {
        // `{hero}`: the hero's id (a scripted look at its own equipment, `view:viewed:{hero}`);
        // `{cell}`: where it stands (`place_at:{cell}`).
        let (x, y) = self.sim.player(self.hero).map_or((0, 0), |p| p.cell);
        let action = (action.replace("{hero}", &self.hero.0.to_string()))
            .replace("{cell}", &format!("{x},{y}"));
        // Talking to NPC `id` as a click on it would (`talk:9999`).
        if let Some(npc) = action.strip_prefix("talk:").and_then(|n| n.parse().ok()) {
            return self.input(Input::Talk(npc));
        }
        // A jump from where the hero stands (`jump_by:6,-4`).
        if let Some((dx, dy)) = (action.strip_prefix("jump_by:"))
            .and_then(|d| d.split_once(','))
            .and_then(|(dx, dy)| Some((dx.parse::<i32>().ok()?, dy.parse::<i32>().ok()?)))
        {
            return self.jump_to((x + dx, y + dy));
        }
        for action in self.ui.run_gui_action(&action) {
            self.act(action);
        }
    }

    /// Says `line` in chat, as typed (`/` commands run).
    pub fn say(&mut self, line: &str) {
        self.act(UiAction::Say(line.to_owned()));
    }

    /// Where the hero's feet are now.
    pub fn hero_position(&self) -> Option<Vec2> {
        self.view.ground(&self.world, self.hero)
    }

    /// Walks the hero up to NPC `id` and talks to it, as if it was clicked.
    pub fn talk_to(&mut self, npc: u32) {
        self.input(Input::Talk(npc));
    }

    /// Walks the hero to `cell`, as if it was clicked (through the portal there, if any).
    pub fn walk_to(&mut self, cell: CellPos) {
        self.input(Input::Walk(cell));
    }

    /// Where the pointer is over the world, for skills cast where it points.
    pub fn point_at(&mut self, point: Option<Vec2>) {
        self.pointer = point;
    }

    /// Casts the skill in slot `slot` of the hero's skill bar.
    pub fn cast_slot(&mut self, slot: usize) {
        let skill = (self.sim.player(self.hero)).and_then(|p| p.skills.get(slot).copied());
        if let Some(skill) = skill {
            self.cast_skill(skill);
        }
    }

    /// Casts the skill picked in the game UI (its skill box), if one is.
    pub fn cast_picked(&mut self) {
        if let Some(skill) = self.ui.picked_skill() {
            self.cast_skill(skill);
        }
    }

    /// Casts `skill`: an attack at the target, the monster pointed at or the nearest one; a
    /// blast where the pointer is (else on the foe, else around the hero); a heal on the hero.
    pub fn cast_skill(&mut self, skill: u32) {
        let Some(t) = self.sim.rules().skill(skill).cloned() else {
            return;
        };
        let pointed = self.pointer.and_then(|p| self.monster_at(p));
        let search = self.settings.gameplay.attack_search;
        let foe = (self.target())
            .or(pointed)
            .or_else(|| self.sim.nearest_monster(self.hero, search));
        let at = match t.target {
            SkillTarget::Myself => Some(CastAt::Me),
            SkillTarget::Enemy => foe.map(CastAt::Entity),
            SkillTarget::Ground => (self.pointer)
                .map(|p| CastAt::Cell(self.world.scene.world_to_cell((p.x, p.y))))
                .or(foe.map(CastAt::Entity))
                .or(Some(CastAt::Me)),
        };
        let Some(at) = at else {
            return self
                .ui
                .system(format!("{} needs a target.", t.name), style::BAD);
        };
        if let CastAt::Entity(e) = at {
            self.target = Some(e);
        }
        self.input(Input::Cast { skill, at });
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
        // Something being placed is put on the cell clicked.
        let cell = self.world.scene.world_to_cell((point.x, point.y));
        if self.ui.place_at(cell) {
            return;
        }
        // A player clicked while the UI waits for one (trade, a team invite).
        if let Some(player) = self.view.player_at(&self.sim, &self.world, point)
            && let Some(action) = self.ui.take_pick(player)
        {
            return self.act(action);
        }
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

    /// The figures the game UI's model controls show, for the host to draw each frame.
    pub fn ui_models_mut(&mut self) -> Vec<&mut crate::preview::UiModel> {
        (self.effigy.as_mut().map(|e| &mut e.model))
            .into_iter()
            .chain(self.viewed_model.as_mut().map(|(_, m)| m))
            .chain(self.hero_model.as_mut())
            .collect()
    }

    /// A figure of player `id` for model control `key`.
    fn player_model(&self, id: EntityId, key: &'static str) -> Option<crate::preview::UiModel> {
        let p = self.sim.player(id)?;
        let game = Rc::clone(&self.world.game);
        let look = game.looks().player(p);
        let tilt = crate::settings::tilt_deg(&self.settings, &*game);
        let preview = crate::preview::LookPreview::new(game, &look, tilt);
        Some(crate::preview::UiModel::new(key, preview))
    }

    /// Keeps the shown player's figure for whom the UI shows (made when it is first shown).
    fn follow_viewed(&mut self) {
        let viewed = self.ui.viewed();
        if self.viewed_model.as_ref().map(|(id, _)| *id) == viewed {
            return;
        }
        self.viewed_model =
            viewed.and_then(|id| Some((id, self.player_model(id, "viewed.model")?)));
    }

    /// Keeps the hero's figure while a window shows it.
    fn follow_hero_model(&mut self) {
        match self.ui.shows_model("hero.model") {
            false => self.hero_model = None,
            true if self.hero_model.is_none() => {
                self.hero_model = self.player_model(self.hero, "hero.model");
            }
            true => {}
        }
    }

    /// What the statue window asks: a pose (`Pose(0)` opens it standing), play or pause,
    /// turn, put the statue up (CHero::CreateStatuty), close.
    pub fn effigy_action(&mut self, action: crate::effigy::EffigyAction) {
        use crate::effigy::{Effigy, EffigyAction as A, StatueRefused};
        let game = Rc::clone(&self.world.game);
        if self.effigy.is_none() {
            let Some(p) = self.sim.player(self.hero) else {
                return;
            };
            let look = game.looks().player(p);
            let tilt = crate::settings::tilt_deg(&self.settings, &*game);
            let preview = crate::preview::LookPreview::new(Rc::clone(&game), &look, tilt);
            self.effigy = Some(Effigy::new(preview));
        }
        let Some(effigy) = self.effigy.as_mut() else {
            return;
        };
        match action {
            A::Pose(0) => {}
            A::Pose(act) => {
                effigy.model.preview.play(act);
                effigy.playing = true;
            }
            A::PlayPause => {
                effigy.playing = !effigy.playing;
                effigy.model.preview.set_paused(!effigy.playing);
            }
            A::Turn(steps) => effigy.model.preview.turn(steps),
            A::Close => self.effigy = None,
            A::Place => {
                let (direction, action, frame) = effigy.model.preview.pose();
                let Some(p) = self.sim.player(self.hero) else {
                    return;
                };
                let statues = &self.settings.gameplay.statues;
                let map = self.sim.map_ids().first().copied().unwrap_or(0);
                let rank = (self.sim.world_state().guild_of(&p.name))
                    .and_then(|(_, rank)| u32::try_from(rank).ok());
                let taken = |c: CellPos| (self.sim.npcs().iter()).any(|n| n.cell == c);
                match crate::effigy::statue_slot(statues, map, rank, p.cell, taken) {
                    Ok(cell) => {
                        let place = Input::Place {
                            cell,
                            direction,
                            pose: Some((action, frame)),
                        };
                        self.effigy = None;
                        self.input(place);
                    }
                    Err(StatueRefused::Full) => {}
                    Err(why) => {
                        let line = match why {
                            StatueRefused::Rank => {
                                "Only the Guild Leader, Deputy Guild Leader and Branch Manager can place the statue."
                            }
                            _ => "Please place the statue in the guild map.",
                        };
                        self.ui.system(line, style::BAD);
                    }
                }
            }
        }
    }

    /// Jumps the hero to the cell under the pointer, checked first as the sim checks it
    /// (`CHero::Jump`): too far, too high on the way, or nowhere to stand is told.
    pub fn jump_to_pointer(&mut self) {
        let Some(point) = self.pointer else { return };
        self.jump_to(self.world.scene.world_to_cell((point.x, point.y)));
    }

    /// Jumps the hero to `cell`, the client's checks first ([`Self::jump_to_pointer`]).
    pub fn jump_to(&mut self, cell: CellPos) {
        if self.sim.player(self.hero).is_none_or(|p| p.cell == cell) {
            return;
        }
        let refused = match self.sim.can_jump(self.hero, cell) {
            Ok(()) => return self.input(Input::Jump(cell)),
            Err(c3_game::JumpRefused::TooFar) => "You can't jump that far!",
            Err(c3_game::JumpRefused::TooHigh) => "You can't jump that high!",
            Err(c3_game::JumpRefused::NoFooting) => "You can't stop here!",
            Err(c3_game::JumpRefused::Transformed) => return,
        };
        self.ui.system(refused, style::BAD);
    }

    /// Runs `seconds` of game and shows what happened (`now`: UI time in seconds).
    /// The light and weather of the place the hero stands (its regions', else the map's).
    fn place_presentation(&mut self) {
        let Some(cell) = self.sim.player(self.hero).map(|p| p.cell) else {
            return;
        };
        let light = self.sim.regions_at(cell).find_map(|r| r.light);
        self.world.light = settings::place_light(&self.settings, &self.setup.map_ids, light);
        self.weather
            .set(self.sim.regions_at(cell).find_map(|r| r.weather));
    }

    pub fn advance(&mut self, seconds: f32, now: f64) {
        match &mut self.link {
            Link::Local => self.sim.advance((seconds * 1000.0).round() as u64),
            Link::Remote(conn) => {
                for event in conn.poll() {
                    match event {
                        RemoteEvent::Update(update) => match &mut self.map_change {
                            // The new map's updates wait for it to load.
                            Some(MapChange::Remote(_, waiting)) => waiting.push(update),
                            _ => self.sim.apply_update(update),
                        },
                        RemoteEvent::Closed(why) => {
                            let why =
                                why.unwrap_or_else(|| "the server closed the connection".into());
                            self.ui.system(format!("Disconnected: {why}"), style::BAD);
                        }
                        // Let in again: the server moved the hero to another map.
                        RemoteEvent::Welcome(welcome) => {
                            self.map_change = Some(MapChange::Remote(welcome, Vec::new()));
                        }
                        RemoteEvent::Refused(_) | RemoteEvent::Characters(_) => {}
                    }
                }
            }
        }
        let events = self.sim.take_events();
        let hero = self.hero;
        for event in &events {
            if let Event::Travel { player, map, cell } = *event
                && player == hero
            {
                self.map_change = Some(MapChange::Local { map, cell });
            }
        }
        if events
            .iter()
            .any(|e| matches!(e, Event::Attacked { target, .. } if *target == hero))
        {
            self.hit_at = Some(now);
        }
        self.view.apply(&events, &self.sim, &mut self.world);
        // What is being placed, where it was put or under the pointer.
        let pointed = (self.pointer).map(|p| self.world.scene.world_to_cell((p.x, p.y)));
        let shown =
            (self.ui.placement()).and_then(|p| Some((p.look, p.cell.or(pointed)?, p.direction)));
        self.view.preview(&mut self.world, shown);
        // A statue asked of the hero: its figure, posed in the game's statue window.
        let statue = self.settings.gameplay.statues.kind;
        let asked = events.iter().any(|e| {
            matches!(e, Event::PlacementAsked { player, kind, .. }
                if *player == self.hero && statue != 0 && *kind == statue)
        });
        if asked {
            self.effigy_action(crate::effigy::EffigyAction::Pose(0));
        }
        // A figure is made again once its player changes clothes.
        for e in events.iter() {
            if let Event::Dressed { player } = *e {
                if player == self.hero {
                    self.hero_model = None;
                }
                if self
                    .viewed_model
                    .as_ref()
                    .is_some_and(|(id, _)| *id == player)
                {
                    self.viewed_model = None;
                }
            }
        }
        self.ui.events(&events, &self.sim, self.hero, now);
        self.audio.events(&events, &self.sim, self.hero);
        self.audio.update(&self.sim, self.hero, now);
        self.place_presentation();
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
        self.follow_viewed();
        self.follow_hero_model();
        let models: Vec<(&str, egui::TextureId)> = (self.effigy.as_ref().map(|e| &e.model))
            .into_iter()
            .chain(self.viewed_model.as_ref().map(|(_, m)| m))
            .chain(self.hero_model.as_ref())
            .filter_map(|m| Some((m.key, m.texture?)))
            .collect();
        let head = |id: EntityId| {
            let feet = self.view.feet(&self.world, id)?;
            let raise = if self.view.jumping(&self.world, id) {
                JUMP_HEAD_RAISE
            } else {
                0.0
            };
            Some(screen.to_screen(feet - Vec2::Y * (HEAD_HEIGHT + raise)))
        };
        let cell = |c: CellPos| screen.to_screen(Vec2::from(self.world.scene.cell_to_world(c)));
        let npc_head = |c: CellPos| {
            screen.to_screen(Vec2::from(self.world.scene.cell_to_world(c)) - Vec2::Y * HEAD_HEIGHT)
        };
        let slots: Vec<(u8, &str)> = self.slots.iter().map(|(s, n)| (*s, n.as_str())).collect();
        let minimap = self
            .minimap
            .texture(ui.ctx())
            .map(|(texture, size)| c3_ui::RadarMap {
                texture,
                size,
                boxed: (self.settings.ui.radar_box).map(|[w, h]| egui::vec2(w as f32, h as f32)),
                bounds: c3_render::minimap::scene_bounds(&self.world.scene),
                projection: self.world.scene.projection,
                marks: self.minimap.marks.clone(),
                fallback: self.minimap.fallback,
            });
        let power_of =
            |id| (self.sim.player(id)).and_then(|p| self.world.game.battle_power(&self.sim, p));
        let view = GameView {
            destinations: &self.destinations,
            effigy: (self.effigy.as_ref()).map(|e| c3_ui::EffigyView {
                texture: e.model.texture,
                playing: e.playing,
            }),
            models: &models,
            power: power_of(self.hero),
            viewed_power: self.ui.viewed().and_then(power_of),
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
            minimap,
        };
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        self.weather
            .show(&ui.painter_at(screen.rect), screen.rect, dt);
        let actions = self.ui.show(ui, &view, &mut self.icons);
        let mut host = Vec::new();
        for action in actions {
            match action {
                UiAction::Rebind(_)
                | UiAction::ResetControls
                | UiAction::Leave
                | UiAction::EdgeLook(_)
                | UiAction::SetPrefs(_) => host.push(action),
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
            UiAction::Game(input) => input,
            UiAction::PlaySound(path) => return self.audio.play_ui(&path),
            UiAction::Use(item) => Input::Use(item),
            UiAction::Unequip(item) => Input::Unequip(item),
            UiAction::Cast(slot) => {
                // Clicked on the skill bar: the pointer is on the bar, not the world.
                self.pointer = None;
                return self.cast_slot(slot);
            }
            UiAction::Say(text) => Input::Say(text),
            UiAction::ToggleSound => return self.audio.toggle_muted(),
            UiAction::ToggleRun => return self.toggle_running(),
            UiAction::Effigy(a) => return self.effigy_action(a),
            UiAction::Rebind(_)
            | UiAction::ResetControls
            | UiAction::Leave
            | UiAction::EdgeLook(_)
            | UiAction::SetPrefs(_) => return,
        };
        self.input(input);
    }
}

/// The game's UI from data, when the project names its files and the game has them.
pub fn game_gui(
    game: &dyn GameModule,
    ui: &c3_core::settings::UiSettings,
) -> Option<c3_ui::gui::Gui> {
    if !ui.from_data() {
        return None;
    }
    // The skins one after another: the first section of a name wins; `path#Prefix` puts
    // `Prefix` before each of that file's section names (two files naming theirs alike).
    let mut skin = Vec::new();
    for entry in std::iter::once(&ui.skin).chain(&ui.skins) {
        let (path, prefix) = entry.split_once('#').unwrap_or((entry.as_str(), ""));
        if let Some(bytes) = (!path.is_empty()).then(|| game.read_file(path)).flatten() {
            skin.extend_from_slice(&prefixed_sections(&bytes, prefix));
            skin.extend_from_slice(b"\n");
        }
    }
    let mut gui = c3_ui::gui::Gui::new(
        &game.read_file(&ui.layout)?,
        &skin,
        &game.read_file(&ui.controls)?,
    );
    gui.load_names(|path| game.read_file(path));
    gui.sounds = c3_ui::gui::GuiSounds {
        open: ui.sound_open.clone(),
        close: ui.sound_close.clone(),
        click: ui.sound_click.clone(),
    };
    Some(gui)
}

/// Ani text with `prefix` put before each section name (`[Face1]` → `[PlayerFace1]`).
fn prefixed_sections(bytes: &[u8], prefix: &str) -> Vec<u8> {
    if prefix.is_empty() {
        return bytes.to_vec();
    }
    let mut out = Vec::with_capacity(bytes.len());
    for line in bytes.split_inclusive(|&b| b == b'\n') {
        match line.strip_prefix(b"[") {
            Some(rest) => {
                out.push(b'[');
                out.extend_from_slice(prefix.as_bytes());
                out.extend_from_slice(rest);
            }
            None => out.extend_from_slice(line),
        }
    }
    out
}

/// How long joining a server may take.
fn join_timeout(settings: &ProjectSettings) -> std::time::Duration {
    std::time::Duration::from_secs_f64(settings.network.join_timeout_seconds.max(1.0))
}

/// `text` when it isn't empty.
fn non_empty(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_owned())
}
