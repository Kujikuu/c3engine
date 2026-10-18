//! c3-net: networking for the engine's server and clients. A [`Server`] accepts connections
//! and a [`Client`] makes one; both run their sockets on a background thread (tokio) and
//! give the game a plain, non-blocking face: `poll()` for what arrived, `send()` to answer,
//! called from the game's own loop.
//!
//! What crosses the wire is shaped by two pluggable parts, so one transport serves the
//! engine's own protocol and a game's (Conquer's): a [`Framing`] says where each packet ends
//! in the byte stream, and a [`Cipher`] scrambles the stream (it can re-key itself from the
//! packets it lets through, as TQ's game server does after login). [`packet`] reads and
//! writes the fixed binary layouts such protocols use.

mod cipher;
mod framing;
pub mod packet;
mod transport;

pub use cipher::{Cipher, Plain};
pub use framing::{FrameError, Framing, LengthPrefixed, MAX_FRAME};
pub use transport::{Client, ClientEvent, ConnId, NetEvent, Server, Wire};

#[cfg(test)]
mod tests;
