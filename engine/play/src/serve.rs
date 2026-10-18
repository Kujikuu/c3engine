//! Running a game server on a thread of its own: the maps (as scenes) become simulations
//! there, players log in to the accounts in a folder, and the host reads its log and stops
//! it. The `c3-server` app and the Studio's Play mode (through a local server) both start one.
//!
//! The server speaks the engine's protocol; a game may add fronts for other clients (a game's
//! original protocol), all serving the same worlds.

use crate::game::{self, GameSetup};
use crate::module::GameRules;
use c3_core::ProjectSettings;
use c3_online::{EngineFront, Front, GameServer, MapWorld, ServerConfig};
use c3_scene::{CellPos, Scene, path};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

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
    /// The game's rules.
    pub rules: Arc<dyn GameRules>,
    /// The project's settings (gameplay numbers, new characters, saving).
    pub settings: ProjectSettings,
    /// Fronts for other clients beside the engine's own, already listening.
    pub fronts: Vec<Box<dyn Front + Send>>,
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
        let front = EngineFront::bind(setup.listen.as_str())
            .map_err(|e| format!("can't listen on {}: {e}", setup.listen))?;
        let addr = front.local_addr();
        let ServeSetup {
            scripts,
            tables,
            accounts,
            rules,
            settings,
            fronts: extra,
            ..
        } = setup;
        let mut fronts: Vec<Box<dyn Front + Send>> = vec![Box::new(front)];
        fronts.extend(extra);
        let stop = Arc::new(AtomicBool::new(false));
        let log: Arc<Mutex<Vec<String>>> = Arc::default();
        let (stopping, sink) = (stop.clone(), log.clone());
        let thread = std::thread::Builder::new()
            .name("c3 game server".into())
            .spawn(move || {
                let say = |line: String| sink.lock().expect("server log").push(line);
                let seed = game::time_seed();
                let worlds = maps
                    .into_iter()
                    .map(|m| {
                        let setup = GameSetup {
                            scripts: scripts.clone(),
                            tables: tables.clone(),
                            map_ids: m.map_ids.clone(),
                            seed,
                            settings: settings.gameplay.clone(),
                        };
                        let (sim, problems) = game::new_sim(&m.scene, &setup, &*rules);
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
                for front in fronts.iter().skip(1) {
                    say(format!("Also serving clients on {}", front.local_addr()));
                }
                let gameplay = settings.gameplay.clone();
                let config = ServerConfig {
                    accounts,
                    new_character: Box::new(move |name, cell| {
                        crate::settings::new_character(&gameplay, name, cell)
                    }),
                    save_every_ms: seconds_to_ms(settings.network.save_interval_seconds),
                    tick_ms: settings.gameplay.tick_ms,
                };
                let fronts = fronts.into_iter().map(|f| f as Box<dyn Front>).collect();
                let mut server = GameServer::new(fronts, worlds, config);
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

    /// The address clients of the engine's protocol connect to.
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

fn seconds_to_ms(seconds: f64) -> u64 {
    (seconds * 1000.0).round().max(1.0) as u64
}
