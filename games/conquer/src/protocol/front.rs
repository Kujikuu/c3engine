//! The Conquer front: an account server that checks logins and hands out tokens, and a
//! game server whose clients play the engine's world through Conquer's packets.

use super::client::{ConquerClient, Ids};
use super::crypto::decrypt_password;
use super::packets::{self as pk, Request, channel};
use super::wire::{account_wire, game_wire};
use c3_game::{EntityId, WorldUpdate};
use c3_net::{ConnId, NetEvent, Server};
use c3_online::{ClientMsg, Front, FrontEvent, MapWorld, PROTOCOL, Welcome};
use std::collections::{BTreeMap, HashMap};
use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// Conquer's usual ports.
pub const ACCOUNT_PORT: u16 = 9958;
pub const GAME_PORT: u16 = 5816;
/// How long a login token waits for its game connection.
const TOKEN_LIFE: Duration = Duration::from_secs(60);
/// What the account server answers a good login with.
const LOGIN_OK: u32 = 2;

/// Where the Conquer servers listen, and the game server's address the client is sent to.
pub struct ConquerSetup {
    pub account_listen: String,
    pub game_listen: String,
    /// The game server's IP as clients reach it.
    pub public_ip: String,
    pub public_port: u16,
}

impl ConquerSetup {
    /// Both servers on the `listen` interface at Conquer's ports; clients sent to `public_ip`.
    pub fn on(listen: &str, public_ip: &str) -> Self {
        Self {
            account_listen: format!("{listen}:{ACCOUNT_PORT}"),
            game_listen: format!("{listen}:{GAME_PORT}"),
            public_ip: public_ip.to_owned(),
            public_port: GAME_PORT,
        }
    }
}

/// A login the account server let through, waiting for its game connection.
struct Pending {
    account: String,
    password: String,
    at: Instant,
}

pub struct ConquerFront {
    account: Server,
    game: Server,
    public_ip: String,
    public_port: u16,
    pending: HashMap<u32, Pending>,
    clients: BTreeMap<ConnId, ConquerClient>,
    ids: Ids,
}

fn token() -> u32 {
    let mut b = [0u8; 4];
    let _ = getrandom::fill(&mut b);
    u32::from_le_bytes(b).max(1)
}

impl ConquerFront {
    /// Starts both servers. A `public_port` of 0 sends clients to the game server's own port.
    pub fn bind(setup: &ConquerSetup) -> io::Result<Self> {
        let game = Server::bind(setup.game_listen.as_str(), game_wire)?;
        let public_port = match setup.public_port {
            0 => game.local_addr().port(),
            port => port,
        };
        Ok(Self {
            account: Server::bind(setup.account_listen.as_str(), account_wire)?,
            game,
            public_ip: setup.public_ip.clone(),
            public_port,
            pending: HashMap::new(),
            clients: BTreeMap::new(),
            ids: Ids::default(),
        })
    }

    /// The account server's address.
    pub fn account_addr(&self) -> SocketAddr {
        self.account.local_addr()
    }

    fn poll_account(&mut self, out: &mut Vec<FrontEvent>) {
        self.pending.retain(|_, p| p.at.elapsed() < TOKEN_LIFE);
        for event in self.account.poll() {
            let NetEvent::Received { id, packet } = event else {
                continue;
            };
            match pk::parse(&packet) {
                Ok(Request::Account { account, password }) => {
                    let token = token();
                    self.pending.insert(
                        token,
                        Pending {
                            account,
                            password: decrypt_password(&password),
                            at: Instant::now(),
                        },
                    );
                    let reply =
                        pk::login_reply(token, LOGIN_OK, &self.public_ip, self.public_port.into());
                    self.account.send(id, reply);
                }
                // The client's goodbye on this socket, before it moves to the game server.
                Ok(_) => {}
                Err(e) => out.push(FrontEvent::Problem(format!("Conquer login: {e}"))),
            }
        }
    }
}

impl Front for ConquerFront {
    fn local_addr(&self) -> SocketAddr {
        self.game.local_addr()
    }

    fn poll(&mut self) -> Vec<FrontEvent> {
        let mut out = Vec::new();
        self.poll_account(&mut out);
        for event in self.game.poll() {
            match event {
                NetEvent::Connected { id, .. } => {
                    self.clients.insert(id, ConquerClient::default());
                }
                NetEvent::Closed { id, .. } => {
                    if self.clients.remove(&id).is_some() {
                        out.push(FrontEvent::Closed(id));
                    }
                }
                NetEvent::Received { id, packet } => match pk::parse(&packet) {
                    Ok(Request::Connect { token }) => match self.pending.remove(&token) {
                        Some(p) => out.push(FrontEvent::Message(
                            id,
                            ClientMsg::Login {
                                protocol: PROTOCOL,
                                account: p.account,
                                password: p.password,
                            },
                        )),
                        None => self.refuse(id, "Your login has expired. Please log in again."),
                    },
                    Ok(request) => {
                        let Some(client) = self.clients.get_mut(&id) else {
                            continue;
                        };
                        let (replies, inputs) = client.request(request, &self.ids);
                        for reply in replies {
                            self.game.send(id, reply);
                        }
                        out.extend(
                            inputs
                                .into_iter()
                                .map(|i| FrontEvent::Message(id, ClientMsg::Input(i))),
                        );
                    }
                    Err(e) => out.push(FrontEvent::Problem(format!("Conquer client {id:?}: {e}"))),
                },
            }
        }
        out
    }

    fn welcome(&mut self, conn: ConnId, welcome: &Welcome, world: &MapWorld) {
        let Some(client) = self.clients.get_mut(&conn) else {
            return;
        };
        match client.welcome(welcome, &world.sim) {
            Ok(packets) => {
                for packet in packets {
                    self.game.send(conn, packet);
                }
            }
            Err(why) => self.refuse(conn, &why),
        }
    }

    fn moved(&mut self, conn: ConnId, welcome: &Welcome, world: &MapWorld) {
        let Some(client) = self.clients.get_mut(&conn) else {
            return;
        };
        match client.moved(welcome, &world.sim) {
            Ok(packets) => {
                for packet in packets {
                    self.game.send(conn, packet);
                }
            }
            Err(why) => self.kick(conn, &why),
        }
    }

    fn update(&mut self, conn: ConnId, update: &WorldUpdate, world: &MapWorld, _hero: EntityId) {
        let Some(client) = self.clients.get_mut(&conn) else {
            return;
        };
        for packet in client.update(update, &world.sim, &mut self.ids) {
            self.game.send(conn, packet);
        }
    }

    fn refuse(&mut self, conn: ConnId, why: &str) {
        self.game.send(
            conn,
            pk::talk(channel::ENTRANCE, "SYSTEM", "ALLUSERS", why, 0xFFFFFF),
        );
        self.game.close(conn);
    }

    fn kick(&mut self, conn: ConnId, why: &str) {
        self.game.send(
            conn,
            pk::talk(channel::SYSTEM, "SYSTEM", "ALLUSERS", why, 0xFFFF00),
        );
        self.game.close(conn);
    }
}
