//! Little-endian binary reader/writer helpers shared by all TQ formats.

use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unexpected end of data at offset {0}")]
    Eof(usize),
    #[error("invalid data: {0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len());
        let end = end.ok_or(Error::Eof(self.pos))?;
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.bytes(N)?.try_into().unwrap())
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }
    pub fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.array()?))
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }
    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.array()?))
    }
    pub fn fixed<const N: usize>(&mut self) -> Result<FixedStr<N>> {
        Ok(FixedStr(self.array()?))
    }
}

#[derive(Default)]
pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
    pub fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn i16(&mut self, v: i16) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn fixed<const N: usize>(&mut self, s: &FixedStr<N>) {
        self.bytes(&s.0);
    }
}

/// Fixed-size C string buffer. Keeps the raw bytes (including garbage after the
/// terminator) so files round-trip byte-identically.
#[derive(Clone, PartialEq, Eq)]
pub struct FixedStr<const N: usize>(pub [u8; N]);

impl<const N: usize> FixedStr<N> {
    pub fn new(s: &str) -> Self {
        let mut b = [0u8; N];
        let n = s.len().min(N - 1);
        b[..n].copy_from_slice(&s.as_bytes()[..n]);
        Self(b)
    }

    /// Text up to the first NUL. TQ strings are GBK; non-UTF-8 bytes are replaced.
    pub fn as_str(&self) -> String {
        let end = self.0.iter().position(|&c| c == 0).unwrap_or(N);
        String::from_utf8_lossy(&self.0[..end]).into_owned()
    }
}

impl<const N: usize> Default for FixedStr<N> {
    fn default() -> Self {
        Self([0; N])
    }
}

impl<const N: usize> fmt::Debug for FixedStr<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}
