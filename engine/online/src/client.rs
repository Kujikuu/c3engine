//! A client's connection to a game server.

use crate::accounts::Enter;
use crate::msg::{CharacterSummary, ClientMsg, PROTOCOL, ServerMsg, Welcome, decode, encode};
use c3_game::{Input, WorldUpdate};
use c3_net::{Client, ClientEvent, Wire};
use std::io;
use std::net::ToSocketAddrs;
use std::time::{Duration, Instant};

/// What the server said.
#[derive(Debug, Clone, PartialEq)]
pub enum RemoteEvent {
    /// The account's characters, to choose from.
    Characters(Vec<CharacterSummary>),
    Welcome(Welcome),
    Update(WorldUpdate),
    /// The login failed.
    Refused(String),
    /// The connection ended (why, when known).
    Closed(Option<String>),
}

pub struct Connection {
    client: Client,
}

impl Connection {
    /// Connects to `addr` (then ask for characters, or log in).
    pub fn open(addr: impl ToSocketAddrs) -> io::Result<Self> {
        Ok(Self {
            client: Client::connect(addr, Wire::plain())?,
        })
    }

    /// Connects to `addr` and logs in to `account`, playing its first character.
    pub fn connect(addr: impl ToSocketAddrs, account: &str, password: &str) -> io::Result<Self> {
        let conn = Self::open(addr)?;
        conn.login(account, password, Enter::First);
        Ok(conn)
    }

    /// Asks for `account`'s characters (answered by [`RemoteEvent::Characters`]).
    pub fn ask_characters(&self, account: &str, password: &str) {
        self.client.send(encode(&ClientMsg::Characters {
            protocol: PROTOCOL,
            account: account.to_owned(),
            password: password.to_owned(),
        }));
    }

    /// Logs in to `account` playing the character `enter` names.
    pub fn login(&self, account: &str, password: &str, enter: Enter) {
        self.client.send(encode(&ClientMsg::Login {
            protocol: PROTOCOL,
            account: account.to_owned(),
            password: password.to_owned(),
            enter,
        }));
    }

    /// Connects, logs in and waits up to `timeout` to be let in; the welcome and any updates
    /// that came with it.
    pub fn join(
        addr: impl ToSocketAddrs,
        account: &str,
        password: &str,
        timeout: Duration,
    ) -> Result<(Self, Welcome, Vec<WorldUpdate>), String> {
        let mut conn = Self::connect(addr, account, password)
            .map_err(|e| format!("Can't reach the server: {e}"))?;
        let end = Instant::now() + timeout;
        let mut welcome = None;
        let mut updates = Vec::new();
        while Instant::now() < end {
            for event in conn.poll() {
                match event {
                    RemoteEvent::Welcome(w) => welcome = Some(w),
                    RemoteEvent::Update(u) => updates.push(u),
                    RemoteEvent::Refused(why) => return Err(why),
                    RemoteEvent::Characters(_) => {}
                    RemoteEvent::Closed(why) => {
                        return Err(
                            why.unwrap_or_else(|| "The server closed the connection.".into())
                        );
                    }
                }
            }
            // The first update (the hero and what is around) follows the welcome.
            if let Some(w) = &welcome
                && !updates.is_empty()
            {
                return Ok((conn, w.clone(), updates));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Err("The server didn't answer in time.".into())
    }

    /// What the server said since the last call.
    pub fn poll(&mut self) -> Vec<RemoteEvent> {
        self.client
            .poll()
            .into_iter()
            .map(|event| match event {
                ClientEvent::Received(bytes) => match decode::<ServerMsg>(&bytes) {
                    Ok(ServerMsg::Characters(list)) => RemoteEvent::Characters(list),
                    Ok(ServerMsg::Welcome(w)) => RemoteEvent::Welcome(w),
                    Ok(ServerMsg::Update(u)) => RemoteEvent::Update(u),
                    Ok(ServerMsg::Refused(why)) => RemoteEvent::Refused(why),
                    Ok(ServerMsg::Kicked(why)) => RemoteEvent::Closed(Some(why)),
                    Err(e) => RemoteEvent::Closed(Some(format!(
                        "Unreadable message from the server: {e}"
                    ))),
                },
                ClientEvent::Closed(why) => RemoteEvent::Closed(why),
            })
            .collect()
    }

    /// Asks the server for `input`.
    pub fn send(&self, input: Input) {
        self.client.send(encode(&ClientMsg::Input(input)));
    }

    pub fn is_open(&self) -> bool {
        self.client.is_open()
    }

    /// Logs out (the server saves the character).
    pub fn logout(&self) {
        self.client.send(encode(&ClientMsg::Logout));
    }
}
