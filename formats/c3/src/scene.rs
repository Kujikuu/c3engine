//! Terrain objects: multi-part map art with its own walkability footprint.
//!
//! - `.scene` (binary): what DMap `Terrain` objects reference. Port of
//!   `C2DMapTerrainObj::Load` / `C2DMapTerrainObjPart::Load` (3dgamemap).
//! - `.Part` (text): the editor's source for one part (`AniFile=…`, `Cell[x,y]={mask,terrain,alt}`).
//!
//! ```text
//! .scene: parts i32 · parts × {
//!   ani_file char[256] · ani_title char[64] · offset i32×2 · frame_interval u32
//!   base u32×2 (w, h) · thick i32 · scene_offset i32×2 · height i32
//!   h × w × { mask u32, terrain i32, altitude i32 }   (row-major)
//! }
//! ```

use crate::io::{Error, FixedStr, Reader, Result, Writer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PartCell {
    pub mask: u32,
    pub terrain: i32,
    pub altitude: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScenePart {
    pub ani_file: FixedStr<256>,
    pub ani_title: FixedStr<64>,
    /// Sprite offset in pixels.
    pub offset: (i32, i32),
    pub frame_interval: u32,
    /// Footprint in cells.
    pub width: u32,
    pub height: u32,
    pub thick: i32,
    /// Cell offset of this part from the object's anchor.
    pub scene_offset: (i32, i32),
    /// Draw height (sort bias).
    pub draw_height: i32,
    /// Row-major footprint cells.
    pub cells: Vec<PartCell>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TerrainObject {
    pub parts: Vec<ScenePart>,
}

impl TerrainObject {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let n = r.i32()?;
        let mut parts = Vec::new();
        for _ in 0..n.max(0) {
            let ani_file = r.fixed()?;
            let ani_title = r.fixed()?;
            let offset = (r.i32()?, r.i32()?);
            let frame_interval = r.u32()?;
            let (width, height) = (r.u32()?, r.u32()?);
            let thick = r.i32()?;
            let scene_offset = (r.i32()?, r.i32()?);
            let draw_height = r.i32()?;
            let count = width as usize * height as usize;
            if count * 12 > r.remaining() {
                return Err(Error::Invalid(format!(
                    "part footprint {width}x{height} exceeds file"
                )));
            }
            let cells = (0..count)
                .map(|_| {
                    Ok(PartCell {
                        mask: r.u32()?,
                        terrain: r.i32()?,
                        altitude: r.i32()?,
                    })
                })
                .collect::<Result<_>>()?;
            parts.push(ScenePart {
                ani_file,
                ani_title,
                offset,
                frame_interval,
                width,
                height,
                thick,
                scene_offset,
                draw_height,
                cells,
            });
        }
        if r.remaining() != 0 {
            return Err(Error::Invalid(format!("{} trailing bytes", r.remaining())));
        }
        Ok(Self { parts })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.i32(self.parts.len() as i32);
        for p in &self.parts {
            w.fixed(&p.ani_file);
            w.fixed(&p.ani_title);
            w.i32(p.offset.0);
            w.i32(p.offset.1);
            w.u32(p.frame_interval);
            w.u32(p.width);
            w.u32(p.height);
            w.i32(p.thick);
            w.i32(p.scene_offset.0);
            w.i32(p.scene_offset.1);
            w.i32(p.draw_height);
            for c in &p.cells {
                w.u32(c.mask);
                w.i32(c.terrain);
                w.i32(c.altitude);
            }
        }
        w.buf
    }
}

impl ScenePart {
    /// Parses an editor `.Part` text file. Scene offset and draw height are not stored
    /// there; they default to zero.
    pub fn parse_text(data: &[u8]) -> Result<Self> {
        let text = encoding_rs::GBK.decode_without_bom_handling(data).0;
        let mut part = ScenePart {
            ani_file: FixedStr::default(),
            ani_title: FixedStr::default(),
            offset: (0, 0),
            frame_interval: 0,
            width: 0,
            height: 0,
            thick: 0,
            scene_offset: (0, 0),
            draw_height: 0,
            cells: Vec::new(),
        };
        let num = |v: &str, k: &str| {
            v.trim()
                .parse::<i64>()
                .map_err(|_| Error::Invalid(format!("bad {k}: {v:?}")))
        };
        let mut cells = Vec::new();
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key {
                "AniFile" => part.ani_file = FixedStr::from_gbk(value),
                "AniTitle" => part.ani_title = FixedStr::from_gbk(value),
                "OffsetX" => part.offset.0 = num(value, key)? as i32,
                "OffsetY" => part.offset.1 = num(value, key)? as i32,
                "AniInterval" => part.frame_interval = num(value, key)? as u32,
                "Width" => part.width = num(value, key)? as u32,
                "Height" => part.height = num(value, key)? as u32,
                "Thick" => part.thick = num(value, key)? as i32,
                k if k.starts_with("Cell[") => {
                    let pos = k.trim_start_matches("Cell[").trim_end_matches(']');
                    let (x, y) = pos
                        .split_once(',')
                        .ok_or_else(|| Error::Invalid(format!("bad cell key {k}")))?;
                    let v: Vec<&str> = value
                        .trim_matches(|c| c == '{' || c == '}')
                        .split(',')
                        .collect();
                    let [mask, terrain, alt] = v[..] else {
                        return Err(Error::Invalid(format!("bad cell {value}")));
                    };
                    cells.push((
                        (num(x, k)? as u32, num(y, k)? as u32),
                        PartCell {
                            mask: num(mask, k)? as u32,
                            terrain: num(terrain, k)? as i32,
                            altitude: num(alt, k)? as i32,
                        },
                    ));
                }
                _ => {}
            }
        }
        part.cells = vec![PartCell::default(); part.width as usize * part.height as usize];
        for ((x, y), c) in cells {
            if x >= part.width || y >= part.height {
                return Err(Error::Invalid(format!(
                    "cell [{x},{y}] outside {}x{}",
                    part.width, part.height
                )));
            }
            part.cells[(y * part.width + x) as usize] = c;
        }
        Ok(part)
    }

    /// Writes the editor `.Part` text form (CRLF, GBK).
    pub fn to_text(&self) -> Vec<u8> {
        let mut s = format!(
            "AniFile={}\r\nAniTitle={}\r\nOffsetX={}\r\nOffsetY={}\r\nAniInterval={}\r\nWidth={}\r\nHeight={}\r\nThick={}\r\n",
            self.ani_file.to_gbk_string(),
            self.ani_title.to_gbk_string(),
            self.offset.0,
            self.offset.1,
            self.frame_interval,
            self.width,
            self.height,
            self.thick
        );
        for y in 0..self.height {
            for x in 0..self.width {
                let c = self.cells[(y * self.width + x) as usize];
                s += &format!(
                    "Cell[{x},{y}]={{{},{},{}}}\r\n",
                    c.mask, c.terrain, c.altitude
                );
            }
        }
        encoding_rs::GBK.encode(&s).0.into_owned()
    }
}
