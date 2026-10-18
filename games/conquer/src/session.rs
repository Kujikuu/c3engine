//! A Conquer game being played: the engine's simulation with Conquer's rules and the
//! project's scripts, shown in a [`World`] with the in-game UI. The simulation runs here
//! ([`Session::start`]) or on a game server this session mirrors ([`Session::join`]); either
//! way the view and UI read the same `Sim`. The Studio's Play mode and the client host it;
//! they own the window, camera and renderer and pass input in.

use crate::Content;
use crate::game::{self, GameSetup};
use crate::rules::SLOT_NAMES;
use crate::sim_view::SimView;
use crate::sound::GameAudio;
use crate::world::World;
use c3_game::{EntityId, Event, Input, Sim, WorldUpdate};
use c3_online::{Connection, RemoteEvent, Welcome};
use c3_project::Project;
use c3_render::camera::Camera2d;
use c3_scene::{CellPos, Scene, path};
use c3_ui::{GameUi, GameView, Icons, LogLine, UiAction, style};
use egui::{Pos2, Rect, TextureHandle, TextureId};
use glam::Vec2;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

/// Cells within which "attack nearest" looks for a monster.
const ATTACK_SEARCH: i32 = 10;
/// How far (cells) the hero may start from the asked cell.
const START_SEARCH: i32 = 50;
/// World pixels above an actor's feet where its name and numbers go.
const HEAD_HEIGHT: f32 = 110.0;
/// Effect marking where the player clicked to walk.
const CLICK_EFFECT: &str = "mouse";
/// Effect played on the hero when it rises a level.
const LEVEL_UP: &str = "LevelUp";
/// How long joining a server may take.
const JOIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

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
}

/// Where a session finds the project's game and where the hero starts.
pub struct SessionSetup {
    pub scripts: Option<PathBuf>,
    pub tables: Option<PathBuf>,
    /// Game map ids of the map (script teleports within it work).
    pub map_ids: Vec<u32>,
    pub start: CellPos,
    pub seed: u64,
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
    audio: GameAudio,
    log: Vec<LogLine>,
    link: Link,
}

impl Session {
    /// Starts a game on `scene` (map `map_path` of `content`), simulated here.
    pub fn start(
        content: Rc<Content>,
        map_path: String,
        scene: Scene,
        setup: SessionSetup,
    ) -> Result<Self, String> {
        let start = path::nearest_walkable(&scene.grid, setup.start, START_SEARCH)
            .ok_or("there is no walkable cell near the start")?;
        let game = GameSetup {
            scripts: setup.scripts,
            tables: setup.tables,
            map_ids: setup.map_ids.clone(),
            seed: setup.seed,
        };
        let (mut sim, problems) = game::new_sim(&scene, &game);
        let hero = sim.add_saved_player(&game::new_character("Hero", start));
        Self::open(
            content,
            Opening {
                map_path,
                scene,
                sim,
                hero,
                map_ids: setup.map_ids,
                problems,
                link: Link::Local,
            },
        )
    }

    /// Joins the game server at `addr` as `account` (a new name makes the account), showing
    /// the map of `project` it puts the player on.
    pub fn join(
        content: Rc<Content>,
        project: &Project,
        addr: &str,
        account: &str,
        password: &str,
    ) -> Result<Self, String> {
        let (conn, welcome, updates) = Connection::join(addr, account, password, JOIN_TIMEOUT)?;
        let scene = game::load_map(project, &content, &welcome.map)?;
        Self::joined(
            content,
            scene,
            Some(project.tables_dir()),
            (conn, welcome, updates),
        )
    }

    /// Joins the game server at `addr`, which serves `scene` (the Studio's local server
    /// plays the map as it is being edited); `tables` are the project's.
    pub fn join_scene(
        content: Rc<Content>,
        scene: Scene,
        tables: Option<PathBuf>,
        addr: &str,
        account: &str,
        password: &str,
    ) -> Result<Self, String> {
        let joined = Connection::join(addr, account, password, JOIN_TIMEOUT)?;
        Self::joined(content, scene, tables, joined)
    }

    fn joined(
        content: Rc<Content>,
        scene: Scene,
        tables: Option<PathBuf>,
        (conn, welcome, updates): (Connection, Welcome, Vec<WorldUpdate>),
    ) -> Result<Self, String> {
        let (mut sim, problems) = game::mirror_sim(&scene, tables.as_ref());
        for update in updates {
            sim.apply_update(update);
        }
        Self::open(
            content,
            Opening {
                map_path: welcome.map,
                scene,
                sim,
                hero: welcome.hero,
                map_ids: welcome.map_ids,
                problems,
                link: Link::Remote(conn),
            },
        )
    }

    fn open(content: Rc<Content>, o: Opening) -> Result<Self, String> {
        let Opening {
            map_path,
            scene,
            mut sim,
            hero,
            map_ids,
            problems,
            link,
        } = o;
        let mut log: Vec<LogLine> = problems.into_iter().map(LogLine::Error).collect();
        // What happened while setting up is shown by attaching the view; keep the warnings.
        for event in sim.take_events() {
            if let Event::Warning(text) = event {
                log.push(LogLine::Error(text));
            }
        }
        let audio = GameAudio::new(content.clone(), map_ids);
        let mut world = World::new(content.clone(), map_path, scene);
        let view = SimView::attach(&sim, hero, &mut world)?;
        let mut ui = GameUi::default();
        ui.system(
            "Welcome! Click to walk, a monster to fight, an NPC to talk. B: bag · L: quests · Enter: chat · /help",
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
            icons: ItemIcons::new(content),
            audio,
            log,
            link,
        })
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
        if let Some(target) = self.sim.nearest_monster(self.hero, ATTACK_SEARCH) {
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
            if self
                .world
                .scene
                .grid
                .get(cell)
                .is_some_and(|c| !c.is_blocked())
            {
                // The marker is decoration: a client without it still walks.
                let _ = self.world.place_effect(CLICK_EFFECT, cell, false);
            }
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
                    self.world.content.item_use_media(kind).0.map(str::to_owned)
                }
                Event::LeveledUp { player, .. } if player == self.hero => Some(LEVEL_UP.to_owned()),
                _ => None,
            };
            // Decoration: a client without the effect still plays.
            if let Some(name) = effect {
                let _ = self.world.hero_effect(&name, false);
            }
        }
    }

    /// Draws the in-game UI over the game view and carries out what the player asks.
    pub fn ui(&mut self, ui: &mut egui::Ui, screen: &Screen, hovered: Option<EntityId>) {
        let head = |id: EntityId| {
            let feet = self.view.feet(&self.world, id)?;
            Some(screen.to_screen(feet - Vec2::Y * HEAD_HEIGHT))
        };
        let cell = |c: CellPos| screen.to_screen(Vec2::from(self.world.scene.cell_to_world(c)));
        let npc_head = |c: CellPos| {
            screen.to_screen(Vec2::from(self.world.scene.cell_to_world(c)) - Vec2::Y * HEAD_HEIGHT)
        };
        let view = GameView {
            sim: &self.sim,
            hero: self.hero,
            rect: screen.rect,
            head: &head,
            cell: &cell,
            npc_head: &npc_head,
            hovered,
            slots: SLOT_NAMES,
            sound: self.audio.muted().map(|m| !m),
        };
        let actions = self.ui.show(ui, &view, &mut self.icons);
        for action in actions {
            self.act(action);
        }
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
        };
        self.input(input);
    }
}

/// Item icons from the client's `data/ItemMinIcon`, loaded once each.
struct ItemIcons {
    content: Rc<Content>,
    cache: HashMap<u32, Option<TextureHandle>>,
}

impl ItemIcons {
    fn new(content: Rc<Content>) -> Self {
        Self {
            content,
            cache: HashMap::new(),
        }
    }
}

impl Icons for ItemIcons {
    fn item(&mut self, ctx: &egui::Context, kind: u32) -> Option<TextureId> {
        let content = &self.content;
        self.cache
            .entry(kind)
            .or_insert_with(|| {
                // Icons are drawn for one quality digit (5 for weapons, 0 for most).
                let image = [kind, kind / 10 * 10 + 5, kind / 10 * 10]
                    .iter()
                    .find_map(|k| content.load_image(&format!("data/ItemMinIcon/{k}.dds")))?;
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
