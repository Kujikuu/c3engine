//! C3 model/effect files: a 16-byte header followed by chunks `{id: [u8; 4], size: u32, body}`.
//! Ported from the client's chunk walk (`3DBaseCode/3DEffect.cpp` `C3DEffect::Load`).
//!
//! The container keeps every chunk's raw bytes, so files round-trip byte-for-byte.
//! Typed decoders exist per chunk kind: [`phy`] (mesh) and [`motion`] (skeletal motion).

pub mod motion;
pub mod phy;

use crate::io::{Error, Reader, Result, Writer};

pub const HEADER_LEN: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub id: [u8; 4],
    pub body: Vec<u8>,
}

impl Chunk {
    pub fn id_str(&self) -> String {
        String::from_utf8_lossy(&self.id).into_owned()
    }

    /// Kind of a known chunk.
    pub fn kind(&self) -> ChunkKind {
        match &self.id {
            b"PHY " => ChunkKind::Phy(phy::Layout::PHY),
            b"PHY2" => ChunkKind::Phy(phy::Layout::PHY2),
            b"PHY3" => ChunkKind::Phy(phy::Layout::PHY3),
            b"PHY4" => ChunkKind::Phy(phy::Layout::PHY4),
            b"MOTI" => ChunkKind::Motion,
            b"SMOT" => ChunkKind::ShapeMotion,
            b"SHAP" => ChunkKind::Shape,
            b"PTCL" | b"PTCX" | b"PTC3" => ChunkKind::Particles,
            _ => ChunkKind::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkKind {
    Phy(phy::Layout),
    Motion,
    ShapeMotion,
    Shape,
    Particles,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct C3File {
    /// e.g. `MAXFILE C3 00001`.
    pub header: [u8; HEADER_LEN],
    pub chunks: Vec<Chunk>,
}

impl C3File {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let header = r.array::<HEADER_LEN>()?;
        let mut chunks = Vec::new();
        while r.remaining() > 0 {
            let id = r.array::<4>()?;
            let size = r.u32()? as usize;
            if size > r.remaining() {
                return Err(Error::Invalid(format!(
                    "chunk {:?} size {size} exceeds file at {}",
                    String::from_utf8_lossy(&id),
                    r.pos()
                )));
            }
            chunks.push(Chunk {
                id,
                body: r.bytes(size)?.to_vec(),
            });
        }
        Ok(Self { header, chunks })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.bytes(&self.header);
        for c in &self.chunks {
            w.bytes(&c.id);
            w.u32(c.body.len() as u32);
            w.bytes(&c.body);
        }
        w.buf
    }

    /// Typed meshes, in file order. The n-th mesh is animated by the n-th motion.
    pub fn meshes(&self) -> Result<Vec<phy::Mesh>> {
        self.chunks
            .iter()
            .filter_map(|c| match c.kind() {
                ChunkKind::Phy(layout) => Some(phy::Mesh::parse(&c.body, layout)),
                _ => None,
            })
            .collect()
    }

    /// Typed skeletal motions, in file order.
    pub fn motions(&self) -> Result<Vec<motion::Motion>> {
        self.chunks
            .iter()
            .filter(|c| c.kind() == ChunkKind::Motion)
            .map(|c| motion::Motion::parse(&c.body))
            .collect()
    }
}
