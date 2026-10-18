//! What clients and the server say to each other.

use crate::accounts::Enter;
use c3_game::{EntityId, Input, PlayerLook, WorldUpdate};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Version of these messages; a client and server must agree.
pub const PROTOCOL: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMsg {
    /// Asks for an account's characters (a new account name makes the account).
    Characters {
        protocol: u32,
        account: String,
        password: String,
    },
    /// Logs in playing the character `enter` names (a new account name makes the account).
    Login {
        protocol: u32,
        account: String,
        password: String,
        enter: Enter,
    },
    /// What the player asks for.
    Input(Input),
    Logout,
}

/// What a client learns when it is let in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Welcome {
    /// The player's character in the world.
    pub hero: EntityId,
    /// The map it is on (the key the game loads maps by).
    pub map: String,
    /// The game's ids for that map.
    pub map_ids: Vec<u32>,
}

/// A character as the character list shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterSummary {
    pub name: String,
    pub level: u32,
    pub look: PlayerLook,
    /// The map it is on (empty: the first).
    pub map: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServerMsg {
    /// An account's characters (the answer to [`ClientMsg::Characters`]; the connection
    /// stays open for the login).
    Characters(Vec<CharacterSummary>),
    Welcome(Welcome),
    /// The login failed, and why.
    Refused(String),
    Update(WorldUpdate),
    /// The server is closing the connection, and why.
    Kicked(String),
}

/// `msg` as bytes.
pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    postcard::to_allocvec(msg).expect("messages always serialize")
}

/// A message from bytes, or why it couldn't be read.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    postcard::from_bytes(bytes).map_err(|e| e.to_string())
}
