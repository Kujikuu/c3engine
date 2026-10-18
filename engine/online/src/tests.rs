use super::*;
use c3_game::{
    Drop, EntityId, Event, Fighter, Input, ItemType, MonsterType, PlayerSave, Rng, Rules, Sim,
    Stats, WorldUpdate,
};
use c3_scene::{Grid, Spawn};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const SLIME: u32 = 1;

struct TestRules {
    slime: MonsterType,
}

impl Rules for TestRules {
    fn monster(&self, kind: u32) -> Option<&MonsterType> {
        (kind == SLIME).then_some(&self.slime)
    }
    fn item(&self, _kind: u32) -> Option<&ItemType> {
        None
    }
    fn player_stats(&self, level: u32) -> Stats {
        Stats {
            max_life: 100 + level,
            attack_min: 10,
            attack_max: 10,
            attack_range: 1,
            attack_ms: 800,
            step_ms: 200,
            ..Default::default()
        }
    }
    fn level_exp(&self, _level: u32) -> Option<u64> {
        Some(1000)
    }
    fn hit(&self, _a: &Fighter, _t: &Fighter, _rng: &mut Rng) -> u32 {
        1
    }
    fn hit_exp(&self, _level: u32, _t: &Fighter, _damage: u32) -> u64 {
        0
    }
    fn kill_exp(&self, _level: u32, _t: &Fighter) -> u64 {
        0
    }
    fn drops(&self, _kind: u32, _rng: &mut Rng) -> Vec<Drop> {
        Vec::new()
    }
    fn shop(&self, _npc: u32) -> &[u32] {
        &[]
    }
}

fn rules() -> Box<dyn Rules> {
    Box::new(TestRules {
        slime: MonsterType {
            id: SLIME,
            name: "Slime".into(),
            look: 101,
            level: 1,
            stats: Stats {
                max_life: 1000,
                step_ms: 100_000,
                attack_ms: 1000,
                attack_range: 1,
                ..Default::default()
            },
            view_range: 3,
            aggressive: false,
            ..MonsterType::default()
        },
    })
}

fn world() -> Sim {
    let spawn = Spawn {
        id: 1,
        monster: SLIME,
        min: (14, 10),
        max: (14, 10),
        max_alive: 1,
        respawn_secs: 5,
        per_wave: 1,
        extra: Default::default(),
    };
    Sim::new(Grid::new(80, 80), vec![], vec![spawn], rules(), 1)
}

fn server(test: &str) -> (GameServer, PathBuf) {
    let dir = std::env::temp_dir().join(format!("c3-online-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    (server_in(&dir, world(), None), dir)
}

/// A server keeping its accounts in `dir`, starting with map "plain" (`plain`).
fn server_in(dir: &std::path::Path, plain: Sim, load_map: Option<MapLoader>) -> GameServer {
    let front = EngineFront::bind("127.0.0.1:0").unwrap();
    let maps = vec![MapWorld {
        key: "plain".into(),
        map_ids: vec![7],
        sim: plain,
        start: (10, 10),
    }];
    let config = ServerConfig {
        accounts: dir.to_owned(),
        new_character: Box::new(|name, cell| PlayerSave {
            money: 100,
            ..PlayerSave::new(name, cell)
        }),
        characters_per_account: 3,
        save_every_ms: 60_000,
        tick_ms: 50,
        load_map,
    };
    GameServer::new(vec![Box::new(front)], maps, config)
}

/// A client's game: its connection and its mirror of the world.
struct Player {
    conn: Connection,
    hero: EntityId,
    /// The map it is on.
    map: String,
    mirror: Sim,
    events: Vec<Event>,
}

impl Player {
    fn join(server: &mut GameServer, account: &str, password: &str) -> Result<Self, String> {
        let addr = server.local_addr();
        // The server must tick while the client waits to be let in.
        let joining = std::thread::spawn({
            let (account, password) = (account.to_owned(), password.to_owned());
            move || Connection::join(addr, &account, &password, Duration::from_secs(5))
        });
        while !joining.is_finished() {
            server.tick(50);
            std::thread::sleep(Duration::from_millis(2));
        }
        let (conn, welcome, updates) = joining.join().unwrap()?;
        let mut player = Self {
            conn,
            hero: welcome.hero,
            map: welcome.map,
            mirror: Sim::new(Grid::new(80, 80), vec![], vec![], rules(), 0),
            events: Vec::new(),
        };
        for u in updates {
            player.apply(u);
        }
        Ok(player)
    }

    fn apply(&mut self, update: WorldUpdate) {
        self.mirror.apply_update(update);
        self.events.extend(self.mirror.take_events());
    }

    fn pump(&mut self) {
        for event in self.conn.poll() {
            match event {
                RemoteEvent::Update(u) => self.apply(u),
                // Moved to another map: a mirror of that one.
                RemoteEvent::Welcome(w) => {
                    self.hero = w.hero;
                    self.map = w.map;
                    self.mirror = Sim::new(Grid::new(80, 80), vec![], vec![], rules(), 0);
                }
                _ => {}
            }
        }
    }
}

/// Ticks the server and pumps the players until `done` holds.
fn run_until(
    server: &mut GameServer,
    players: &mut [&mut Player],
    done: impl Fn(&[&mut Player]) -> bool,
) {
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        server.tick(50);
        std::thread::sleep(Duration::from_millis(3));
        for p in players.iter_mut() {
            p.pump();
        }
        if done(players) {
            return;
        }
    }
    panic!("timed out");
}

#[test]
fn players_log_in_see_the_world_and_each_other() {
    let (mut server, dir) = server("play");
    let mut a = Player::join(&mut server, "alice", "pw").unwrap();
    assert_eq!(a.map, "plain");
    let me = a.mirror.player(a.hero).expect("the hero is mirrored");
    let start = me.cell;
    assert!(
        c3_game::distance(start, (10, 10)) <= 3,
        "near the start: {start:?}"
    );
    assert_eq!(me.money, 100);
    assert_eq!(a.mirror.monsters().count(), 1, "the slime nearby is seen");

    // Walking happens on the server; the mirror follows with the steps.
    a.conn.send(Input::Walk((12, 10)));
    run_until(&mut server, &mut [&mut a], |p| {
        p[0].mirror.player(p[0].hero).unwrap().cell == (12, 10)
    });
    assert!(
        a.events
            .iter()
            .any(|e| matches!(e, Event::Moved { id, .. } if *id == a.hero))
    );

    // A second player appears to the first, without the first's private state.
    let mut b = Player::join(&mut server, "bob", "pw").unwrap();
    run_until(&mut server, &mut [&mut a, &mut b], |p| {
        p[0].mirror.player(p[1].hero).is_some()
    });
    let bob_seen = a.mirror.player(b.hero).unwrap();
    assert_eq!(bob_seen.money, 0, "others' money isn't sent");
    assert!(b.mirror.player(a.hero).is_some());

    // Chat reaches those around; command answers only their sender.
    b.conn.send(Input::Say("hello".into()));
    b.conn.send(Input::Say("/heal".into()));
    run_until(&mut server, &mut [&mut a, &mut b], |p| {
        p[0].events
            .iter()
            .any(|e| matches!(e, Event::Chat { text, .. } if text == "hello"))
            && p[1]
                .events
                .iter()
                .any(|e| matches!(e, Event::CommandReply { ok: true, .. }))
    });
    assert!(
        !a.events
            .iter()
            .any(|e| matches!(e, Event::CommandReply { .. }))
    );

    // Bob leaves: Alice sees him go; his character is kept.
    b.conn.logout();
    run_until(&mut server, &mut [&mut a], |p| {
        p[0].mirror.player(b.hero).is_none()
    });
    assert_eq!(server.players_online(), 1);
    drop(server);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn characters_are_kept_and_passwords_checked() {
    let (mut server, dir) = server("accounts");
    let mut a = Player::join(&mut server, "carol", "secret").unwrap();
    a.conn.send(Input::Walk((11, 12)));
    run_until(&mut server, &mut [&mut a], |p| {
        p[0].mirror.player(p[0].hero).unwrap().cell == (11, 12)
    });
    assert_eq!(
        Player::join(&mut server, "carol", "secret")
            .err()
            .as_deref(),
        Some("This account is already playing.")
    );
    a.conn.logout();
    let end = Instant::now() + Duration::from_secs(5);
    while server.players_online() > 0 && Instant::now() < end {
        server.tick(50);
    }
    assert_eq!(
        Player::join(&mut server, "carol", "wrong").err().as_deref(),
        Some("Wrong password.")
    );
    let back = Player::join(&mut server, "carol", "secret").unwrap();
    assert_eq!(
        back.mirror.player(back.hero).unwrap().cell,
        (11, 12),
        "where it logged out"
    );
    assert!(
        Player::join(&mut server, "x", "pw").is_err(),
        "names are 3+ characters"
    );
    drop(server);
    let _ = std::fs::remove_dir_all(dir);
}

/// A plain map with a portal at (12, 10) to cell (5, 6) of map 8, "cave", which servers
/// load when asked for it.
fn portal_world() -> (Sim, MapLoader) {
    let mut plain = world();
    plain.set_map_ids(vec![7]);
    plain.set_portals(&[c3_scene::Portal {
        cell: (12, 10),
        id: 1,
        to: Some(c3_scene::PortalTarget {
            map: 8,
            cell: (5, 6),
        }),
    }]);
    let load = |key: MapKey| match key {
        MapKey::Id(8) | MapKey::Key("cave") => {
            let mut sim = Sim::new(Grid::new(80, 80), vec![], vec![], rules(), 2);
            sim.set_map_ids(vec![8]);
            Ok(MapWorld {
                key: "cave".into(),
                map_ids: vec![8],
                sim,
                start: (5, 5),
            })
        }
        other => Err(format!("no map {other:?}")),
    };
    (plain, Box::new(load))
}

#[test]
fn portals_move_players_to_maps_loaded_when_needed() {
    let dir = std::env::temp_dir().join(format!("c3-online-{}-travel", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (plain, load) = portal_world();
    let mut server = server_in(&dir, plain, Some(load));
    let mut a = Player::join(&mut server, "dave", "pw").unwrap();
    let hero = a.hero;
    a.conn.send(Input::Walk((12, 10)));
    run_until(&mut server, &mut [&mut a], |p| {
        p[0].map == "cave"
            && p[0]
                .mirror
                .player(p[0].hero)
                .is_some_and(|me| me.cell == (5, 6))
    });
    assert_eq!(a.hero, hero, "a player keeps its id from map to map");
    assert!(
        server.take_log().iter().any(|l| l == "Loaded map cave"),
        "cave was loaded when dave went"
    );
    a.conn.logout();
    let end = Instant::now() + Duration::from_secs(5);
    while server.players_online() > 0 && Instant::now() < end {
        server.tick(50);
    }
    drop(server);

    // A new server loads the map the character was saved on.
    let (plain, load) = portal_world();
    let mut server = server_in(&dir, plain, Some(load));
    let back = Player::join(&mut server, "dave", "pw").unwrap();
    assert_eq!(back.map, "cave");
    assert_eq!(back.mirror.player(back.hero).unwrap().cell, (5, 6));
    drop(server);
    let _ = std::fs::remove_dir_all(dir);
}
