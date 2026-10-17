//! PUL: puzzle ground layout. A grid of tile indices; tile `n` is the ANI section
//! `Puzzle<n>` of `ani_file`. `0xFFFF` = empty. Each tile is `GRID_SIZE` px square.
//! Port of `CPuzzleBmp::Load` / `Save` (3dgamemap/PuzzleBmp.cpp).
//!
//! ```text
//! header char[8] ("PUZZLE" v1 | "PUZZLE2" v2) · ani_file char[256]
//! width u32 · height u32 · width*height × u16
//! v2 only: roll_speed_x i32 · roll_speed_y i32
//! ```

use crate::io::{Error, FixedStr, Reader, Result, Writer};

pub const EMPTY: u16 = 0xFFFF;
pub const GRID_SIZE: u32 = 256;

#[derive(Debug, Clone, PartialEq)]
pub struct Puzzle {
    pub header: FixedStr<8>,
    pub ani_file: FixedStr<256>,
    pub width: u32,
    pub height: u32,
    /// Row-major tile indices.
    pub tiles: Vec<u16>,
    /// Scroll speed (v2 only; used by sky/background layers).
    pub roll_speed: Option<(i32, i32)>,
}

impl Puzzle {
    pub fn new(width: u32, height: u32, ani_file: &str) -> Self {
        Self {
            header: FixedStr::new("PUZZLE2"),
            ani_file: FixedStr::new(ani_file),
            width,
            height,
            tiles: vec![EMPTY; (width * height) as usize],
            roll_speed: Some((0, 0)),
        }
    }

    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        Self::parse(&std::fs::read(path)?)
    }

    pub fn version(&self) -> u32 {
        if self.roll_speed.is_some() { 2 } else { 1 }
    }

    pub fn tile(&self, x: u32, y: u32) -> u16 {
        if x < self.width && y < self.height { self.tiles[(y * self.width + x) as usize] } else { EMPTY }
    }

    pub fn set_tile(&mut self, x: u32, y: u32, index: u16) {
        if x < self.width && y < self.height {
            self.tiles[(y * self.width + x) as usize] = index;
        }
    }

    /// ANI section title for a tile index.
    pub fn tile_title(index: u16) -> String {
        format!("Puzzle{index}")
    }

    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let header: FixedStr<8> = r.fixed()?;
        let v2 = match header.as_str().as_str() {
            "PUZZLE" => false,
            "PUZZLE2" => true,
            h => return Err(Error::Invalid(format!("bad puzzle header {h:?}"))),
        };
        let ani_file = r.fixed()?;
        let width = r.u32()?;
        let height = r.u32()?;
        let n = width as usize * height as usize;
        if n * 2 > r.remaining() {
            return Err(Error::Invalid(format!("puzzle size {width}x{height} exceeds file")));
        }
        let tiles = (0..n).map(|_| r.u16()).collect::<Result<_>>()?;
        let roll_speed = if v2 { Some((r.i32()?, r.i32()?)) } else { None };
        if r.remaining() != 0 {
            return Err(Error::Invalid(format!("{} trailing bytes", r.remaining())));
        }
        Ok(Self { header, ani_file, width, height, tiles, roll_speed })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.fixed(&self.header);
        w.fixed(&self.ani_file);
        w.u32(self.width);
        w.u32(self.height);
        self.tiles.iter().for_each(|&t| w.u16(t));
        if let Some((x, y)) = self.roll_speed {
            w.i32(x);
            w.i32(y);
        }
        w.buf
    }
}
