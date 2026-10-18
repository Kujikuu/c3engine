//! How Conquer's bytes are framed: `[size: u16][type: u16]…` on the account server; on the
//! game server the same, followed by an 8-byte seal ("TQClient" in, "TQServer" out), after a
//! key exchange whose two packets are framed by their own length fields.

use super::crypto::{CLIENT_PAD, GameCipher, SEAL, TqCipher};
use c3_net::{FrameError, Framing, Wire};
use std::sync::atomic::{AtomicBool, Ordering};

/// Largest packet the client handles.
const MAX_PACKET: usize = 1024;
/// Largest key reply accepted.
const MAX_KEY_REPLY: usize = 4096;

fn packet_size(header: &[u8]) -> Result<usize, FrameError> {
    let size = u16::from_le_bytes([header[0], header[1]]) as usize;
    match size {
        0..4 => Err(FrameError::TooSmall(size)),
        s if s > MAX_PACKET => Err(FrameError::TooBig(s)),
        s => Ok(s),
    }
}

/// The account server's framing: packets carry their size.
struct AccountFraming;

impl Framing for AccountFraming {
    fn header_len(&self) -> usize {
        2
    }

    fn frame_len(&self, header: &[u8]) -> Result<usize, FrameError> {
        packet_size(header)
    }
}

/// The game server's framing: the client's key reply first, then sealed packets.
#[derive(Default)]
struct GameFraming {
    exchanged: AtomicBool,
}

impl Framing for GameFraming {
    fn header_len(&self) -> usize {
        if self.exchanged.load(Ordering::Relaxed) {
            2
        } else {
            CLIENT_PAD + 4
        }
    }

    fn frame_len(&self, header: &[u8]) -> Result<usize, FrameError> {
        if self.exchanged.load(Ordering::Relaxed) {
            return Ok(packet_size(header)? + SEAL);
        }
        let body = i32::from_le_bytes(
            header[CLIENT_PAD..CLIENT_PAD + 4]
                .try_into()
                .expect("4 bytes"),
        );
        match usize::try_from(body) {
            Ok(n) if (12..=MAX_KEY_REPLY).contains(&n) => Ok(CLIENT_PAD + n),
            Ok(n) if n > MAX_KEY_REPLY => Err(FrameError::TooBig(n)),
            _ => Err(FrameError::TooSmall(body.max(0) as usize)),
        }
    }

    fn seal(&self, mut packet: Vec<u8>) -> Vec<u8> {
        packet.extend_from_slice(b"TQServer");
        packet
    }

    fn open(&self, mut frame: Vec<u8>) -> Vec<u8> {
        if !self.exchanged.swap(true, Ordering::Relaxed) {
            // The key reply (its cipher re-keyed on it already): from now on, packets.
            return frame;
        }
        frame.truncate(frame.len() - SEAL);
        frame
    }
}

/// A connection to the account server.
pub fn account_wire() -> Wire {
    Wire {
        framing: Box::new(AccountFraming),
        cipher: Box::new(TqCipher::new()),
        greeting: None,
    }
}

/// A connection to the game server: it greets with the key packet.
pub fn game_wire() -> Wire {
    let (cipher, key_packet) = GameCipher::new();
    Wire {
        framing: Box::new(GameFraming::default()),
        cipher: Box::new(cipher),
        greeting: Some(key_packet),
    }
}
