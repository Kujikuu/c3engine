//! PHY chunk: a skinned triangle mesh (up to two bones per vertex).
//! Port of `Phy_LoadVersioned` / `Phy_ReadVersionedVertex` (C3_CORE_DLL/source/c3_phy.cpp).
//!
//! ```text
//! name: u32 len + bytes · blend_count u32 · normal_vertices u32 · alpha_vertices u32
//! vertices × { pos f32×3, [legacy morph f32×9], uv f32×2, color u32, bones u32×2, weights f32×2, [normal f32×3] }
//! normal_triangles u32 · alpha_triangles u32 · indices u16 × 3·(n+a)
//! texture: u32 len + bytes · bbox_min f32×3 · bbox_max f32×3 · bind matrix f32×16
//! tex_row u32 · 3 × keys(u32 n + n × {frame i32, f32, bool u32, i32})
//! optional "STEP" uv_step f32×2 · optional "2SID" · optional "BILB" | "BIB2"
//! ```

use crate::io::{Error, Reader, Result, Writer};

/// On-disk vertex layout, selected by the chunk id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub stored_normal: bool,
    /// Three obsolete morph positions follow the position.
    pub legacy_morph: bool,
}

impl Layout {
    pub const PHY: Self = Self {
        stored_normal: false,
        legacy_morph: true,
    };
    pub const PHY2: Self = Self {
        stored_normal: true,
        legacy_morph: true,
    };
    pub const PHY3: Self = Self {
        stored_normal: true,
        legacy_morph: false,
    };
    pub const PHY4: Self = Self {
        stored_normal: false,
        legacy_morph: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 3],
    /// Kept verbatim for round-trips; the 5065 client ignores them.
    pub legacy_morph: Option<[f32; 9]>,
    pub uv: [f32; 2],
    /// ARGB.
    pub color: u32,
    pub bones: [u32; 2],
    pub weights: [f32; 2],
    pub normal: Option<[f32; 3]>,
}

/// Per-frame parameter key (alpha, draw on/off, texture change).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    pub frame: i32,
    pub float: f32,
    pub flag: u32,
    pub int: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Billboard {
    None,
    /// "BILB": faces the camera.
    Full,
    /// "BIB2": rotates around its vertical axis.
    Vertical,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    pub layout: Layout,
    pub name: Vec<u8>,
    pub blend_count: u32,
    /// Opaque vertices come first, then alpha-blended ones.
    pub opaque_vertices: u32,
    pub vertices: Vec<Vertex>,
    /// Opaque triangles come first, then alpha-blended ones.
    pub opaque_triangles: u32,
    pub indices: Vec<u16>,
    pub texture: Vec<u8>,
    pub bbox: ([f32; 3], [f32; 3]),
    /// Row-major D3D matrix (row vectors, translation in the last row).
    pub bind_matrix: [f32; 16],
    pub tex_row: u32,
    pub alpha_keys: Vec<Key>,
    pub draw_keys: Vec<Key>,
    pub texture_keys: Vec<Key>,
    pub uv_step: Option<[f32; 2]>,
    pub two_sided: bool,
    pub billboard: Billboard,
}

fn f32s<const N: usize>(r: &mut Reader) -> Result<[f32; N]> {
    let mut a = [0.0; N];
    for v in &mut a {
        *v = r.f32()?;
    }
    Ok(a)
}

fn bytes_u32(r: &mut Reader) -> Result<Vec<u8>> {
    let n = r.u32()? as usize;
    Ok(r.bytes(n)?.to_vec())
}

fn keys(r: &mut Reader) -> Result<Vec<Key>> {
    let n = r.u32()? as usize;
    if n * 16 > r.remaining() {
        return Err(Error::Invalid(format!("{n} keys exceed chunk")));
    }
    (0..n)
        .map(|_| {
            Ok(Key {
                frame: r.i32()?,
                float: r.f32()?,
                flag: r.u32()?,
                int: r.i32()?,
            })
        })
        .collect()
}

impl Mesh {
    pub fn parse(body: &[u8], layout: Layout) -> Result<Self> {
        let mut r = Reader::new(body);
        let name = bytes_u32(&mut r)?;
        let blend_count = r.u32()?;
        let opaque_vertices = r.u32()?;
        let alpha_vertices = r.u32()?;
        let vcount = opaque_vertices as usize + alpha_vertices as usize;
        let vsize = 12
            + if layout.legacy_morph { 36 } else { 0 }
            + 28
            + if layout.stored_normal { 12 } else { 0 };
        if vcount * vsize > r.remaining() {
            return Err(Error::Invalid(format!("{vcount} vertices exceed chunk")));
        }
        let mut vertices = Vec::with_capacity(vcount);
        for _ in 0..vcount {
            vertices.push(Vertex {
                pos: f32s(&mut r)?,
                legacy_morph: if layout.legacy_morph {
                    Some(f32s(&mut r)?)
                } else {
                    None
                },
                uv: f32s(&mut r)?,
                color: r.u32()?,
                bones: [r.u32()?, r.u32()?],
                weights: f32s(&mut r)?,
                normal: if layout.stored_normal {
                    Some(f32s(&mut r)?)
                } else {
                    None
                },
            });
        }
        let opaque_triangles = r.u32()?;
        let alpha_triangles = r.u32()?;
        let icount = 3 * (opaque_triangles as usize + alpha_triangles as usize);
        if icount * 2 > r.remaining() {
            return Err(Error::Invalid(format!("{icount} indices exceed chunk")));
        }
        let indices = (0..icount).map(|_| r.u16()).collect::<Result<_>>()?;
        let texture = bytes_u32(&mut r)?;
        let bbox = (f32s(&mut r)?, f32s(&mut r)?);
        let bind_matrix = f32s(&mut r)?;
        let tex_row = r.u32()?;
        let alpha_keys = keys(&mut r)?;
        let draw_keys = keys(&mut r)?;
        let texture_keys = keys(&mut r)?;

        let mut uv_step = None;
        let mut two_sided = false;
        let mut billboard = Billboard::None;
        let tag = |r: &mut Reader, t: &[u8; 4]| {
            let hit = r.remaining() >= 4 && r.peek(4) == Some(&t[..]);
            if hit {
                r.bytes(4).ok();
            }
            hit
        };
        if tag(&mut r, b"STEP") {
            uv_step = Some(f32s(&mut r)?);
        }
        if tag(&mut r, b"2SID") {
            two_sided = true;
        }
        if tag(&mut r, b"BILB") {
            billboard = Billboard::Full;
        } else if tag(&mut r, b"BIB2") {
            billboard = Billboard::Vertical;
        }
        if r.remaining() != 0 {
            return Err(Error::Invalid(format!(
                "{} unparsed bytes at end of PHY",
                r.remaining()
            )));
        }

        Ok(Self {
            layout,
            name,
            blend_count,
            opaque_vertices,
            vertices,
            opaque_triangles,
            indices,
            texture,
            bbox,
            bind_matrix,
            tex_row,
            alpha_keys,
            draw_keys,
            texture_keys,
            uv_step,
            two_sided,
            billboard,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        let floats = |w: &mut Writer, a: &[f32]| a.iter().for_each(|&f| w.f32(f));
        w.u32(self.name.len() as u32);
        w.bytes(&self.name);
        w.u32(self.blend_count);
        w.u32(self.opaque_vertices);
        w.u32(self.vertices.len() as u32 - self.opaque_vertices);
        for v in &self.vertices {
            floats(&mut w, &v.pos);
            if let Some(m) = &v.legacy_morph {
                floats(&mut w, m);
            }
            floats(&mut w, &v.uv);
            w.u32(v.color);
            w.u32(v.bones[0]);
            w.u32(v.bones[1]);
            floats(&mut w, &v.weights);
            if let Some(n) = &v.normal {
                floats(&mut w, n);
            }
        }
        let triangles = self.indices.len() as u32 / 3;
        w.u32(self.opaque_triangles);
        w.u32(triangles - self.opaque_triangles);
        self.indices.iter().for_each(|&i| w.u16(i));
        w.u32(self.texture.len() as u32);
        w.bytes(&self.texture);
        floats(&mut w, &self.bbox.0);
        floats(&mut w, &self.bbox.1);
        floats(&mut w, &self.bind_matrix);
        w.u32(self.tex_row);
        for keys in [&self.alpha_keys, &self.draw_keys, &self.texture_keys] {
            w.u32(keys.len() as u32);
            for k in keys {
                w.i32(k.frame);
                w.f32(k.float);
                w.u32(k.flag);
                w.i32(k.int);
            }
        }
        if let Some(s) = &self.uv_step {
            w.bytes(b"STEP");
            floats(&mut w, s);
        }
        if self.two_sided {
            w.bytes(b"2SID");
        }
        match self.billboard {
            Billboard::None => {}
            Billboard::Full => w.bytes(b"BILB"),
            Billboard::Vertical => w.bytes(b"BIB2"),
        }
        w.buf
    }

    pub fn name_str(&self) -> String {
        String::from_utf8_lossy(&self.name).into_owned()
    }

    pub fn texture_str(&self) -> String {
        String::from_utf8_lossy(&self.texture).into_owned()
    }
}
