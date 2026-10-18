//! Texture decoding: DDS (DXT1, DXT3, DXT5, uncompressed 32/24-bit) and TGA (types 2/10)
//! to straight RGBA8. The 5065 archives contain only DXT1/DXT3 DDS plus a few TGAs.

use crate::io::{Error, Reader, Result};
pub use c3_assets::Image;

/// Decodes a DDS or TGA file to an engine [`Image`].
pub fn decode_image(data: &[u8]) -> Result<Image> {
    if data.starts_with(b"DDS ") {
        decode_dds(data)
    } else {
        decode_tga(data)
    }
}

fn rgb565(c: u16) -> [u8; 3] {
    let r = (c >> 11) & 31;
    let g = (c >> 5) & 63;
    let b = c & 31;
    [
        ((r << 3) | (r >> 2)) as u8,
        ((g << 2) | (g >> 4)) as u8,
        ((b << 3) | (b >> 2)) as u8,
    ]
}

/// Decodes one 8-byte BC1 color block into 16 RGBA pixels.
fn color_block(b: &[u8], out: &mut [[u8; 4]; 16], bc1_alpha: bool) {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mix = |a: u8, b: u8, wa: u16, wb: u16| ((a as u16 * wa + b as u16 * wb) / (wa + wb)) as u8;
    let mut pal = [[0u8; 4]; 4];
    pal[0] = [p0[0], p0[1], p0[2], 255];
    pal[1] = [p1[0], p1[1], p1[2], 255];
    if c0 > c1 || !bc1_alpha {
        for i in 0..3 {
            pal[2][i] = mix(p0[i], p1[i], 2, 1);
            pal[3][i] = mix(p0[i], p1[i], 1, 2);
        }
        pal[2][3] = 255;
        pal[3][3] = 255;
    } else {
        for i in 0..3 {
            pal[2][i] = mix(p0[i], p1[i], 1, 1);
        }
        pal[2][3] = 255;
        pal[3] = [0, 0, 0, 0];
    }
    let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    for (i, px) in out.iter_mut().enumerate() {
        *px = pal[((bits >> (2 * i)) & 3) as usize];
    }
}

fn decode_blocks(data: &[u8], w: u32, h: u32, block_bytes: usize, kind: u8) -> Result<Vec<u8>> {
    let bw = w.div_ceil(4) as usize;
    let bh = h.div_ceil(4) as usize;
    if data.len() < bw * bh * block_bytes {
        return Err(Error::Invalid("DDS data truncated".into()));
    }
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let mut px = [[0u8; 4]; 16];
    for by in 0..bh {
        for bx in 0..bw {
            let blk = &data[(by * bw + bx) * block_bytes..][..block_bytes];
            match kind {
                1 => color_block(blk, &mut px, true),
                2 => {
                    color_block(&blk[8..], &mut px, false);
                    for (i, p) in px.iter_mut().enumerate() {
                        let a = (blk[i / 2] >> (4 * (i % 2))) & 15;
                        p[3] = a * 17;
                    }
                }
                _ => {
                    color_block(&blk[8..], &mut px, false);
                    let (a0, a1) = (blk[0] as u32, blk[1] as u32);
                    let mut pal = [0u32; 8];
                    pal[0] = a0;
                    pal[1] = a1;
                    if a0 > a1 {
                        for i in 1..7 {
                            pal[i + 1] = ((7 - i as u32) * a0 + i as u32 * a1) / 7;
                        }
                    } else {
                        for i in 1..5 {
                            pal[i + 1] = ((5 - i as u32) * a0 + i as u32 * a1) / 5;
                        }
                        pal[6] = 0;
                        pal[7] = 255;
                    }
                    let bits = blk[2..8]
                        .iter()
                        .rev()
                        .fold(0u64, |acc, &b| (acc << 8) | b as u64);
                    for (i, p) in px.iter_mut().enumerate() {
                        p[3] = pal[((bits >> (3 * i)) & 7) as usize] as u8;
                    }
                }
            }
            for (i, p) in px.iter().enumerate() {
                let x = bx * 4 + i % 4;
                let y = by * 4 + i / 4;
                if x < w as usize && y < h as usize {
                    let o = (y * w as usize + x) * 4;
                    rgba[o..o + 4].copy_from_slice(p);
                }
            }
        }
    }
    Ok(rgba)
}

fn mask_shift(mask: u32) -> (u32, u32) {
    if mask == 0 {
        return (0, 0);
    }
    let shift = mask.trailing_zeros();
    (shift, (mask >> shift).count_ones())
}

fn expand(v: u32, mask: u32, default: u8) -> u8 {
    let (s, bits) = mask_shift(mask);
    if bits == 0 {
        return default;
    }
    let x = (v & mask) >> s;
    ((x * 255) / ((1 << bits) - 1)) as u8
}

fn decode_dds(data: &[u8]) -> Result<Image> {
    let mut r = Reader::new(data);
    r.bytes(4)?;
    if r.u32()? != 124 {
        return Err(Error::Invalid("bad DDS header size".into()));
    }
    r.u32()?; // flags
    let height = r.u32()?;
    let width = r.u32()?;
    r.bytes(4 * 3 + 4 * 11)?; // pitch, depth, mips, reserved[11]
    r.u32()?; // pf size
    let pf_flags = r.u32()?;
    let fourcc = r.array::<4>()?;
    let bits = r.u32()?;
    let masks = [r.u32()?, r.u32()?, r.u32()?, r.u32()?];
    let body = &data[128..];

    let rgba = if pf_flags & 4 != 0 {
        match &fourcc {
            b"DXT1" => decode_blocks(body, width, height, 8, 1)?,
            b"DXT2" | b"DXT3" => decode_blocks(body, width, height, 16, 2)?,
            b"DXT4" | b"DXT5" => decode_blocks(body, width, height, 16, 3)?,
            f => {
                return Err(Error::Invalid(format!(
                    "unsupported DDS fourcc {:?}",
                    String::from_utf8_lossy(f)
                )));
            }
        }
    } else if bits == 32 || bits == 24 || bits == 16 {
        let bpp = (bits / 8) as usize;
        let n = (width * height) as usize;
        if body.len() < n * bpp {
            return Err(Error::Invalid("DDS data truncated".into()));
        }
        let has_alpha = pf_flags & 1 != 0;
        let mut out = Vec::with_capacity(n * 4);
        for px in body[..n * bpp].chunks_exact(bpp) {
            let mut v = 0u32;
            for (i, &b) in px.iter().enumerate() {
                v |= (b as u32) << (8 * i);
            }
            out.push(expand(v, masks[0], 0));
            out.push(expand(v, masks[1], 0));
            out.push(expand(v, masks[2], 0));
            out.push(if has_alpha {
                expand(v, masks[3], 255)
            } else {
                255
            });
        }
        out
    } else {
        return Err(Error::Invalid(format!(
            "unsupported DDS pixel format ({bits} bpp)"
        )));
    };
    Ok(Image::new(width, height, rgba))
}

fn decode_tga(data: &[u8]) -> Result<Image> {
    let mut r = Reader::new(data);
    let id_len = r.array::<1>()?[0] as usize;
    let cmap_type = r.array::<1>()?[0];
    let img_type = r.array::<1>()?[0];
    let cmap = r.array::<5>()?;
    r.bytes(4)?; // origin
    let width = r.u16()? as u32;
    let height = r.u16()? as u32;
    let bits = r.array::<1>()?[0];
    let desc = r.array::<1>()?[0];
    if cmap_type != 0 || !(img_type == 2 || img_type == 10) || !(bits == 24 || bits == 32) {
        return Err(Error::Invalid(format!(
            "unsupported TGA type={img_type} bits={bits}"
        )));
    }
    r.bytes(id_len)?;
    let _ = cmap;
    let bpp = (bits / 8) as usize;
    let n = (width * height) as usize;
    let mut px = Vec::with_capacity(n * 4);
    let push = |px: &mut Vec<u8>, p: &[u8]| {
        px.extend_from_slice(&[p[2], p[1], p[0], if bpp == 4 { p[3] } else { 255 }]);
    };
    if img_type == 2 {
        for _ in 0..n {
            push(&mut px, r.bytes(bpp)?);
        }
    } else {
        while px.len() < n * 4 {
            let h = r.array::<1>()?[0];
            let count = (h & 0x7F) as usize + 1;
            if h & 0x80 != 0 {
                let p = r.bytes(bpp)?;
                for _ in 0..count {
                    push(&mut px, p);
                }
            } else {
                for _ in 0..count {
                    push(&mut px, r.bytes(bpp)?);
                }
            }
        }
        px.truncate(n * 4);
    }
    // Bottom-up unless descriptor bit 5 is set.
    if desc & 0x20 == 0 {
        let row = width as usize * 4;
        let mut flipped = Vec::with_capacity(px.len());
        for y in (0..height as usize).rev() {
            flipped.extend_from_slice(&px[y * row..(y + 1) * row]);
        }
        px = flipped;
    }
    Ok(Image::new(width, height, px))
}
