//! The game server: maps, logged-in players and the tick loop that serves them.

use crate::accounts::Accounts;
use crate::front::{Front, FrontEvent};
use crate::msg::{ClientMsg, Welcome};
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

pub struct ServerConfig {
    /// Folder of the account files.
    pub accounts: PathBuf,
    /// A new account's character (its name, at the first map's start).
    pub new_character: NewCharacter,
    /// How often logged-in characters are saved (ms of game time).
    pub save_every_ms: u64,
    /// Game time per tick (ms), the pace the server runs at.
    pub tick_ms: u64,
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
    since_save_ms: u64,
    log: Vec<String>,
}

impl GameServer {
    /// Serves `maps` (at least one) to the clients of `fronts`.
    pub fn new(fronts: Vec<Box<dyn Front>>, maps: Vec<MapWorld>, config: ServerConfig) -> Self {
        assert!(!maps.is_empty(), "a server needs a map");
        assert!(!fronts.is_empty(), "a server needs a front");
        Self {
            fronts,
            maps,
            accounts: Accounts::new(config.accounts.clone()),
            config,
            online: BTreeMap::new(),
            since_save_ms: 0,
            log: Vec::new(),
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
        for (index, map) in self.maps.iter_mut().enumerate() {
            map.sim.advance(ms);
            let events = map.sim.take_events();
            for event in &events {
                match event {
                    c3_game::Event::Warning(text) => self.log.push(format!("{}: {text}", map.key)),
                    c3_game::Event::ScriptLog(text) => self.log.push(format!("Script: {text}")),
                    _ => {}
                }
            }
            for (client, player) in self.online.iter_mut().filter(|(_, p)| p.map == index) {
                let update = map.sim.update_for(player.hero, &mut player.viewer, &events);
                self.fronts[client.front].update(client.conn, &update, map, player.hero);
            }
        }
        self.since_save_ms += ms;
        if self.since_save_ms >= self.config.save_every_ms {
            self.since_save_ms = 0;
            self.save_all();
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
            ClientMsg::Login {
                account, password, ..
            } => self.login(client, &account, &password),
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

    fn login(&mut self, client: Client, account: &str, password: &str) {
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
        let save = match self.accounts.login(account, password, new_character) {
            Ok(save) => save,
            Err(e) => return front.refuse(client.conn, &e.to_string()),
        };
        let map = self
            .maps
            .iter()
            .position(|m| m.key == save.map)
            .unwrap_or(0);
        let world = &mut self.maps[map];
        let hero = world.sim.add_saved_player(&save);
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
        let online: Vec<Client> = self.online.keys().copied().collect();
        for client in online {
            if let Some(p) = self.online.remove(&client) {
                self.save(&p);
                self.online.insert(client, p);
            }
        }
    }
}
