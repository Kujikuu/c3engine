//! Fixed binary packet layouts: little-endian numbers, fixed-width and length-prefixed
//! strings, and the common `[length: u16][type: u16][body]` header (TQ's games use it).

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketError {
    /// The packet ended before a field (`needed` bytes at `at`).
    Short { at: usize, needed: usize },
    /// The packet is not of the type being read.
    WrongType { expected: u16, found: u16 },
    /// A field holds something it can't.
    Invalid(String),
}

impl fmt::Display for PacketError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Short { at, needed } => {
                write!(f, "the packet ends before {needed} bytes at {at}")
            }
            Self::WrongType { expected, found } => {
                write!(f, "expected packet type {expected}, found {found}")
            }
            Self::Invalid(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for PacketError {}

pub type Result<T> = std::result::Result<T, PacketError>;

/// Builds a packet's bytes.
#[derive(Debug, Default, Clone)]
pub struct Writer {
    buf: Vec<u8>,
}

macro_rules! put {
    ($($name:ident: $t:ty),*) => {$(
        pub fn $name(&mut self, v: $t) -> &mut Self {
            self.buf.extend_from_slice(&v.to_le_bytes());
            self
        }
    )*};
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    put!(u8: u8, u16: u16, u32: u32, u64: u64, i8: i8, i16: i16, i32: i32, i64: i64);

    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(v);
        self
    }

    /// `n` zero bytes.
    pub fn zeros(&mut self, n: usize) -> &mut Self {
        self.buf.resize(self.buf.len() + n, 0);
        self
    }

    /// `text` in exactly `width` bytes: cut, or padded with zeros.
    pub fn fixed_str(&mut self, text: &str, width: usize) -> &mut Self {
        let b = text.as_bytes();
        let n = b.len().min(width);
        self.buf.extend_from_slice(&b[..n]);
        self.zeros(width - n)
    }

    /// `text` after its length in one byte (cut at 255 bytes).
    pub fn short_str(&mut self, text: &str) -> &mut Self {
        let b = &text.as_bytes()[..text.len().min(255)];
        self.u8(b.len() as u8).bytes(b)
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

/// Reads a packet's fields in order.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

macro_rules! get {
    ($($name:ident: $t:ty),*) => {$(
        pub fn $name(&mut self) -> Result<$t> {
            const N: usize = std::mem::size_of::<$t>();
            Ok(<$t>::from_le_bytes(self.take(N)?.try_into().expect("N bytes")))
        }
    )*};
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    get!(u8: u8, u16: u16, u32: u32, u64: u64, i8: i8, i16: i16, i32: i32, i64: i64);

    /// The next `n` bytes.
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.buf.len());
        let Some(end) = end else {
            return Err(PacketError::Short {
                at: self.pos,
                needed: n,
            });
        };
        let out = &self.buf[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }

    /// A string in `width` bytes, up to its first zero.
    pub fn fixed_str(&mut self, width: usize) -> Result<String> {
        let b = self.take(width)?;
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        Ok(String::from_utf8_lossy(&b[..end]).into_owned())
    }

    /// A string after its length in one byte.
    pub fn short_str(&mut self) -> Result<String> {
        let n = self.u8()? as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }
}

/// A packet with a `[length: u16][type: u16]` header.
pub trait Packet: Sized {
    /// Its packet type.
    const TYPE: u16;
    /// Writes the body (after the header).
    fn write(&self, w: &mut Writer);
    /// Reads the body (after the header).
    fn read(r: &mut Reader) -> Result<Self>;

    /// The whole packet: header and body.
    fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u16(0).u16(Self::TYPE);
        self.write(&mut w);
        let mut bytes = w.into_bytes();
        let len = bytes.len() as u16;
        bytes[..2].copy_from_slice(&len.to_le_bytes());
        bytes
    }

    /// Reads a whole packet of this type.
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let found = packet_type(bytes)?;
        if found != Self::TYPE {
            return Err(PacketError::WrongType {
                expected: Self::TYPE,
                found,
            });
        }
        Self::read(&mut Reader::new(&bytes[4..]))
    }
}

/// The type of a packet with a `[length][type]` header.
pub fn packet_type(bytes: &[u8]) -> Result<u16> {
    let mut r = Reader::new(bytes);
    r.u16()?;
    r.u16()
}
