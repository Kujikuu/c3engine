//! c3-assets: game-agnostic asset access and decoded asset types.
//!
//! Games and format plugins provide an [`AssetSource`] (loose folders, packed archives, …).
//! Engine code reads assets only through it, by forward-slash relative path.

use std::borrow::Cow;

/// Read-only virtual file system.
pub trait AssetSource {
    /// Returns the bytes at `path`, or `None` if it doesn't exist.
    fn read(&self, path: &str) -> Option<Cow<'_, [u8]>>;
}

/// Decoded image in straight (non-premultiplied) RGBA8, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        debug_assert_eq!(rgba.len(), (width * height * 4) as usize);
        Self {
            width,
            height,
            rgba,
        }
    }
}
