//! A Conquer game played on this machine: the engine's simulation with Conquer's rules and
//! the project's scripts, shown in a [`World`] with the in-game UI. The Studio's Play mode
//! and the client host it; they own the window, camera and renderer and pass input in.
//! (In M6 the client's session talks to a server instead of running the simulation.)

use crate::Content;
use crate::character::DEFAULT_HERO;
use crate::rules::{ConquerRules, SLOT_NAMES};
use crate::sim_view::SimView;
use crate::sound::GameAudio;
use crate::world::World;
use c3_game::{EntityId, Event, Rules, Sim};
use c3_render::camera::Camera2d;
use c3_scene::{CellPos, Scene, path};
use c3_script::{Quest, ScriptHost, quest};
use c3_ui::{GameUi, GameView, Icons, LogLine, UiAction, style};
use egui::{Pos2, Rect, TextureHandle, TextureId};
use glam::Vec2;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

/// Money the hero starts with, to try shops.
const START_MONEY: u64 = 1000;
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
}

impl Session {
    /// Starts a game on `scene` (map `map_path` of `content`).
    pub fn start(
        content: Rc<Content>,
        map_path: String,
        scene: Scene,
        setup: SessionSetup,
    ) -> Result<Self, String> {
        let start = path::nearest_walkable(&scene.grid, setup.start, START_SEARCH)
            .ok_or("there is no walkable cell near the start")?;
        let mut log = Vec::new();
        let rules: Box<dyn Rules> = match &setup.tables {
            Some(dir) => {
                let (rules, problems) = ConquerRules::from_tables(dir);
                log.extend(
                    problems
                        .into_iter()
                        .map(|p| LogLine::Error(format!("Game rules: {p}"))),
                );
                Box::new(rules)
            }
            None => Box::new(ConquerRules::default()),
        };
        let mut sim = Sim::new(
            scene.grid.clone(),
            scene.npcs.clone(),
            scene.spawns.clone(),
            rules,
            setup.seed,
        );
        if let Some(dir) = &setup.tables {
            let file = dir.join(format!("{}{}", quest::QUEST_TABLE, c3_data::TABLE_SUFFIX));
            match c3_data::load_table(&file) {
                Ok(table) => sim.set_quests(Quest::all(&table)),
                Err(e) if file.exists() => log.push(LogLine::Error(format!("Quests: {e}"))),
                Err(_) => {}
            }
        }
        if let Some(dir) = setup.scripts {
            sim.set_scripts(ScriptHost::new(dir, setup.tables), setup.map_ids.clone());
        }
        let hero = sim.add_player("Hero", start, 1);
        sim.set_money(hero, START_MONEY);
        let gear = [
            DEFAULT_HERO.armor,
            DEFAULT_HERO.armet,
            DEFAULT_HERO.r_weapon,
            DEFAULT_HERO.l_weapon,
        ];
        for kind in gear {
            sim.equip(hero, kind);
        }
        // What happened while setting up is shown by attaching the view; keep the warnings.
        for event in sim.take_events() {
            if let Event::Warning(text) = event {
                log.push(LogLine::Error(text));
            }
        }
        let audio = GameAudio::new(content.clone(), setup.map_ids.clone());
        let mut world = World::new(content.clone(), map_path, scene);
        let view = SimView::attach(&sim, hero, &mut world)?;
        let mut ui = GameUi::default();
        ui.system(
            "Welcome! Click to walk, a monster to fight, an NPC to talk. B: bag · Enter: chat · /help",
            style::GOLD,
        );
        Ok(Self {
            sim,
            hero,
            world,
            view,
            ui,
            icons: ItemIcons::new(content),
            audio,
            log,
        })
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
        self.sim.set_running(self.hero, !running);
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
        self.sim.talk(self.hero, npc);
    }

    /// Sends the hero after the nearest monster it can reach.
    pub fn attack_nearest(&mut self) {
        if let Some(target) = self.sim.nearest_monster(self.hero, ATTACK_SEARCH) {
            self.sim.attack(self.hero, target);
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
            self.sim.talk(self.hero, npc);
        } else if let Some(monster) = self.monster_at(point) {
            self.sim.attack(self.hero, monster);
        } else if let Some(item) = self.view.item_at(&self.sim, &self.world, point) {
            self.sim.pick_up(self.hero, item);
        } else {
            let cell = self.world.scene.world_to_cell((point.x, point.y));
            self.sim.walk(self.hero, cell);
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
        self.sim.advance((seconds * 1000.0).round() as u64);
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
        let hero = self.hero;
        match action {
            UiAction::Answer(i) => self.sim.answer(hero, i),
            UiAction::CloseDialogue => self.sim.close_dialogue(hero),
            UiAction::Buy { npc, kind } => self.sim.buy(hero, npc, kind),
            UiAction::Sell { npc, item } => self.sim.sell(hero, npc, item),
            UiAction::CloseShop => self.sim.close_shop(hero),
            UiAction::Use(item) => self.sim.use_item(hero, item),
            UiAction::Unequip(item) => self.sim.unequip(hero, item),
            UiAction::ToggleSound => self.audio.toggle_muted(),
            UiAction::Say(text) if text.starts_with('/') => match self.sim.command(hero, &text) {
                Ok(answer) => self.ui.system(answer, style::GOOD),
                Err(why) => self.ui.system(why, style::WARN),
            },
            UiAction::Say(text) => {
                let name = self.sim.player(hero).map_or("You", |p| p.name.as_str());
                let line = format!("{name}: {text}");
                self.ui.system(line, style::TEXT);
            }
        }
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
