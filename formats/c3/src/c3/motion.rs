//! MOTI chunk: skeletal animation as per-bone matrices at key frames.
//! Port of `Motion_Load` (C3_CORE_DLL/source/c3_phy.cpp).
//!
//! ```text
//! bones u32 · frames u32
//! "KKEY": keys u32, keys × { frame u32, bones × matrix f32×16 }
//! "XKEY": keys u32, keys × { frame u16, bones × 3×4 matrix f32×12 (last column 0,0,0,1 implied) }
//! "ZKEY": keys u32, keys × { frame u16, bones × { quat f32×4 (x,y,z,w), translation f32×3 } }
//! (none): frames × bones matrices, stored bone-major
//! morphs u32 · frames × morphs × f32
//! ```

use crate::io::{Error, Reader, Result, Writer};

/// Row-major D3D matrix (row vectors, translation in the last row).
pub type Matrix = [f32; 16];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuatTranslation {
    /// x, y, z, w.
    pub rotation: [f32; 4],
    pub translation: [f32; 3],
}

impl QuatTranslation {
    /// Matrix as `D3DXMatrixRotationQuaternion` + translation builds it.
    pub fn to_matrix(&self) -> Matrix {
        let [x, y, z, w] = self.rotation;
        let [tx, ty, tz] = self.translation;
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y + z * w),
            2.0 * (x * z - y * w),
            0.0,
            2.0 * (x * y - z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + x * w),
            0.0,
            2.0 * (x * z + y * w),
            2.0 * (y * z - x * w),
            1.0 - 2.0 * (x * x + y * y),
            0.0,
            tx,
            ty,
            tz,
            1.0,
        ]
    }
}

/// Key frame data as stored; the encoding is kept so files round-trip exactly.
#[derive(Debug, Clone, PartialEq)]
pub enum Keys {
    /// "KKEY": full matrices at sparse key frames.
    Matrices(Vec<(u32, Vec<Matrix>)>),
    /// "XKEY": 3×4 matrices (rows 1–4, first three columns) at sparse key frames.
    Affine(Vec<(u16, Vec<[f32; 12]>)>),
    /// "ZKEY": compact rotation + translation at sparse key frames.
    Compact(Vec<(u16, Vec<QuatTranslation>)>),
    /// No tag: a matrix for every frame. `frames[frame][bone]`.
    Dense(Vec<Vec<Matrix>>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Motion {
    pub bones: u32,
    pub frames: u32,
    pub keys: Keys,
    /// Morph weights per frame (unused by the 5065 client, kept for round-trips).
    pub morphs: u32,
    pub morph_weights: Vec<f32>,
}

/// Expands a 3×4 "tidy" matrix: column 4 of each row is (0, 0, 0, 1).
fn affine_to_matrix(a: &[f32; 12]) -> Matrix {
    [
        a[0], a[1], a[2], 0.0, a[3], a[4], a[5], 0.0, a[6], a[7], a[8], 0.0, a[9], a[10], a[11],
        1.0,
    ]
}

fn matrix(r: &mut Reader) -> Result<Matrix> {
    let mut m = [0.0; 16];
    for v in &mut m {
        *v = r.f32()?;
    }
    Ok(m)
}

fn check(r: &Reader, need: usize, what: &str) -> Result<()> {
    if need > r.remaining() {
        return Err(Error::Invalid(format!("{what} exceed chunk")));
    }
    Ok(())
}

impl Motion {
    pub fn parse(body: &[u8]) -> Result<Self> {
        let mut r = Reader::new(body);
        let bones = r.u32()?;
        let frames = r.u32()?;
        let nb = bones as usize;
        let keys = match r.peek(4) {
            Some(b"KKEY") => {
                r.bytes(4)?;
                let n = r.u32()? as usize;
                check(&r, n * (4 + nb * 64), "KKEY keys")?;
                Keys::Matrices(
                    (0..n)
                        .map(|_| {
                            Ok((
                                r.u32()?,
                                (0..nb).map(|_| matrix(&mut r)).collect::<Result<_>>()?,
                            ))
                        })
                        .collect::<Result<_>>()?,
                )
            }
            Some(b"XKEY") => {
                r.bytes(4)?;
                let n = r.u32()? as usize;
                check(&r, n * (2 + nb * 48), "XKEY keys")?;
                let mut keys = Vec::with_capacity(n);
                for _ in 0..n {
                    let frame = r.u16()?;
                    let mut b = Vec::with_capacity(nb);
                    for _ in 0..nb {
                        let mut m = [0.0; 12];
                        for v in &mut m {
                            *v = r.f32()?;
                        }
                        b.push(m);
                    }
                    keys.push((frame, b));
                }
                Keys::Affine(keys)
            }
            Some(b"ZKEY") => {
                r.bytes(4)?;
                let n = r.u32()? as usize;
                check(&r, n * (2 + nb * 28), "ZKEY keys")?;
                let mut keys = Vec::with_capacity(n);
                for _ in 0..n {
                    let frame = r.u16()?;
                    let mut b = Vec::with_capacity(nb);
                    for _ in 0..nb {
                        b.push(QuatTranslation {
                            rotation: [r.f32()?, r.f32()?, r.f32()?, r.f32()?],
                            translation: [r.f32()?, r.f32()?, r.f32()?],
                        });
                    }
                    keys.push((frame, b));
                }
                Keys::Compact(keys)
            }
            _ => {
                let nf = frames as usize;
                check(&r, nf * nb * 64, "dense frames")?;
                let mut dense = vec![vec![[0.0; 16]; nb]; nf];
                for bone in 0..nb {
                    for frame in dense.iter_mut() {
                        frame[bone] = matrix(&mut r)?;
                    }
                }
                Keys::Dense(dense)
            }
        };
        let morphs = r.u32()?;
        let n = frames as usize * morphs as usize;
        check(&r, n * 4, "morph weights")?;
        let morph_weights = (0..n).map(|_| r.f32()).collect::<Result<_>>()?;
        if r.remaining() != 0 {
            return Err(Error::Invalid(format!(
                "{} unparsed bytes at end of MOTI",
                r.remaining()
            )));
        }
        Ok(Self {
            bones,
            frames,
            keys,
            morphs,
            morph_weights,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u32(self.bones);
        w.u32(self.frames);
        let put = |w: &mut Writer, m: &Matrix| m.iter().for_each(|&f| w.f32(f));
        match &self.keys {
            Keys::Matrices(keys) => {
                w.bytes(b"KKEY");
                w.u32(keys.len() as u32);
                for (frame, ms) in keys {
                    w.u32(*frame);
                    ms.iter().for_each(|m| put(&mut w, m));
                }
            }
            Keys::Affine(keys) => {
                w.bytes(b"XKEY");
                w.u32(keys.len() as u32);
                for (frame, bs) in keys {
                    w.u16(*frame);
                    bs.iter().flatten().for_each(|&f| w.f32(f));
                }
            }
            Keys::Compact(keys) => {
                w.bytes(b"ZKEY");
                w.u32(keys.len() as u32);
                for (frame, bs) in keys {
                    w.u16(*frame);
                    for b in bs {
                        b.rotation
                            .iter()
                            .chain(&b.translation)
                            .for_each(|&f| w.f32(f));
                    }
                }
            }
            Keys::Dense(frames) => {
                for bone in 0..self.bones as usize {
                    frames.iter().for_each(|f| put(&mut w, &f[bone]));
                }
            }
        }
        w.u32(self.morphs);
        self.morph_weights.iter().for_each(|&f| w.f32(f));
        w.buf
    }

    /// Key frames as (frame, per-bone matrices), whatever the encoding.
    pub fn key_matrices(&self) -> Vec<(u32, Vec<Matrix>)> {
        match &self.keys {
            Keys::Matrices(k) => k.clone(),
            Keys::Affine(k) => k
                .iter()
                .map(|(f, b)| (*f as u32, b.iter().map(affine_to_matrix).collect()))
                .collect(),
            Keys::Compact(k) => k
                .iter()
                .map(|(f, b)| {
                    (
                        *f as u32,
                        b.iter().map(QuatTranslation::to_matrix).collect(),
                    )
                })
                .collect(),
            Keys::Dense(frames) => frames
                .iter()
                .enumerate()
                .map(|(i, m)| (i as u32, m.clone()))
                .collect(),
        }
    }
}
