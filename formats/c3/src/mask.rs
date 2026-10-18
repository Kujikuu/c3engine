//! `.MSK`: 1-bit hit-test mask for a same-named DDS/TGA image. Port of `CAlphaMask`
//! (3DBaseCode/AlphaMask.cpp). No header: dimensions come from the paired image.
//! Bit `i = x + y·width` lives in byte `i / 8`, bit `i % 8` (LSB first); set = opaque.

use crate::io::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlphaMask {
    pub width: u32,
    pub height: u32,
    /// Raw bits as stored (kept whole so files round-trip).
    pub bits: Vec<u8>,
}

impl AlphaMask {
    /// Bytes the client reads for an image of this size.
    pub fn byte_len(width: u32, height: u32) -> usize {
        (width as usize * height as usize) / 8 + 1
    }

    /// Wraps mask bytes for an image of `width`×`height`. Short files are accepted
    /// (see [`AlphaMask::is_truncated`]); missing bits read as transparent. The client
    /// would read past its buffer for those.
    pub fn parse(data: &[u8], width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(Error::Invalid(format!(
                "mask for empty image {width}x{height}"
            )));
        }
        Ok(Self {
            width,
            height,
            bits: data.to_vec(),
        })
    }

    /// True if the file holds fewer bits than the image has pixels.
    pub fn is_truncated(&self) -> bool {
        self.bits.len() < (self.width as usize * self.height as usize).div_ceil(8)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.bits.clone()
    }

    /// True if the pixel is opaque (hit). Outside the image is transparent.
    pub fn is_opaque(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return false;
        }
        let i = x as usize + y as usize * self.width as usize;
        self.bits
            .get(i / 8)
            .is_some_and(|b| b & (1 << (i % 8)) != 0)
    }
}
