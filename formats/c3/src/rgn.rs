//! `.rgn`: window shape as rectangles — a `u32` payload size followed by Win32
//! `GetRegionData` bytes (`RGNDATAHEADER` + `RECT`s). Port of `LoadPlayRegion`
//! (Game/GameDlg.cpp), with the same validation.
//!
//! ```text
//! payload u32 · { header_size u32 = 32, type u32 = 1, count u32, rect_bytes u32, bound i32×4 } · count × i32×4
//! ```

use crate::io::{Error, Reader, Result, Writer};

/// left, top, right, bottom (right/bottom exclusive).
pub type Rect = [i32; 4];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub bound: Rect,
    pub rects: Vec<Rect>,
}

const HEADER: u32 = 32;
const RECTANGLES: u32 = 1;

fn rect(r: &mut Reader) -> Result<Rect> {
    Ok([r.i32()?, r.i32()?, r.i32()?, r.i32()?])
}

impl Region {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let payload = r.u32()? as usize;
        let (size, kind, count, rect_bytes) =
            (r.u32()?, r.u32()?, r.u32()? as usize, r.u32()? as usize);
        let bound = rect(&mut r)?;
        if size != HEADER
            || kind != RECTANGLES
            || rect_bytes != count * 16
            || payload != 32 + rect_bytes
            || r.remaining() != rect_bytes
        {
            return Err(Error::Invalid("not a rectangle region".into()));
        }
        let rects = (0..count).map(|_| rect(&mut r)).collect::<Result<_>>()?;
        Ok(Self { bound, rects })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        let rect_bytes = self.rects.len() as u32 * 16;
        w.u32(HEADER + rect_bytes);
        w.u32(HEADER);
        w.u32(RECTANGLES);
        w.u32(self.rects.len() as u32);
        w.u32(rect_bytes);
        for v in std::iter::once(&self.bound).chain(&self.rects).flatten() {
            w.i32(*v);
        }
        w.buf
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.rects
            .iter()
            .any(|&[l, t, r, b]| x >= l && x < r && y >= t && y < b)
    }
}
