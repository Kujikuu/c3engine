//! A Conquer client's login and first steps against a running server, through both ciphers.

use super::crypto::{ClientCipher, SERVER_PAD, TqClient};
use super::packets::{self as pk, Request, action};
use super::{ConquerFront, ConquerSetup};
use crate::ConquerGameRules;
use c3_net::packet::{Reader, Writer, packet_type};
use c3_online::MapKey;
use c3_play::serve::MapSource;
use c3_play::serve::{ServeSetup, ServedMap, ServerHandle};
use c3_scene::{Grid, IsoProjection, Npc, Portal, PortalTarget, Scene};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAP_ID: u32 = 1002;
/// The map the test map's portal leads to, loaded when first needed.
const OTHER_MAP_ID: u32 = 1003;
const PORTAL: (i32, i32) = (12, 12);
const NPC_ID: u32 = 7;
/// "test1234" as the client seals it.
const SEALED_PASSWORD: [u8; 16] = [
    0x66, 0x9c, 0xa8, 0x64, 0xed, 0x57, 0x3e, 0x60, 0x0d, 0xa6, 0x08, 0x4d, 0xf3, 0x67, 0xeb, 0x73,
];
const WAIT: Duration = Duration::from_secs(10);

fn scene() -> Scene {
    let grid = Grid::new(40, 40);
    Scene {
        projection: IsoProjection::for_grid(64.0, 32.0, grid.width),
        grid,
        ground: None,
        portals: vec![Portal {
            cell: PORTAL,
            id: 0,
            to: Some(PortalTarget {
                map: OTHER_MAP_ID,
                cell: (20, 20),
            }),
        }],
        entities: Vec::new(),
        layers: Vec::new(),
        npcs: vec![Npc {
            id: NPC_ID,
            name: "Guide".into(),
            cell: (12, 8),
            direction: 0,
            look: 2,
            kind: 2,
            script: String::new(),
            extra: BTreeMap::new(),
        }],
        spawns: Vec::new(),
        regions: Vec::new(),
        traps: Vec::new(),
        meta: BTreeMap::new(),
    }
}

/// A server with a Conquer front, and the front's account server address.
fn server(accounts: &std::path::Path) -> (ServerHandle, SocketAddr) {
    let front = ConquerFront::bind(&ConquerSetup {
        account_listen: "127.0.0.1:0".into(),
        game_listen: "127.0.0.1:0".into(),
        public_ip: "127.0.0.1".into(),
        public_port: 0,
    })
    .expect("Conquer front listens");
    let account = front.account_addr();
    let server = ServerHandle::spawn(
        vec![ServedMap {
            key: "test".into(),
            scene: scene(),
            map_ids: vec![MAP_ID],
            start: (10, 10),
        }],
        ServeSetup {
            listen: "127.0.0.1:0".into(),
            scripts: None,
            tables: None,
            accounts: accounts.to_owned(),
            rules: Arc::new(ConquerGameRules),
            settings: crate::module::conquer_settings(),
            fronts: vec![Box::new(front)],
            more_maps: Some(Box::new(|| {
                let source = |_: MapKey| {
                    Ok(ServedMap {
                        key: "other".into(),
                        scene: scene(),
                        map_ids: vec![OTHER_MAP_ID],
                        start: (10, 10),
                    })
                };
                Ok(Box::new(source) as MapSource)
            })),
        },
    )
    .expect("server starts");
    (server, account)
}

fn connect(addr: SocketAddr) -> TcpStream {
    let s = TcpStream::connect(addr).expect("connects");
    s.set_read_timeout(Some(WAIT)).unwrap();
    s
}

/// Logs in at the account server: the token and the game server it names.
fn account_login(addr: SocketAddr, account: &str) -> (u32, SocketAddr) {
    let mut s = connect(addr);
    let mut cipher = TqClient::default();
    let mut login = Writer::new();
    login
        .u16(52)
        .u16(pk::ACCOUNT)
        .fixed_str(account, 16)
        .bytes(&SEALED_PASSWORD)
        .fixed_str("C3", 16);
    let mut login = login.into_bytes();
    cipher.encrypt(&mut login);
    s.write_all(&login).unwrap();
    let mut reply = [0u8; 32];
    s.read_exact(&mut reply).expect("login reply");
    cipher.decrypt(&mut reply);
    assert_eq!(packet_type(&reply).unwrap(), pk::LOGIN_REPLY);
    let mut r = Reader::new(&reply[4..]);
    let token = r.u32().unwrap();
    assert_eq!(r.u32().unwrap(), 2, "a good login");
    let ip = r.fixed_str(16).unwrap();
    let port = r.u32().unwrap() as u16;
    (token, format!("{ip}:{port}").parse().unwrap())
}

struct GameClient {
    stream: TcpStream,
    cipher: ClientCipher,
}

impl GameClient {
    /// Connects and does the key exchange.
    fn connect(addr: SocketAddr) -> Self {
        let mut c = Self {
            stream: connect(addr),
            cipher: ClientCipher::new(),
        };
        let mut greeting = c.read(SERVER_PAD + 4);
        let rest = i32::from_le_bytes(greeting[SERVER_PAD..].try_into().unwrap()) as usize - 4;
        greeting.extend(c.read(rest));
        let reply = c.cipher.answer(&greeting);
        c.stream.write_all(&reply).unwrap();
        c
    }

    fn read(&mut self, n: usize) -> Vec<u8> {
        let mut buf = vec![0u8; n];
        self.stream
            .read_exact(&mut buf)
            .expect("the server answers");
        self.cipher.decrypt(&mut buf);
        buf
    }

    fn send(&mut self, mut packet: Vec<u8>) {
        packet.extend_from_slice(b"TQClient");
        self.cipher.encrypt(&mut packet);
        self.stream.write_all(&packet).unwrap();
    }

    fn receive(&mut self) -> Vec<u8> {
        let mut packet = self.read(2);
        let size = u16::from_le_bytes([packet[0], packet[1]]) as usize;
        packet.extend(self.read(size - 2 + 8));
        assert_eq!(&packet[size..], b"TQServer");
        packet.truncate(size);
        packet
    }

    /// The next packet of `kind` that `wanted` accepts.
    fn wait_for(&mut self, kind: u16, wanted: impl Fn(&[u8]) -> bool) -> Vec<u8> {
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            let packet = self.receive();
            if packet_type(&packet).unwrap() == kind && wanted(&packet) {
                return packet;
            }
        }
        panic!("no packet {kind} came");
    }
}

/// Logs `account` in through both servers into the map: the client and its hero's id.
fn enter(account_addr: SocketAddr, account: &str) -> (GameClient, u32) {
    let (token, game_addr) = account_login(account_addr, account);
    let mut game = GameClient::connect(game_addr);
    let mut connect = Writer::new();
    connect.u16(28).u16(pk::CONNECT).u32(token).u32(0).zeros(16);
    game.send(connect.into_bytes());
    let answer = game.wait_for(pk::TALK, |_| true);
    assert!(
        matches!(pk::parse(&answer).unwrap(), Request::Talk { words, .. } if words == "ANSWER_OK")
    );
    let hero = u32_at(&game.wait_for(pk::USER_INFO, |_| true), 4);
    assert!(hero >= 1_000_000, "a player's id");
    // Entering the map: where the hero stands, then the NPCs around.
    game.send(pk::action(hero, action::ENTER_MAP, 0, (0, 0), 0));
    let entered = game.wait_for(pk::ACTION, |p| u32_at(p, 8) == hero);
    assert_eq!(u32_at(&entered, 12), MAP_ID);
    (game, hero)
}

fn u32_at(packet: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(packet[at..at + 4].try_into().unwrap())
}

#[test]
fn conquer_clients_log_in_walk_and_meet() {
    let accounts = std::env::temp_dir().join(format!("c3-conquer-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&accounts);
    let server = server(&accounts);
    let (server, account_addr) = server;

    let (mut game, hero) = enter(account_addr, "tester");
    let npc = game.wait_for(pk::NPC_INFO, |_| true);
    assert_eq!(u32_at(&npc, 4), NPC_ID);

    // A step east (direction 6): the server walks the hero and says so.
    game.send(pk::walk(hero, 6, false));
    let walked = game.wait_for(pk::WALK, |p| u32_at(p, 4) == hero);
    assert_eq!(walked[8] % 8, 6);

    // A second player comes in: each sees the other.
    let (mut other, other_hero) = enter(account_addr, "friend");
    other.wait_for(pk::PLAYER, |p| u32_at(p, 4) == hero);
    game.wait_for(pk::PLAYER, |p| u32_at(p, 4) == other_hero);

    // The client steps on the portal: the server flies its hero to the other map.
    let data = PORTAL.0 as u32 | (PORTAL.1 as u32) << 16;
    game.send(pk::action(hero, action::CHANGE_MAP, data, (0, 0), 0));
    let flown = game.wait_for(pk::ACTION, |p| {
        u32_at(p, 8) == hero && u16::from_le_bytes([p[22], p[23]]) == action::FLY_MAP
    });
    assert_eq!(u32_at(&flown, 12), OTHER_MAP_ID);

    drop((game, other));
    drop(server);
    let _ = std::fs::remove_dir_all(&accounts);
}

/// MsgInteract MAGICATTACK fields scrambled as the client does (the inverse of `pk::spell`).
fn scramble(hero: u32, ts: u32, spell: pk::Spell) -> (u32, (u16, u16), (u16, u16)) {
    let key = hero as u16;
    let kind = spell.kind.wrapping_sub(0x14BE).rotate_right(3) ^ key ^ 0x915D;
    let mut level = ((ts as u16 & 0xFF) ^ 0x37) << 8 | (spell.level ^ 0x21);
    if [1000, 1001, 1002, 1030, 1125, 1150, 1160, 1165].contains(&spell.kind) {
        level ^= 0x6279;
    }
    let target = (spell.target.wrapping_sub(0x8B90_B51A) ^ hero ^ 0x5F2D_2463).rotate_right(19);
    let x = spell.cell.0.wrapping_add(0x22EE).rotate_right(1) ^ key ^ 0x2ED6;
    let y = spell.cell.1.wrapping_sub(0x76DE).rotate_right(5) ^ key ^ 0xB99B;
    (target, (x, y), (kind, level))
}

#[test]
fn spells_unscramble() {
    let hero = 1_000_123;
    for spell in [
        pk::Spell {
            kind: 1000,
            level: 3,
            target: 400_017,
            cell: (312, 288),
        },
        pk::Spell {
            kind: 1045,
            level: 0,
            target: 0,
            cell: (55, 701),
        },
    ] {
        let ts = 0x1234_56AB;
        let (target, cell, data) = scramble(hero, ts, spell);
        assert_eq!(pk::spell(hero, ts, target, cell, data), Some(spell));
        // A forged level word fails the time-stamp check.
        assert_eq!(pk::spell(hero, ts + 1, target, cell, data), None);
    }
}

#[test]
fn a_spell_shows_its_hits() {
    let packet = pk::magic_effect(7, 9, 1000, 2, &[(9, 120), (11, 80)]);
    assert_eq!(packet_type(&packet).ok(), Some(pk::MAGIC_EFFECT));
    assert_eq!(packet.len(), 20 + 2 * 12);
    assert_eq!(u32_at(&packet, 4), 7);
    assert_eq!(packet[16], 2);
    assert_eq!(u32_at(&packet, 20), 9);
    assert_eq!(u32_at(&packet, 24), 120);
}

#[test]
fn guild_info_is_the_clients_size() {
    let packet = pk::guild_info(&pk::GuildInfo {
        id: 12,
        fund: 5000,
        members: 3,
        rank: 100,
        leader: "Lead",
    });
    assert_eq!(packet.len(), 40);
    assert_eq!(u32_at(&packet, 4), 12);
    assert_eq!(packet[20], 100);
}
