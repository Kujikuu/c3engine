//! Where packets begin and end in a byte stream.

use std::fmt;

/// The largest packet any framing accepts (bigger ones close the connection).
pub const MAX_FRAME: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// A packet claims to be bigger than allowed.
    TooBig(usize),
    /// A packet claims to be smaller than its own header.
    TooSmall(usize),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::TooBig(n) => write!(f, "a packet of {n} bytes is too big"),
            Self::TooSmall(n) => write!(f, "a packet of {n} bytes is shorter than its header"),
        }
    }
}

impl std::error::Error for FrameError {}

/// Splits a (decrypted) byte stream into packets.
pub trait Framing: Send {
    /// Bytes at the start of a packet that tell its length.
    fn header_len(&self) -> usize;
    /// The whole packet's length (header included), read from its header.
    fn frame_len(&self, header: &[u8]) -> Result<usize, FrameError>;
    /// The bytes to send for `packet` (a framing that adds its own header adds it here).
    fn seal(&self, packet: Vec<u8>) -> Vec<u8> {
        packet
    }
    /// The packet inside a received frame (a framing that added a header drops it here).
    fn open(&self, frame: Vec<u8>) -> Vec<u8> {
        frame
    }
}

/// Each packet follows its length (u32, little-endian): for protocols whose packets don't
/// carry their own (the engine's).
#[derive(Debug, Default, Clone, Copy)]
pub struct LengthPrefixed;

impl Framing for LengthPrefixed {
    fn header_len(&self) -> usize {
        4
    }

    fn frame_len(&self, header: &[u8]) -> Result<usize, FrameError> {
        let len = u32::from_le_bytes(header[..4].try_into().expect("4 header bytes")) as usize;
        if len > MAX_FRAME {
            return Err(FrameError::TooBig(len));
        }
        Ok(len + 4)
    }

    fn seal(&self, packet: Vec<u8>) -> Vec<u8> {
        let mut out = Vec::with_capacity(packet.len() + 4);
        out.extend_from_slice(&(packet.len() as u32).to_le_bytes());
        out.extend(packet);
        out
    }

    fn open(&self, mut frame: Vec<u8>) -> Vec<u8> {
        frame.drain(..4);
        frame
    }
}
