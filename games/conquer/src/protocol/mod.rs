//! Conquer 5065's own protocol, so the original client plays on the engine's server: an
//! account server (login) and a game server front that shows the client the engine's world
//! in Conquer's packets. The layouts are in docs/conquer5065_protocol.md.

mod client;
mod crypto;
mod front;
pub(crate) mod packets;
mod wire;

pub use front::{ACCOUNT_PORT, ConquerFront, ConquerSetup, GAME_PORT};

/// The name Project Settings give the front for the original 5065 client.
pub const FRONT_KIND: &str = "conquer-5065";

#[cfg(test)]
mod tests;
