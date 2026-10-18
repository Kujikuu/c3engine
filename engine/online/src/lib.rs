//! c3-online: the engine's own online play. A [`GameServer`] runs the game's maps (one
//! simulation each), lets players log in to their stored characters, carries out what they
//! ask and sends each one what is around them; a [`Connection`] is a client's end, whose
//! updates keep a mirror simulation (`Sim::apply_update`) the client shows as if local.
//!
//! Clients come in through [`Front`]s, one per protocol: the engine's own ([`EngineFront`]:
//! serde values, [`ClientMsg`] and [`ServerMsg`], in postcard over `c3-net`) and any a game
//! adds (Conquer's, for its original client).

mod accounts;
mod client;
mod front;
mod msg;
mod server;

pub use accounts::{AccountError, Accounts, Enter};
pub use client::{Connection, RemoteEvent};
pub use front::{EngineFront, Front, FrontEvent};
pub use msg::{CharacterSummary, ClientMsg, PROTOCOL, ServerMsg, Welcome, decode, encode};
pub use server::{GameServer, MapKey, MapLoader, MapWorld, NewCharacter, ServerConfig};

#[cfg(test)]
mod tests;
