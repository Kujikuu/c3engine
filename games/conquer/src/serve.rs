//! Running a Conquer game server on a thread of its own: the maps (as scenes) become
//! simulations there, players log in to the accounts in a folder, and the host reads its log
//! and stops it. The `c3-server` app and the Studio's Play mode (through a local server)
//! both start one.

use crate::game::{self, GameSetup};
use c3_net::{Server, Wire};
use c3_online::{GameServer, MapWorld, ServerConfig};
use c3_scene::{CellPos, Scene, path};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// How often logged-in characters are saved (ms).
const SAVE_EVERY_MS: u64 = 60_000;
/// How far (cells) the start may be moved to stand on walkable ground.
const START_SEARCH: i32 = 50;

/// A map to serve.
pub struct ServedMap {
    /// How clients load it (an engine scene's name or a content map path).
    pub key: String,
    pub scene: Scene,
    pub map_ids: Vec<u32>,
    /// Where new characters start.
    pub start: CellPos,
}

/// Where the served game's data is.
pub struct ServeSetup {
    /// Address to listen on (`127.0.0.1:0` picks a free port).
    pub listen: String,
    pub scripts: Option<PathBuf>,
    pub tables: Option<PathBuf>,
    /// Folder of the account files.
    pub accounts: PathBuf,
}

/// A running server.
pub struct ServerHandle {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    log: Arc<Mutex<Vec<String>>>,
    thread: Option<JoinHandle<()>>,
}

impl ServerHandle {
    /// Starts serving `maps` (at least one).
    pub fn spawn(maps: Vec<ServedMap>, setup: ServeSetup) -> Result<Self, String> {
        if maps.is_empty() {
            return Err("there is no map to serve".into());
        }
        let net = Server::bind(setup.listen.as_str(), Wire::plain)
            .map_err(|e| format!("can't listen on {}: {e}", setup.listen))?;
        let addr = net.local_addr();
        let stop = Arc::new(AtomicBool::new(false));
        let log: Arc<Mutex<Vec<String>>> = Arc::default();
        let (stopping, sink) = (stop.clone(), log.clone());
        let thread = std::thread::Builder::new()
            .name("c3 game server".into())
            .spawn(move || {
                let say = |line: String| sink.lock().expect("server log").push(line);
                let seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(1, |d| d.as_secs());
                let worlds = maps
                    .into_iter()
                    .map(|m| {
                        let game = GameSetup {
                            scripts: setup.scripts.clone(),
                            tables: setup.tables.clone(),
                            map_ids: m.map_ids.clone(),
                            seed,
                        };
                        let (sim, problems) = game::new_sim(&m.scene, &game);
                        problems.into_iter().for_each(&say);
                        let start = path::nearest_walkable(&m.scene.grid, m.start, START_SEARCH)
                            .unwrap_or(m.start);
                        MapWorld {
                            key: m.key,
                            map_ids: m.map_ids,
                            sim,
                            start,
                        }
                    })
                    .collect::<Vec<_>>();
                say(format!("Serving {} map(s) on {addr}", worlds.len()));
                let config = ServerConfig {
                    accounts: setup.accounts,
                    new_character: Box::new(game::new_character),
                    save_every_ms: SAVE_EVERY_MS,
                };
                let mut server = GameServer::new(net, worlds, config);
                server.run(&stopping, |line| say(line.to_owned()));
                say("The server stopped; characters saved.".into());
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            addr,
            stop,
            log,
            thread: Some(thread),
        })
    }

    /// The address clients connect to.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// What the server said since the last call.
    pub fn take_log(&self) -> Vec<String> {
        std::mem::take(&mut *self.log.lock().expect("server log"))
    }

    /// Whether it still runs.
    pub fn is_running(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    /// Stops it (characters are saved) and waits until it has.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}
