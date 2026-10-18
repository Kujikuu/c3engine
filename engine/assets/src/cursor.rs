//! Mouse cursors from Windows cursor files: `.cur`/`.ico` (one image and its hotspot) and
//! animated `.ani` (RIFF `ACON`: frames, per-step timing and an optional frame order). Frames
//! are classic bitmaps (1, 4, 8, 24 or 32 bits with a transparency mask) or embedded PNGs,
//! which the caller decodes.

use crate::Image;

/// A decoded cursor: its frames and the order and timing they play in.
#[derive(Debug, Clone, PartialEq)]
pub struct Cursor {
    pub frames: Vec<CursorFrame>,
    /// Frames in play order with how long each shows; one step for a still cursor.
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CursorFrame {
    pub image: Image,
    /// The pointing pixel.
    pub hotspot: [u32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub frame: usize,
    pub ms: u32,
}

/// An `.ani` step's length unit (a "jiffy", 1/60 s), in ms.
const JIFFY_MS: f64 = 1000.0 / 60.0;

impl Cursor {
    /// A still cursor of one image.
    pub fn still(image: Image, hotspot: [u32; 2]) -> Self {
        Self {
            frames: vec![CursorFrame { image, hotspot }],
            steps: vec![Step { frame: 0, ms: 0 }],
        }
    }

    /// The frame showing `ms` after the cursor started.
    pub fn frame_at(&self, ms: u64) -> &CursorFrame {
        let total: u64 = self.steps.iter().map(|s| u64::from(s.ms)).sum();
        let mut t = if total == 0 { 0 } else { ms % total };
        for step in &self.steps {
            if t < u64::from(step.ms) {
                return &self.frames[step.frame];
            }
            t -= u64::from(step.ms);
        }
        &self.frames[self.steps.first().map_or(0, |s| s.frame)]
    }
}

/// Decodes a `.cur`, `.ico` or `.ani` file; `png` decodes frames stored as PNG.
pub fn decode(bytes: &[u8], png: &dyn Fn(&[u8]) -> Option<Image>) -> Result<Cursor, String> {
    if bytes.starts_with(b"RIFF") {
        decode_ani(bytes, png)
    } else {
        let frame = decode_icon(bytes, png)?;
        Ok(Cursor::still(frame.image, frame.hotspot))
    }
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// The largest image of an icon or cursor file, with its hotspot.
fn decode_icon(b: &[u8], png: &dyn Fn(&[u8]) -> Option<Image>) -> Result<CursorFrame, String> {
    let bad = || "not a cursor or icon file".to_owned();
    let (reserved, kind, count) = (
        u16_at(b, 0).ok_or_else(bad)?,
        u16_at(b, 2).ok_or_else(bad)?,
        u16_at(b, 4).ok_or_else(bad)?,
    );
    if reserved != 0 || !(1..=2).contains(&kind) || count == 0 {
        return Err(bad());
    }
    // Pick the biggest entry (cursors usually hold one).
    let entry = (0..usize::from(count))
        .map(|i| 6 + i * 16)
        .filter(|&e| e + 16 <= b.len())
        .max_by_key(|&e| {
            let side = |v: u8| if v == 0 { 256 } else { u32::from(v) };
            side(b[e]) * side(b[e + 1])
        })
        .ok_or_else(bad)?;
    let hotspot = if kind == 2 {
        [
            u32::from(u16_at(b, entry + 4).unwrap_or(0)),
            u32::from(u16_at(b, entry + 6).unwrap_or(0)),
        ]
    } else {
        [0, 0]
    };
    let size = u32_at(b, entry + 8).ok_or_else(bad)? as usize;
    let offset = u32_at(b, entry + 12).ok_or_else(bad)? as usize;
    let data = b
        .get(offset..offset.saturating_add(size).min(b.len()))
        .ok_or("the cursor's image is outside the file")?;
    let image = if data.starts_with(b"\x89PNG") {
        png(data).ok_or("the cursor's PNG image can't be read")?
    } else {
        decode_dib(data)?
    };
    Ok(CursorFrame { image, hotspot })
}

/// A device-independent bitmap of an icon: the colour image over a 1-bit transparency mask,
/// both stored bottom row first, its height counting both.
fn decode_dib(d: &[u8]) -> Result<Image, String> {
    let bad = || "the cursor's bitmap is damaged".to_owned();
    let header = u32_at(d, 0).ok_or_else(bad)? as usize;
    let width = u32_at(d, 4).ok_or_else(bad)? as i32;
    let height = (u32_at(d, 8).ok_or_else(bad)? as i32) / 2;
    let bpp = u16_at(d, 14).ok_or_else(bad)?;
    let compression = u32_at(d, 16).ok_or_else(bad)?;
    if width <= 0 || height <= 0 || width > 1024 || height > 1024 || compression != 0 {
        return Err(bad());
    }
    let (w, h) = (width as usize, height as usize);
    let colours = match u32_at(d, 32).ok_or_else(bad)? {
        0 if bpp <= 8 => 1usize << bpp,
        n => n as usize,
    };
    let palette_at = header;
    let pixels_at = palette_at + if bpp <= 8 { colours * 4 } else { 0 };
    let row = (w * usize::from(bpp)).div_ceil(32) * 4;
    let mask_at = pixels_at + row * h;
    let mask_row = w.div_ceil(32) * 4;
    let palette = |i: usize| -> [u8; 4] {
        let p = palette_at + i * 4;
        d.get(p..p + 3)
            .map_or([0, 0, 0, 255], |c| [c[2], c[1], c[0], 255])
    };
    let mut rgba = vec![0u8; w * h * 4];
    let mut any_alpha = false;
    for y in 0..h {
        let src = pixels_at + (h - 1 - y) * row;
        let line = d.get(src..src + row).ok_or_else(bad)?;
        for x in 0..w {
            let px = match bpp {
                32 => {
                    let c = &line[x * 4..x * 4 + 4];
                    any_alpha |= c[3] != 0;
                    [c[2], c[1], c[0], c[3]]
                }
                24 => {
                    let c = &line[x * 3..x * 3 + 3];
                    [c[2], c[1], c[0], 255]
                }
                8 => palette(usize::from(line[x])),
                4 => palette(usize::from((line[x / 2] >> (4 * (1 - x % 2))) & 0x0F)),
                1 => palette(usize::from((line[x / 8] >> (7 - x % 8)) & 1)),
                _ => return Err(format!("{bpp}-bit cursors aren't supported")),
            };
            rgba[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&px);
        }
    }
    // Without an alpha channel the mask says what is see-through (a set bit).
    if !any_alpha {
        for y in 0..h {
            let line = mask_at + (h - 1 - y) * mask_row;
            for x in 0..w {
                let clear = d
                    .get(line + x / 8)
                    .is_some_and(|m| (m >> (7 - x % 8)) & 1 == 1);
                rgba[(y * w + x) * 4 + 3] = if clear { 0 } else { 255 };
            }
        }
    }
    Ok(Image::new(w as u32, h as u32, rgba))
}

/// An animated cursor: its icon frames, step rates and frame order.
fn decode_ani(b: &[u8], png: &dyn Fn(&[u8]) -> Option<Image>) -> Result<Cursor, String> {
    if b.get(8..12) != Some(b"ACON") {
        return Err("not an animated cursor".into());
    }
    let mut frames = Vec::new();
    let (mut steps, mut default_rate) = (0usize, 0u32);
    let (mut rates, mut order): (Vec<u32>, Vec<usize>) = (Vec::new(), Vec::new());
    let mut chunks = vec![(12usize, b.len())];
    while let Some((mut at, end)) = chunks.pop() {
        while at + 8 <= end {
            let id = &b[at..at + 4];
            let size = u32_at(b, at + 4).unwrap_or(0) as usize;
            let body = at + 8;
            let body_end = body.saturating_add(size).min(end);
            match id {
                b"LIST" => chunks.push((body + 4, body_end)),
                b"anih" => {
                    steps = u32_at(b, body + 8).unwrap_or(0) as usize;
                    default_rate = u32_at(b, body + 28).unwrap_or(0);
                }
                b"rate" => {
                    rates = (body..body_end)
                        .step_by(4)
                        .filter_map(|p| u32_at(b, p))
                        .collect();
                }
                b"seq " => {
                    order = (body..body_end)
                        .step_by(4)
                        .filter_map(|p| u32_at(b, p).map(|v| v as usize))
                        .collect();
                }
                b"icon" => frames.push(decode_icon(&b[body..body_end], png)?),
                _ => {}
            }
            at = body + size + (size & 1);
        }
    }
    if frames.is_empty() {
        return Err("the animated cursor has no frames".into());
    }
    let steps = if steps == 0 { frames.len() } else { steps };
    let steps = (0..steps)
        .map(|i| {
            let frame = order.get(i).copied().unwrap_or(i).min(frames.len() - 1);
            let jiffies = rates.get(i).copied().unwrap_or(default_rate).max(1);
            Step {
                frame,
                ms: (f64::from(jiffies) * JIFFY_MS).round() as u32,
            }
        })
        .collect();
    Ok(Cursor { frames, steps })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2×2 8-bit cursor: red, green / blue, see-through; hotspot (1, 0).
    fn tiny_cur() -> Vec<u8> {
        let mut dib = Vec::new();
        for v in [40u32, 2, 4] {
            dib.extend(v.to_le_bytes());
        }
        dib.extend(1u16.to_le_bytes());
        dib.extend(8u16.to_le_bytes());
        dib.extend([0u8; 4 * 4]); // compression, size, resolution
        dib.extend(3u32.to_le_bytes()); // colours used
        dib.extend(0u32.to_le_bytes());
        for bgr in [[0u8, 0, 255], [0, 255, 0], [255, 0, 0]] {
            dib.extend(bgr);
            dib.push(0);
        }
        // Colour rows, bottom first, 4-byte aligned: (blue, any) then (red, green).
        dib.extend([2u8, 0, 0, 0, 0, 1, 0, 0]);
        // Mask rows, bottom first: the bottom right pixel is see-through.
        dib.extend([0b0100_0000u8, 0, 0, 0, 0, 0, 0, 0]);
        let mut file = Vec::new();
        for v in [0u16, 2, 1] {
            file.extend(v.to_le_bytes());
        }
        file.extend([2u8, 2, 0, 0]);
        file.extend(1u16.to_le_bytes()); // hotspot x
        file.extend(0u16.to_le_bytes()); // hotspot y
        file.extend((dib.len() as u32).to_le_bytes());
        file.extend(22u32.to_le_bytes());
        file.extend(dib);
        file
    }

    fn no_png(_: &[u8]) -> Option<Image> {
        None
    }

    #[test]
    fn decodes_a_cursor_with_its_mask_and_hotspot() {
        let cursor = decode(&tiny_cur(), &no_png).unwrap();
        let frame = &cursor.frames[0];
        assert_eq!(frame.hotspot, [1, 0]);
        assert_eq!(
            frame.image.rgba,
            [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 0, 0, 0]
        );
    }

    #[test]
    fn decodes_an_animated_cursor_with_its_order_and_rates() {
        let icon = tiny_cur();
        let chunk = |id: &[u8], body: &[u8]| {
            let mut c = id.to_vec();
            c.extend((body.len() as u32).to_le_bytes());
            c.extend(body);
            if body.len() % 2 == 1 {
                c.push(0);
            }
            c
        };
        let words = |v: &[u32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        let mut fram = b"fram".to_vec();
        fram.extend(chunk(b"icon", &icon));
        fram.extend(chunk(b"icon", &icon));
        let mut body = b"ACON".to_vec();
        body.extend(chunk(b"anih", &words(&[36, 2, 3, 0, 0, 0, 0, 6, 3])));
        body.extend(chunk(b"rate", &words(&[6, 12, 6])));
        body.extend(chunk(b"seq ", &words(&[0, 1, 0])));
        body.extend(chunk(b"LIST", &fram));
        let file = chunk(b"RIFF", &body);
        let cursor = decode(&file, &no_png).unwrap();
        assert_eq!(cursor.frames.len(), 2);
        let order: Vec<(usize, u32)> = cursor.steps.iter().map(|s| (s.frame, s.ms)).collect();
        assert_eq!(order, [(0, 100), (1, 200), (0, 100)]);
        assert_eq!(cursor.frame_at(150), &cursor.frames[1]);
        assert_eq!(cursor.frame_at(450), &cursor.frames[0]);
    }

    #[test]
    fn decodes_conquers_cursors() {
        let dir = std::path::Path::new(r"D:\Conquer2\Tools\eosource\client5065\data\Cursor");
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let bytes = std::fs::read(entry.path()).unwrap();
            let cursor = decode(&bytes, &no_png)
                .unwrap_or_else(|e| panic!("{}: {e}", entry.path().display()));
            let frame = cursor.frame_at(0);
            assert_eq!((frame.image.width, frame.image.height), (32, 32));
            assert!(frame.image.rgba.chunks(4).any(|p| p[3] == 255));
            assert!(frame.image.rgba.chunks(4).any(|p| p[3] == 0));
        }
    }
}
