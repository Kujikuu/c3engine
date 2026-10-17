//! ANI: text index mapping a title to an ordered list of frame image paths.
//! Port of the parser in `3DBaseCode/Ani.cpp`.
//!
//! ```text
//! [Title]
//! FrameAmount=2
//! Frame0=data/pic/a/0.dds
//! Frame1=data/pic/a/1.dds
//! ```
//! Text is GBK, so titles and paths are kept as raw bytes. `//` lines are comments.
//! Lookups are by exact title, first match wins (as in the client).

use crate::io::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AniSection {
    pub title: Vec<u8>,
    pub frames: Vec<Vec<u8>>,
}

impl AniSection {
    pub fn title_str(&self) -> String {
        String::from_utf8_lossy(&self.title).into_owned()
    }

    pub fn frame_str(&self, i: usize) -> Option<String> {
        self.frames.get(i).map(|f| String::from_utf8_lossy(f).into_owned())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ani {
    pub sections: Vec<AniSection>,
}

/// Client cap on frames per section (`_MAX_FRAME`); FrameAmount is taken modulo this.
pub const MAX_FRAME: usize = 1024;

fn trim(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|c| !c.is_ascii_whitespace()).unwrap_or(s.len());
    let end = s.iter().rposition(|c| !c.is_ascii_whitespace()).map_or(start, |e| e + 1);
    &s[start..end]
}

fn parse_usize(s: &[u8]) -> Option<usize> {
    std::str::from_utf8(s).ok()?.trim().parse().ok()
}

impl Ani {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        Self::parse(&std::fs::read(path)?)
    }

    pub fn get(&self, title: &str) -> Option<&AniSection> {
        self.sections.iter().find(|s| s.title == title.as_bytes())
    }

    /// Lenient parse: a section with a missing or short frame list keeps the frames it has.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut sections: Vec<AniSection> = Vec::new();
        let mut expected = 0usize;
        for (n, raw) in data.split(|&c| c == b'\n').enumerate() {
            let line = trim(raw);
            if line.is_empty() || line.starts_with(b"//") {
                continue;
            }
            if line[0] == b'[' {
                let end = line.iter().position(|&c| c == b']').unwrap_or(line.len());
                sections.push(AniSection { title: line[1..end].to_vec(), frames: Vec::new() });
                expected = 0;
                continue;
            }
            let Some(eq) = line.iter().position(|&c| c == b'=') else { continue };
            let (key, val) = (trim(&line[..eq]), trim(&line[eq + 1..]));
            let Some(sec) = sections.last_mut() else {
                return Err(Error::Invalid(format!("line {}: key before any section", n + 1)));
            };
            if key.eq_ignore_ascii_case(b"FrameAmount") {
                expected = parse_usize(val).unwrap_or(0) % MAX_FRAME;
                sec.frames = vec![Vec::new(); expected];
            } else if key.len() > 5 && key[..5].eq_ignore_ascii_case(b"Frame") {
                if let Some(i) = parse_usize(&key[5..]) {
                    if i < expected {
                        sec.frames[i] = val.to_vec();
                    }
                }
            }
        }
        // Drop trailing empty frame slots from short sections.
        for s in &mut sections {
            while s.frames.last().is_some_and(|f| f.is_empty()) {
                s.frames.pop();
            }
        }
        Ok(Self { sections })
    }

    /// Canonical output (CRLF, blank line between sections), as TQ tools write it.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, s) in self.sections.iter().enumerate() {
            if i > 0 {
                out.extend_from_slice(b"\r\n");
            }
            out.push(b'[');
            out.extend_from_slice(&s.title);
            out.extend_from_slice(b"]\r\n");
            out.extend_from_slice(format!("FrameAmount={}\r\n", s.frames.len()).as_bytes());
            for (j, f) in s.frames.iter().enumerate() {
                out.extend_from_slice(format!("Frame{j}=").as_bytes());
                out.extend_from_slice(f);
                out.extend_from_slice(b"\r\n");
            }
        }
        out
    }
}
