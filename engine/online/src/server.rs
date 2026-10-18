//! The game server: maps, logged-in players and the tick loop that serves them.

use crate::accounts::Accounts;
use crate::msg::{ClientMsg, PROTOCOL, ServerMsg, Welcome, decode, encode};
use c3_game::{EntityId, PlayerSave, Sim, TICK_MS, Viewer};
use c3_net::{ConnId, NetEvent, Server};
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
}

/// A cell near `start` picked by `name`, so new characters don't all stand on one spot.
fn spread(start: CellPos, name: &str) -> CellPos {
    let h = name
        .bytes()
        .fold(17u32, |h, b| h.wrapping_mul(31).wrapping_add(b.into()));
    let offset = |v: u32| (v % 7) as i32 - 3;
    (start.0 + offset(h), start.1 + offset(h / 7))
}

/// A logged-in player.
struct Online {
    account: String,
    map: usize,
    hero: EntityId,
    viewer: Viewer,
}

pub struct GameServer {
    net: Server,
    maps: Vec<MapWorld>,
    accounts: Accounts,
    config: ServerConfig,
    online: BTreeMap<ConnId, Online>,
    since_save_ms: u64,
    log: Vec<String>,
}

impl GameServer {
    /// Serves `maps` (at least one) to the clients of `net`.
    pub fn new(net: Server, maps: Vec<MapWorld>, config: ServerConfig) -> Self {
        assert!(!maps.is_empty(), "a server needs a map");
        Self {
            net,
            maps,
            accounts: Accounts::new(config.accounts.clone()),
            config,
            online: BTreeMap::new(),
            since_save_ms: 0,
            log: Vec::new(),
        }
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.net.local_addr()
    }

    /// Players logged in.
    pub fn players_online(&self) -> usize {
        self.online.len()
    }

    /// What the server has to say since the last call (logins, problems, script output).
    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }

    /// Runs `ms` of game: takes in what clients sent, advances every map and sends each
    /// player what changed around them.
    pub fn tick(&mut self, ms: u64) {
        for event in self.net.poll() {
            match event {
                NetEvent::Connected { .. } => {}
                NetEvent::Received { id, packet } => self.received(id, &packet),
                NetEvent::Closed { id, .. } => self.leave(id),
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
            for (&conn, player) in self.online.iter_mut().filter(|(_, p)| p.map == index) {
                let update = map.sim.update_for(player.hero, &mut player.viewer, &events);
                if !update.is_empty() {
                    self.net.send(conn, encode(&ServerMsg::Update(update)));
                }
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
        let step = Duration::from_millis(TICK_MS);
        let mut next = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            self.tick(TICK_MS);
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
        for &conn in self.online.keys() {
            self.net.send(
                conn,
                encode(&ServerMsg::Kicked("The server is shutting down.".into())),
            );
            self.net.close(conn);
        }
    }

    fn received(&mut self, conn: ConnId, packet: &[u8]) {
        let msg: ClientMsg = match decode(packet) {
            Ok(msg) => msg,
            Err(e) => {
                self.log
                    .push(format!("Unreadable message from {conn:?}: {e}"));
                self.net.close(conn);
                return;
            }
        };
        match msg {
            ClientMsg::Login {
                protocol,
                account,
                password,
            } => self.login(conn, protocol, &account, &password),
            ClientMsg::Input(input) => {
                if let Some(p) = self.online.get(&conn) {
                    self.maps[p.map].sim.input(p.hero, input);
                }
            }
            ClientMsg::Logout => {
                self.leave(conn);
                self.net.close(conn);
            }
        }
    }

    fn refuse(&mut self, conn: ConnId, why: String) {
        self.net.send(conn, encode(&ServerMsg::Refused(why)));
        self.net.close(conn);
    }

    fn login(&mut self, conn: ConnId, protocol: u32, account: &str, password: &str) {
        if self.online.contains_key(&conn) {
            return;
        }
        if protocol != PROTOCOL {
            let why =
                format!("This server speaks protocol {PROTOCOL}; the client speaks {protocol}.");
            return self.refuse(conn, why);
        }
        if self
            .online
            .values()
            .any(|p| p.account.eq_ignore_ascii_case(account))
        {
            return self.refuse(conn, "This account is already playing.".into());
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
            Err(e) => return self.refuse(conn, e.to_string()),
        };
        let map = self
            .maps
            .iter()
            .position(|m| m.key == save.map)
            .unwrap_or(0);
        let world = &mut self.maps[map];
        let hero = world.sim.add_saved_player(&save);
        self.net.send(
            conn,
            encode(&ServerMsg::Welcome(Welcome {
                hero,
                map: world.key.clone(),
                map_ids: world.map_ids.clone(),
            })),
        );
        self.log
            .push(format!("{account} logged in on {}", world.key));
        self.online.insert(
            conn,
            Online {
                account: account.to_owned(),
                map,
                hero,
                viewer: Viewer::default(),
            },
        );
    }

    /// The player on `conn` left: save their character and take it out of the world.
    fn leave(&mut self, conn: ConnId) {
        let Some(p) = self.online.remove(&conn) else {
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
        let online: Vec<ConnId> = self.online.keys().copied().collect();
        for conn in online {
            if let Some(p) = self.online.remove(&conn) {
                self.save(&p);
                self.online.insert(conn, p);
            }
        }
    }
}
