//! Copying content (e.g. an original game client's files) into a project, with progress.

use crate::{Error, Result};
use std::path::{Path, PathBuf};

/// How far a copy has got.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CopyProgress {
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl CopyProgress {
    /// 0..1 by bytes.
    pub fn fraction(&self) -> f32 {
        if self.bytes_total == 0 {
            1.0
        } else {
            self.bytes_done as f32 / self.bytes_total as f32
        }
    }
}

/// Files under `src` whose top-level entry (file or folder name) is in `entries`
/// (case-insensitive), with their sizes.
fn collect(src: &Path, entries: &[&str]) -> Result<Vec<(PathBuf, u64)>> {
    let io = |p: &Path| {
        let p = p.to_path_buf();
        move |e| Error::Io(p, e)
    };
    let mut files = Vec::new();
    let mut stack: Vec<PathBuf> = std::fs::read_dir(src)
        .map_err(io(src))?
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            entries.iter().any(|want| want.eq_ignore_ascii_case(&name))
        })
        .map(|e| e.path())
        .collect();
    while let Some(path) = stack.pop() {
        let meta = std::fs::metadata(&path).map_err(io(&path))?;
        if meta.is_dir() {
            stack.extend(
                std::fs::read_dir(&path)
                    .map_err(io(&path))?
                    .flatten()
                    .map(|e| e.path()),
            );
        } else {
            files.push((path, meta.len()));
        }
    }
    files.sort();
    Ok(files)
}

/// Copies the top-level `entries` of `src` (files, or folders with everything inside) into
/// `dst`, keeping the layout. `progress` is called after each file; returning false stops
/// the copy and returns `Ok(false)`.
pub fn copy_entries(
    src: &Path,
    dst: &Path,
    entries: &[&str],
    mut progress: impl FnMut(CopyProgress) -> bool,
) -> Result<bool> {
    let files = collect(src, entries)?;
    let mut p = CopyProgress {
        files_total: files.len(),
        bytes_total: files.iter().map(|(_, n)| n).sum(),
        ..Default::default()
    };
    if !progress(p) {
        return Ok(false);
    }
    for (file, size) in files {
        let rel = file.strip_prefix(src).unwrap_or(&file);
        let target = dst.join(rel);
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::Io(dir.to_path_buf(), e))?;
        }
        std::fs::copy(&file, &target).map_err(|e| Error::Io(file.clone(), e))?;
        p.files_done += 1;
        p.bytes_done += size;
        if !progress(p) {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_only_the_listed_entries_with_progress() {
        let root = std::env::temp_dir().join(format!("c3-copy-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (src, dst) = (root.join("src"), root.join("dst"));
        std::fs::create_dir_all(src.join("Data/sub")).unwrap();
        std::fs::write(src.join("Data/sub/a.dds"), b"1234").unwrap();
        std::fs::write(src.join("c3.wdf"), b"12").unwrap();
        std::fs::write(src.join("Conquer.exe"), b"x").unwrap();
        let mut last = CopyProgress::default();
        let done = copy_entries(&src, &dst, &["data", "c3.wdf"], |p| {
            last = p;
            true
        })
        .unwrap();
        assert!(done);
        assert_eq!(std::fs::read(dst.join("Data/sub/a.dds")).unwrap(), b"1234");
        assert!(dst.join("c3.wdf").is_file() && !dst.join("Conquer.exe").exists());
        assert_eq!(
            (last.files_done, last.files_total, last.bytes_done),
            (2, 2, 6)
        );
        let stopped = copy_entries(&src, &root.join("dst2"), &["data", "c3.wdf"], |p| {
            p.files_done == 0
        })
        .unwrap();
        assert!(!stopped);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
