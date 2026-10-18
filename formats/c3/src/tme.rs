//! `.TME` terrain magic effect: a group of 3D effects spawned on the ground with delays
//! and offsets. Port of `CTerainEffectUnit::Create` (3dgamemap/TerrainEffectManager.cpp).
//!
//! ```text
//! count u32 · count × { effect char[128], delay u32, random_delay u32, offset_x i32, offset_y i32 }
//! ```
//! The client negates `offset_x` after loading; values here are as stored.

use crate::io::{Error, FixedStr, Reader, Result, Writer};

#[derive(Debug, Clone, PartialEq)]
pub struct TerrainEffectUnit {
    /// Effect name in `3DEffect.ini`.
    pub effect: FixedStr<128>,
    /// Milliseconds before this unit starts.
    pub delay: u32,
    /// Extra random delay range in milliseconds.
    pub random_delay: u32,
    pub offset: (i32, i32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TerrainEffect {
    pub units: Vec<TerrainEffectUnit>,
}

impl TerrainEffect {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let n = r.u32()? as usize;
        if n * 144 > r.remaining() {
            return Err(Error::Invalid(format!("{n} units exceed file")));
        }
        let units = (0..n)
            .map(|_| {
                Ok(TerrainEffectUnit {
                    effect: r.fixed()?,
                    delay: r.u32()?,
                    random_delay: r.u32()?,
                    offset: (r.i32()?, r.i32()?),
                })
            })
            .collect::<Result<_>>()?;
        if r.remaining() != 0 {
            return Err(Error::Invalid(format!("{} trailing bytes", r.remaining())));
        }
        Ok(Self { units })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u32(self.units.len() as u32);
        for u in &self.units {
            w.fixed(&u.effect);
            w.u32(u.delay);
            w.u32(u.random_delay);
            w.i32(u.offset.0);
            w.i32(u.offset.1);
        }
        w.buf
    }
}
