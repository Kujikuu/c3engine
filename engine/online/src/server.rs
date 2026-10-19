//! The game server: maps, logged-in players and the tick loop that serves them.

use crate::accounts::{Accounts, Enter};
use crate::front::{Front, FrontEvent};
use crate::msg::{CharacterSummary, ClientMsg, Welcome};
use c3_game::{EntityId, PlayerSave, Sim, Viewer};
use c3_net::ConnId;
use c3_scene::CellPos;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// One map the server runs.
pub struct MapWorld {
    /// How the game names the map (sent to clients, which load it by this).
    pub key: String,
    /// The game's ids for the map.
    pub map_ids: Vec<u32>,
    pub sim: Sim,
    /// Where new characters start on it.
    pub start: CellPos,
}

/// Makes a new account's character: its name, at a start cell.
pub type NewCharacter = Box<dyn Fn(&str, CellPos) -> PlayerSave>;

/// A map the server is asked for: by a game id (a portal), or by key (a saved character's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapKey<'a> {
    Id(u32),
    Key(&'a str),
}

/// Loads a map the server doesn't run yet.
pub type MapLoader = Box<dyn FnMut(MapKey) -> Result<MapWorld, String>>;

/// Where the server's player ids start: above any id a map's simulation gives out, so a
/// player keeps theirs from map to map.
const PLAYER_IDS: u32 = 1 << 30;

pub struct ServerConfig {
    /// Folder of the account files.
    pub accounts: PathBuf,
    /// A new account's character (its name, at the first map's start).
    pub new_character: NewCharacter,
    /// Characters one account may have.
    pub characters_per_account: usize,
    /// How often logged-in characters are saved (ms of game time).
    pub save_every_ms: u64,
    /// Game time per tick (ms), the pace the server runs at.
    pub tick_ms: u64,
    /// Loads the maps players go to that the server doesn't run yet; without it, the
    /// server runs only the maps it started with.
    pub load_map: Option<MapLoader>,
}

/// A cell near `start` picked by `name`, so new characters don't all stand on one spot.
fn spread(start: CellPos, name: &str) -> CellPos {
    let h = name
        .bytes()
        .fold(17u32, |h, b| h.wrapping_mul(31).wrapping_add(b.into()));
    let offset = |v: u32| (v % 7) as i32 - 3;
    (start.0 + offset(h), start.1 + offset(h / 7))
}

/// A client: which front it came through, and its connection there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Client {
    front: usize,
    conn: ConnId,
}

/// A logged-in player.
struct Online {
    account: String,
    map: usize,
    hero: EntityId,
    viewer: Viewer,
}

pub struct GameServer {
    fronts: Vec<Box<dyn Front>>,
    maps: Vec<MapWorld>,
    accounts: Accounts,
    config: ServerConfig,
    online: BTreeMap<Client, Online>,
    /// The next player id to give out.
    next_player: u32,
    since_save_ms: u64,
    log: Vec<String>,
    /// What every map shares (guilds, map owners, world variables), kept beside the accounts.
    world: c3_game::WorldState,
}

/// The file the shared world state is kept in, in the accounts folder (accounts can't start
/// with `_`).
const WORLD_FILE: &str = "_world.ron";

impl GameServer {
    /// Serves `maps` (at least one) to the clients of `fronts`.
    pub fn new(fronts: Vec<Box<dyn Front>>, maps: Vec<MapWorld>, config: ServerConfig) -> Self {
        assert!(!maps.is_empty(), "a server needs a map");
        assert!(!fronts.is_empty(), "a server needs a front");
        let world = Self::load_world(&config.accounts);
        Self {
            fronts,
            maps,
            accounts: Accounts::new(config.accounts.clone()),
            config,
            online: BTreeMap::new(),
            next_player: PLAYER_IDS,
            since_save_ms: 0,
            log: Vec::new(),
            world,
        }
    }

    /// The shared world state saved in `dir` (none yet: an empty one).
    fn load_world(dir: &std::path::Path) -> c3_game::WorldState {
        std::fs::read_to_string(dir.join(WORLD_FILE))
            .ok()
            .and_then(|text| ron::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Keeps the shared world state.
    fn save_world(&mut self) {
        let file = self.config.accounts.join(WORLD_FILE);
        let text = ron::ser::to_string_pretty(&self.world, ron::ser::PrettyConfig::default());
        let saved = text
            .map_err(|e| e.to_string())
            .and_then(|t| std::fs::write(&file, t).map_err(|e| e.to_string()));
        if let Err(e) = saved {
            self.log.push(format!("Couldn't save the world state: {e}"));
        }
    }

    /// Makes the maps' changes to the shared state and hands every map the result.
    fn share_world(&mut self) {
        let changes: Vec<_> = (self.maps.iter_mut())
            .flat_map(|m| m.sim.take_world_changes())
            .collect();
        if changes.is_empty() && self.maps.iter().all(|m| m.sim.world_state() == &self.world) {
            return;
        }
        for change in &changes {
            self.world.apply(change);
        }
        for map in &mut self.maps {
            map.sim.set_world_state(self.world.clone());
        }
    }

    /// The first front's address.
    pub fn local_addr(&self) -> SocketAddr {
        self.fronts[0].local_addr()
    }

    /// Players logged in.
    pub fn players_online(&self) -> usize {
        self.online.len()
    }

    /// What the server has to say since the last call (logins, problems, script output).
    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }

    /// Runs `ms` of game: takes in what clients sent, advances every map and shows each
    /// player what changed around them.
    pub fn tick(&mut self, ms: u64) {
        for front in 0..self.fronts.len() {
            for event in self.fronts[front].poll() {
                match event {
                    FrontEvent::Message(conn, msg) => self.received(Client { front, conn }, msg),
                    FrontEvent::Closed(conn) => self.leave(Client { front, conn }),
                    FrontEvent::Problem(text) => self.log.push(text),
                }
            }
        }
        self.share_rosters();
        self.share_map_population();
        let mut travels = Vec::new();
        for (index, map) in self.maps.iter_mut().enumerate() {
            map.sim.advance(ms);
            let events = map.sim.take_events();
            for event in &events {
                match *event {
                    c3_game::Event::Warning(ref text) => {
                        self.log.push(format!("{}: {text}", map.key));
                    }
                    c3_game::Event::ScriptLog(ref text) => {
                        self.log.push(format!("Script: {text}"));
                    }
                    c3_game::Event::Travel { player, map, cell } => {
                        travels.push((index, player, map, cell));
                    }
                    _ => {}
                }
            }
            for (client, player) in self.online.iter_mut().filter(|(_, p)| p.map == index) {
                let update = map.sim.update_for(player.hero, &mut player.viewer, &events);
                self.fronts[client.front].update(client.conn, &update, map, player.hero);
            }
        }
        self.carry_team_ops();
        self.carry_npc_moves();
        self.share_world();
        for (from, hero, map, cell) in travels {
            self.travel(from, hero, map, cell);
        }
        self.since_save_ms += ms;
        if self.since_save_ms >= self.config.save_every_ms {
            self.since_save_ms = 0;
            self.save_all();
        }
    }

    /// Tells each map who plays on the others, so teams reach across maps.
    fn share_rosters(&mut self) {
        let roster: Vec<(usize, c3_game::Mate)> = (self.maps.iter().enumerate())
            .flat_map(|(i, m)| m.sim.mates().into_iter().map(move |mate| (i, mate)))
            .collect();
        for (index, map) in self.maps.iter_mut().enumerate() {
            let others = roster.iter().filter(|(i, _)| *i != index);
            map.sim
                .set_elsewhere(others.map(|(_, m)| m.clone()).collect());
        }
    }

    /// Tells every map how many players (all, alive) are on each map, for scripts.
    fn share_map_population(&mut self) {
        let mut population = std::collections::BTreeMap::new();
        for map in &self.maps {
            let all = map.sim.players().count() as u32;
            let alive = map.sim.players().filter(|p| p.is_alive()).count() as u32;
            for &id in map.sim.map_ids() {
                population.insert(id, (all, alive));
            }
        }
        for map in &mut self.maps {
            map.sim.set_map_population(population.clone());
        }
    }

    /// Carries what teams asked of players on other maps to those maps.
    fn carry_team_ops(&mut self) {
        let ops: Vec<_> = (self.maps.iter_mut())
            .flat_map(|m| m.sim.take_team_ops())
            .collect();
        for (player, op) in ops {
            if let Some(map) = self
                .maps
                .iter_mut()
                .find(|m| m.sim.player(player).is_some())
            {
                map.sim.apply_team_op(player, op);
            }
        }
    }

    /// Ticks in real time until `stop` is set, then saves everyone; the server's log lines
    /// go to `log` as they come.
    pub fn run(&mut self, stop: &AtomicBool, mut log: impl FnMut(&str)) {
        let tick = self.config.tick_ms.max(1);
        let step = Duration::from_millis(tick);
        let mut next = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            self.tick(tick);
            for line in self.take_log() {
                log(&line);
            }
            next += step;
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else {
                // Fell behind (a stall): carry on from now rather than rush.
                next = now;
            }
        }
        self.shutdown();
        for line in self.take_log() {
            log(&line);
        }
    }

    /// Saves every logged-in character and closes their connections.
    pub fn shutdown(&mut self) {
        self.save_all();
        for client in self.online.keys() {
            self.fronts[client.front].kick(client.conn, "The server is shutting down.");
        }
    }

    fn received(&mut self, client: Client, msg: ClientMsg) {
        match msg {
            ClientMsg::Characters {
                account, password, ..
            } => self.list_characters(client, &account, &password),
            ClientMsg::Login {
                account,
                password,
                enter,
                ..
            } => self.login(client, &account, &password, &enter),
            ClientMsg::Input(input) => {
                if let Some(p) = self.online.get(&client) {
                    self.maps[p.map].sim.input(p.hero, input);
                }
            }
            ClientMsg::Logout => {
                self.leave(client);
                self.fronts[client.front].kick(client.conn, "Logged out.");
            }
        }
    }

    /// Tells `client` the characters of `account`.
    fn list_characters(&mut self, client: Client, account: &str, password: &str) {
        let front = &mut self.fronts[client.front];
        match self.accounts.characters(account, password) {
            Ok(saves) => {
                let list: Vec<CharacterSummary> = (saves.into_iter())
                    .map(|s| CharacterSummary {
                        name: s.name,
                        level: s.level,
                        look: s.look,
                        map: s.map,
                    })
                    .collect();
                front.characters(client.conn, &list);
            }
            Err(e) => front.refuse(client.conn, &e.to_string()),
        }
    }

    fn login(&mut self, client: Client, account: &str, password: &str, enter: &Enter) {
        if self.online.contains_key(&client) {
            return;
        }
        let front = &mut self.fronts[client.front];
        if self
            .online
            .values()
            .any(|p| p.account.eq_ignore_ascii_case(account))
        {
            return front.refuse(client.conn, "This account is already playing.");
        }
        let first = &self.maps[0];
        let (start, first_key) = (first.start, first.key.clone());
        // Next to the start, if it is a short walk from there (never behind a wall).
        let near = spread(start, account);
        let near = if first.sim.reachable(start, near, 8) {
            near
        } else {
            start
        };
        let new_character = |name: &str| {
            let mut save = (self.config.new_character)(name, near);
            save.map = first_key.clone();
            save
        };
        let most = self.config.characters_per_account;
        let mut save = match self
            .accounts
            .login(account, password, enter, most, new_character)
        {
            Ok(save) => save,
            Err(e) => return front.refuse(client.conn, &e.to_string()),
        };
        // An unnamed map is the first.
        let found = match save.map.as_str() {
            "" => Ok(0),
            key => self.map_index(MapKey::Key(key)),
        };
        let map = found.unwrap_or_else(|e| {
            // The map is gone: start over at the first map's start.
            self.log
                .push(format!("{account}'s map {} can't be played: {e}", save.map));
            save.cell = self.maps[0].start;
            0
        });
        let hero = EntityId(self.next_player);
        self.next_player += 1;
        let world = &mut self.maps[map];
        world.sim.add_saved_player_as(&save, hero);
        world.sim.logged_in(hero);
        let front = &mut self.fronts[client.front];
        let welcome = Welcome {
            hero,
            map: world.key.clone(),
            map_ids: world.map_ids.clone(),
        };
        front.welcome(client.conn, &welcome, world);
        self.log
            .push(format!("{account} logged in on {}", world.key));
        self.online.insert(
            client,
            Online {
                account: account.to_owned(),
                map,
                hero,
                viewer: Viewer::default(),
            },
        );
    }

    /// The index of map `key`, loaded if the server doesn't run it yet.
    fn map_index(&mut self, key: MapKey) -> Result<usize, String> {
        let found = self.maps.iter().position(|m| match key {
            MapKey::Id(id) => m.map_ids.contains(&id),
            MapKey::Key(k) => m.key == k,
        });
        if let Some(index) = found {
            return Ok(index);
        }
        let load = self
            .config
            .load_map
            .as_mut()
            .ok_or("the server runs only the maps it started with")?;
        let world = load(key)?;
        self.log.push(format!("Loaded map {}", world.key));
        self.maps.push(world);
        Ok(self.maps.len() - 1)
    }

    /// Brings NPCs that left a map to the maps they went to (loaded if need be).
    fn carry_npc_moves(&mut self) {
        let moves: Vec<_> = (self.maps.iter_mut())
            .flat_map(|m| m.sim.take_npc_moves())
            .collect();
        for (map, npc) in moves {
            match self.map_index(MapKey::Id(map)) {
                Ok(index) => self.maps[index].sim.arrive_npc(npc),
                Err(e) => self
                    .log
                    .push(format!("NPC {} can't go to map {map}: {e}", npc.id)),
            }
        }
    }

    /// Takes player `hero` from map `from` to `cell` of the map with game id `map`.
    fn travel(&mut self, from: usize, hero: EntityId, map: u32, cell: CellPos) {
        let Some(client) = self
            .online
            .iter()
            .find(|(_, p)| p.map == from && p.hero == hero)
            .map(|(c, _)| *c)
        else {
            return;
        };
        let to = match self.map_index(MapKey::Id(map)) {
            Ok(to) => to,
            Err(e) => {
                let account = &self.online[&client].account;
                self.log
                    .push(format!("{account} can't go to map {map}: {e}"));
                return;
            }
        };
        let Some(mut save) = self.maps[from].sim.save_player(hero) else {
            return;
        };
        // The hero keeps its team on the next map.
        let team = self.maps[from].sim.team_of(hero);
        self.maps[from].sim.travel_out(hero);
        let world = &mut self.maps[to];
        save.map = world.key.clone();
        save.cell = cell;
        world.sim.add_saved_player_as(&save, hero);
        world.sim.set_team(hero, team);
        let welcome = Welcome {
            hero,
            map: world.key.clone(),
            map_ids: world.map_ids.clone(),
        };
        self.fronts[client.front].moved(client.conn, &welcome, world);
        let p = self.online.get_mut(&client).expect("found above");
        p.map = to;
        p.viewer = Viewer::default();
        self.log
            .push(format!("{} went to {}", p.account, world.key));
    }

    /// The player of `client` left: save their character and take it out of the world.
    fn leave(&mut self, client: Client) {
        let Some(p) = self.online.remove(&client) else {
            return;
        };
        self.save(&p);
        self.maps[p.map].sim.remove_player(p.hero);
        self.log.push(format!("{} logged out", p.account));
    }

    fn save(&mut self, p: &Online) {
        let world = &self.maps[p.map];
        let Some(mut save) = world.sim.save_player(p.hero) else {
            return;
        };
        save.map = world.key.clone();
        if let Err(e) = self.accounts.save(&p.account, save) {
            self.log.push(format!("Couldn't save {}: {e}", p.account));
        }
    }

    fn save_all(&mut self) {
        self.save_world();
        let online: Vec<Client> = self.online.keys().copied().collect();
        for client in online {
            if let Some(p) = self.online.remove(&client) {
                self.save(&p);
                self.online.insert(client, p);
            }
        }
    }
}
