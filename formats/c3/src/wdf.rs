//! WDF package archive.
//!
//! Layout (little-endian):
//! ```text
//! header  { id: u32 = 'WDFP', count: i32, index_offset: u32 }
//! data    ... file blobs ...
//! index   count × { uid: u32, offset: u32, size: u32, space: u32 }  (sorted by uid)
//! ```
//! Entries are addressed by `hash::path_id(path)`; names are not stored.

use crate::hash::path_id;
use crate::io::{Error, Reader, Result, Writer};

pub const MAGIC: u32 = u32::from_le_bytes(*b"PFDW");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WdfEntry {
    pub uid: u32,
    pub offset: u32,
    pub size: u32,
    /// Reserved slack after the blob (lets tools patch in place).
    pub space: u32,
}

pub struct Wdf {
    pub data: Vec<u8>,
    pub header_offset: u32,
    pub entries: Vec<WdfEntry>,
}

impl Wdf {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        Self::parse(std::fs::read(path)?)
    }

    pub fn parse(data: Vec<u8>) -> Result<Self> {
        let mut r = Reader::new(&data);
        let magic = r.u32()?;
        if magic != MAGIC {
            return Err(Error::Invalid(format!("bad WDF magic {magic:#x}")));
        }
        let count = r.i32()?;
        let header_offset = r.u32()?;
        let count = usize::try_from(count).map_err(|_| Error::Invalid("negative count".into()))?;

        let mut r = Reader::new(&data);
        r.bytes(header_offset as usize)?;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let e = WdfEntry {
                uid: r.u32()?,
                offset: r.u32()?,
                size: r.u32()?,
                space: r.u32()?,
            };
            if e.offset as u64 + e.size as u64 > data.len() as u64 {
                return Err(Error::Invalid(format!("entry {:#x} out of bounds", e.uid)));
            }
            entries.push(e);
        }
        Ok(Self {
            data,
            header_offset,
            entries,
        })
    }

    pub fn find_uid(&self, uid: u32) -> Option<&WdfEntry> {
        // The client binary-searches; tolerate unsorted archives anyway.
        match self.entries.binary_search_by_key(&uid, |e| e.uid) {
            Ok(i) => Some(&self.entries[i]),
            Err(_) => self.entries.iter().find(|e| e.uid == uid),
        }
    }

    pub fn find(&self, path: &str) -> Option<&WdfEntry> {
        self.find_uid(path_id(path))
    }

    pub fn entry_bytes(&self, e: &WdfEntry) -> &[u8] {
        &self.data[e.offset as usize..(e.offset + e.size) as usize]
    }

    pub fn read(&self, path: &str) -> Option<&[u8]> {
        self.find(path).map(|e| self.entry_bytes(e))
    }
}

/// Builds a new WDF from files. Layout matches TQ packers: header, blobs, sorted index.
#[derive(Default)]
pub struct WdfBuilder {
    files: Vec<(u32, Vec<u8>)>,
}

impl WdfBuilder {
    pub fn add(&mut self, path: &str, bytes: Vec<u8>) {
        self.add_uid(path_id(path), bytes);
    }

    pub fn add_uid(&mut self, uid: u32, bytes: Vec<u8>) {
        self.files.retain(|(u, _)| *u != uid);
        self.files.push((uid, bytes));
    }

    pub fn build(mut self) -> Vec<u8> {
        self.files.sort_by_key(|(u, _)| *u);
        let mut w = Writer::default();
        w.bytes(&[0; 12]);
        let mut index = Vec::with_capacity(self.files.len());
        for (uid, bytes) in &self.files {
            index.push(WdfEntry {
                uid: *uid,
                offset: w.buf.len() as u32,
                size: bytes.len() as u32,
                space: 0,
            });
            w.bytes(bytes);
        }
        let index_offset = w.buf.len() as u32;
        for e in &index {
            w.u32(e.uid);
            w.u32(e.offset);
            w.u32(e.size);
            w.u32(e.space);
        }
        w.buf[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        w.buf[4..8].copy_from_slice(&(index.len() as i32).to_le_bytes());
        w.buf[8..12].copy_from_slice(&index_offset.to_le_bytes());
        w.buf
    }
}

/// Rewrites an archive keeping blob placement, slack and index order, so that an
/// unmodified archive is reproduced byte-for-byte.
pub fn write_preserving(wdf: &Wdf) -> Vec<u8> {
    let mut out = wdf.data[..wdf.header_offset as usize].to_vec();
    let mut w = Writer::default();
    for e in &wdf.entries {
        w.u32(e.uid);
        w.u32(e.offset);
        w.u32(e.size);
        w.u32(e.space);
    }
    out.extend_from_slice(&w.buf);
    out.extend_from_slice(
        &wdf.data[(wdf.header_offset as usize + w.buf.len()).min(wdf.data.len())..],
    );
    out
}
