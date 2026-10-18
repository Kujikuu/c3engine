//! `ini/GameMap.dat`: map document id → DMap path. Port of `LoadGameMapCatalogEntry`
//! (3dgamemap/GameMapCatalog.cpp).
//!
//! ```text
//! count u32 · count × { id u32, path_len u32, path bytes, puzzle_grid_size u32 }
//! ```

use crate::io::{Error, Reader, Result, Writer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapEntry {
    pub id: u32,
    /// Raw path bytes (GBK); see [`MapEntry::path_str`].
    pub path: Vec<u8>,
    /// Ground tile size in pixels (normally 256).
    pub puzzle_grid_size: u32,
}

impl MapEntry {
    pub fn path_str(&self) -> String {
        encoding_rs::GBK
            .decode_without_bom_handling(&self.path)
            .0
            .into_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GameMapCatalog {
    pub entries: Vec<MapEntry>,
}

impl GameMapCatalog {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let n = r.u32()? as usize;
        let mut entries = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            let id = r.u32()?;
            let len = r.u32()? as usize;
            let path = r.bytes(len)?.to_vec();
            entries.push(MapEntry {
                id,
                path,
                puzzle_grid_size: r.u32()?,
            });
        }
        if r.remaining() != 0 {
            return Err(Error::Invalid(format!("{} trailing bytes", r.remaining())));
        }
        Ok(Self { entries })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u32(self.entries.len() as u32);
        for e in &self.entries {
            w.u32(e.id);
            w.u32(e.path.len() as u32);
            w.bytes(&e.path);
            w.u32(e.puzzle_grid_size);
        }
        w.buf
    }

    /// First entry with `id` (the client rejects duplicates).
    pub fn get(&self, id: u32) -> Option<&MapEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}
