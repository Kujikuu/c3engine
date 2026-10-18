//! c3-online: the engine's own online play. A [`GameServer`] runs the game's maps (one
//! simulation each), lets players log in to their stored characters, carries out what they
//! ask and sends each one what is around them; a [`Connection`] is a client's end, whose
//! updates keep a mirror simulation (`Sim::apply_update`) the client shows as if local.
//!
//! Messages are serde values ([`ClientMsg`], [`ServerMsg`]) in postcard, length-prefixed
//! over `c3-net`. A game's own protocol (Conquer's, for its original client) is separate.

mod accounts;
mod client;
mod msg;
mod server;

pub use accounts::{AccountError, Accounts};
pub use client::{Connection, RemoteEvent};
pub use msg::{ClientMsg, PROTOCOL, ServerMsg, Welcome, decode, encode};
pub use server::{GameServer, MapWorld, NewCharacter, ServerConfig};

#[cfg(test)]
mod tests;
