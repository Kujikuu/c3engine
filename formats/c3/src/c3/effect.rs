//! C3 effect chunks: SHAP (ribbon trails), SMOT (trail motion), PTCL (baked particles),
//! CAME (camera path). Ports of `Shape_Load`, `SMotion_Load` (c3_shape.cpp),
//! `Ptcl_Load(C3Ptcl*)` (c3_ptcl.cpp) and `Camera_Load` (c3_camera.cpp).
//!
//! ```text
//! SHAP: name · lines u32 · lines × { points u32, points × f32×3 } · texture · segments u32
//! SMOT: frames u32 · frames × matrix f32×16
//! PTCL: name · texture · rows u32 · capacity u32 · frames u32 ·
//!       frames × { n u32, if n > 0: pos f32×3 ×n, age f32 ×n, size f32 ×n, matrix f32×16 }
//! CAME: name · fov f32 · frames u32 · from f32×3 ×frames · to f32×3 ×frames
//! ```
//! Strings are `u32 length + bytes`. PTCX/PTC3 never occur in 5065 and stay raw.

use crate::io::{Error, Reader, Result, Writer};

pub type Vec3 = [f32; 3];
pub type Matrix = [f32; 16];

fn string(r: &mut Reader) -> Result<Vec<u8>> {
    let n = r.u32()? as usize;
    Ok(r.bytes(n)?.to_vec())
}

fn put_string(w: &mut Writer, s: &[u8]) {
    w.u32(s.len() as u32);
    w.bytes(s);
}

fn floats<const N: usize>(r: &mut Reader) -> Result<[f32; N]> {
    let mut a = [0.0; N];
    for v in &mut a {
        *v = r.f32()?;
    }
    Ok(a)
}

/// Reads `n` items of `size` bytes each, after checking they fit.
fn many<T>(
    r: &mut Reader,
    n: usize,
    size: usize,
    mut f: impl FnMut(&mut Reader) -> Result<T>,
) -> Result<Vec<T>> {
    if n.saturating_mul(size) > r.remaining() {
        return Err(Error::Invalid(format!("{n} items exceed chunk")));
    }
    (0..n).map(|_| f(r)).collect()
}

fn done(r: &Reader, what: &str) -> Result<()> {
    match r.remaining() {
        0 => Ok(()),
        n => Err(Error::Invalid(format!(
            "{n} unparsed bytes at end of {what}"
        ))),
    }
}

fn put_floats(w: &mut Writer, a: &[f32]) {
    a.iter().for_each(|&f| w.f32(f));
}

/// Ribbon trail: polylines swept by an [`ShapeMotion`].
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub name: Vec<u8>,
    pub lines: Vec<Vec<Vec3>>,
    pub texture: Vec<u8>,
    pub segments: u32,
}

impl Shape {
    pub fn parse(body: &[u8]) -> Result<Self> {
        let mut r = Reader::new(body);
        let name = string(&mut r)?;
        let n = r.u32()? as usize;
        let lines = many(&mut r, n, 4, |r| {
            let points = r.u32()? as usize;
            many(r, points, 12, floats::<3>)
        })?;
        let texture = string(&mut r)?;
        let segments = r.u32()?;
        done(&r, "SHAP")?;
        Ok(Self {
            name,
            lines,
            texture,
            segments,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        put_string(&mut w, &self.name);
        w.u32(self.lines.len() as u32);
        for line in &self.lines {
            w.u32(line.len() as u32);
            line.iter().for_each(|p| put_floats(&mut w, p));
        }
        put_string(&mut w, &self.texture);
        w.u32(self.segments);
        w.buf
    }
}

/// One transform per frame for a [`Shape`].
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeMotion {
    pub frames: Vec<Matrix>,
}

impl ShapeMotion {
    pub fn parse(body: &[u8]) -> Result<Self> {
        let mut r = Reader::new(body);
        let n = r.u32()? as usize;
        let frames = many(&mut r, n, 64, floats::<16>)?;
        done(&r, "SMOT")?;
        Ok(Self { frames })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u32(self.frames.len() as u32);
        self.frames.iter().for_each(|m| put_floats(&mut w, m));
        w.buf
    }
}

/// Live particles at one frame.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParticleFrame {
    pub positions: Vec<Vec3>,
    pub ages: Vec<f32>,
    pub sizes: Vec<f32>,
    /// Emitter transform; only stored when the frame has particles.
    pub matrix: Option<Matrix>,
}

/// Particle system baked to per-frame positions.
#[derive(Debug, Clone, PartialEq)]
pub struct Particles {
    pub name: Vec<u8>,
    pub texture: Vec<u8>,
    /// Rows in the texture atlas.
    pub rows: u32,
    /// Maximum live particles.
    pub capacity: u32,
    pub frames: Vec<ParticleFrame>,
}

impl Particles {
    pub fn parse(body: &[u8]) -> Result<Self> {
        let mut r = Reader::new(body);
        let name = string(&mut r)?;
        let texture = string(&mut r)?;
        let rows = r.u32()?;
        let capacity = r.u32()?;
        let n = r.u32()? as usize;
        let frames = many(&mut r, n, 4, |r| {
            let count = r.u32()? as usize;
            if count == 0 {
                return Ok(ParticleFrame::default());
            }
            Ok(ParticleFrame {
                positions: many(r, count, 12, floats::<3>)?,
                ages: many(r, count, 4, |r| r.f32())?,
                sizes: many(r, count, 4, |r| r.f32())?,
                matrix: Some(floats::<16>(r)?),
            })
        })?;
        done(&r, "PTCL")?;
        Ok(Self {
            name,
            texture,
            rows,
            capacity,
            frames,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        put_string(&mut w, &self.name);
        put_string(&mut w, &self.texture);
        w.u32(self.rows);
        w.u32(self.capacity);
        w.u32(self.frames.len() as u32);
        for f in &self.frames {
            w.u32(f.positions.len() as u32);
            if f.positions.is_empty() {
                continue;
            }
            f.positions.iter().for_each(|p| put_floats(&mut w, p));
            put_floats(&mut w, &f.ages);
            put_floats(&mut w, &f.sizes);
            put_floats(&mut w, &f.matrix.unwrap_or_default());
        }
        w.buf
    }
}

/// Camera path: eye and target position per frame.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraPath {
    pub name: Vec<u8>,
    /// Field of view (stored but ignored by the 5065 client).
    pub fov: f32,
    pub from: Vec<Vec3>,
    pub to: Vec<Vec3>,
}

impl CameraPath {
    pub fn parse(body: &[u8]) -> Result<Self> {
        let mut r = Reader::new(body);
        let name = string(&mut r)?;
        let fov = r.f32()?;
        let n = r.u32()? as usize;
        let from = many(&mut r, n, 12, floats::<3>)?;
        let to = many(&mut r, n, 12, floats::<3>)?;
        done(&r, "CAME")?;
        Ok(Self {
            name,
            fov,
            from,
            to,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        put_string(&mut w, &self.name);
        w.f32(self.fov);
        w.u32(self.from.len() as u32);
        self.from
            .iter()
            .chain(&self.to)
            .for_each(|p| put_floats(&mut w, p));
        w.buf
    }
}
