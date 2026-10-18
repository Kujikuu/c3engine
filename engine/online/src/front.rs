//! How clients reach a game server. A [`Front`] owns its sockets and speaks one protocol: it
//! turns what its clients send into [`ClientMsg`]s and shows them the world in its own
//! words. The engine's protocol is [`EngineFront`]; a game can add its own (Conquer's, for
//! its original client), and one server serves them all at once.

use crate::msg::{ClientMsg, PROTOCOL, ServerMsg, Welcome, decode, encode};
use crate::server::MapWorld;
use c3_game::{EntityId, WorldUpdate};
use c3_net::{ConnId, NetEvent, Server, Wire};
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};

/// What a front's clients did.
#[derive(Debug, Clone, PartialEq)]
pub enum FrontEvent {
    Message(ConnId, ClientMsg),
    /// The connection is gone.
    Closed(ConnId),
    /// Something for the server's log (a client sent garbage).
    Problem(String),
}

/// One protocol's clients.
pub trait Front {
    /// The address its clients connect to.
    fn local_addr(&self) -> SocketAddr;
    /// What its clients did since the last call.
    fn poll(&mut self) -> Vec<FrontEvent>;
    /// Lets `conn` in as `welcome.hero` on `world`.
    fn welcome(&mut self, conn: ConnId, welcome: &Welcome, world: &MapWorld);
    /// Shows `conn` (playing `hero` on `world`) what changed around it.
    fn update(&mut self, conn: ConnId, update: &WorldUpdate, world: &MapWorld, hero: EntityId);
    /// Tells `conn` its login failed, and closes it.
    fn refuse(&mut self, conn: ConnId, why: &str);
    /// Tells `conn` why it is being disconnected, and closes it.
    fn kick(&mut self, conn: ConnId, why: &str);
}

/// The engine's own protocol (postcard messages, see `msg`).
pub struct EngineFront {
    net: Server,
}

impl EngineFront {
    pub fn new(net: Server) -> Self {
        Self { net }
    }

    /// Listens on `addr` with the engine's wire.
    pub fn bind(addr: impl ToSocketAddrs) -> io::Result<Self> {
        Ok(Self::new(Server::bind(addr, Wire::plain)?))
    }
}

impl Front for EngineFront {
    fn local_addr(&self) -> SocketAddr {
        self.net.local_addr()
    }

    fn poll(&mut self) -> Vec<FrontEvent> {
        let mut out = Vec::new();
        for event in self.net.poll() {
            match event {
                NetEvent::Connected { .. } => {}
                NetEvent::Closed { id, .. } => out.push(FrontEvent::Closed(id)),
                NetEvent::Received { id, packet } => match decode::<ClientMsg>(&packet) {
                    Ok(ClientMsg::Login { protocol, .. }) if protocol != PROTOCOL => {
                        let why = format!(
                            "This server speaks protocol {PROTOCOL}; the client speaks {protocol}."
                        );
                        self.refuse(id, &why);
                    }
                    Ok(msg) => out.push(FrontEvent::Message(id, msg)),
                    Err(e) => {
                        out.push(FrontEvent::Problem(format!(
                            "Unreadable message from {id:?}: {e}"
                        )));
                        self.net.close(id);
                    }
                },
            }
        }
        out
    }

    fn welcome(&mut self, conn: ConnId, welcome: &Welcome, _world: &MapWorld) {
        self.net
            .send(conn, encode(&ServerMsg::Welcome(welcome.clone())));
    }

    fn update(&mut self, conn: ConnId, update: &WorldUpdate, _world: &MapWorld, _hero: EntityId) {
        if !update.is_empty() {
            self.net
                .send(conn, encode(&ServerMsg::Update(update.clone())));
        }
    }

    fn refuse(&mut self, conn: ConnId, why: &str) {
        self.net
            .send(conn, encode(&ServerMsg::Refused(why.to_owned())));
        self.net.close(conn);
    }

    fn kick(&mut self, conn: ConnId, why: &str) {
        self.net
            .send(conn, encode(&ServerMsg::Kicked(why.to_owned())));
        self.net.close(conn);
    }
}
