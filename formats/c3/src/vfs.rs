//! Client virtual file system: loose files under the client root take priority,
//! then packed WDF archives (prefix-named archive first, then all), as the client does.

use crate::hash::{legacy_string_id, normalize_path};
use crate::wdf::Wdf;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

pub struct ClientFs {
    pub root: PathBuf,
    archives: Vec<(u32, Wdf)>,
}

impl ClientFs {
    /// Opens the client root and every `*.wdf` directly inside it.
    pub fn open(root: impl Into<PathBuf>) -> crate::Result<Self> {
        let root = root.into();
        let mut archives = Vec::new();
        for entry in std::fs::read_dir(&root)?.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("wdf")) {
                let name = path.file_name().unwrap().to_string_lossy().to_ascii_lowercase();
                archives.push((legacy_string_id(&name), Wdf::open(&path)?));
            }
        }
        Ok(Self { root, archives })
    }

    pub fn read(&self, path: &str) -> Option<Cow<'_, [u8]>> {
        let norm = normalize_path(path);
        let loose = self.root.join(Path::new(&norm));
        if let Ok(bytes) = std::fs::read(&loose) {
            return Some(Cow::Owned(bytes));
        }
        let id = legacy_string_id(&norm);
        let pkg = norm.split_once('/').map(|(p, _)| legacy_string_id(&format!("{p}.wdf")));
        let preferred = self.archives.iter().filter(|(a, _)| Some(*a) == pkg);
        for (_, a) in preferred.chain(self.archives.iter()) {
            if let Some(e) = a.find_uid(id) {
                return Some(Cow::Borrowed(a.entry_bytes(e)));
            }
        }
        None
    }
}
