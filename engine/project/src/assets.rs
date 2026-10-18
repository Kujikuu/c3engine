//! The asset database: every file in the project's `assets/` with its sidecar
//! ([`AssetMeta`]: a stable id and import settings), and the file operations that keep the
//! sidecars and the project's references to the files in step (rename, move, delete to the
//! project's trash, import, new folder).
//!
//! Paths are relative to `assets/`, with `/`, as maps and settings name them.

use crate::{Error, Project, Result};
use c3_core::asset::{AssetId, AssetMeta, Importer, META_SUFFIX};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// The project folder that holds deleted assets until the deletion is undone.
const TRASH: &str = ".c3/trash";

/// Text files of the project that refer to assets by path, by extension.
const REFERRING: [&str; 4] = ["ron", "toml", "json", "lua"];

/// The project's assets and their sidecars.
#[derive(Debug, Default)]
pub struct AssetDb {
    root: PathBuf,
    /// Folders left out: another engine's content kept inside the assets (a copied TQ
    /// client), used as it is, with no sidecars.
    skip: Vec<String>,
    metas: BTreeMap<String, AssetMeta>,
    by_id: HashMap<AssetId, String>,
}

impl AssetDb {
    /// Reads the assets under `root`, giving a sidecar to each file that has none. Returns
    /// the database and what was wrong (unreadable sidecars get a fresh one).
    pub fn open(root: impl Into<PathBuf>) -> (Self, Vec<String>) {
        Self::open_without(root, Vec::new())
    }

    /// [`AssetDb::open`] without the folders `skip`.
    pub fn open_without(root: impl Into<PathBuf>, skip: Vec<String>) -> (Self, Vec<String>) {
        let mut db = Self {
            root: root.into(),
            skip,
            ..Self::default()
        };
        let mut problems = Vec::new();
        let mut files = Vec::new();
        walk(&db.root, "", &db.skip, &mut files);
        for rel in files {
            match db.load_meta(&rel) {
                Ok(meta) => db.insert(rel, meta),
                Err(e) => problems.push(e),
            }
        }
        (db, problems)
    }

    /// The database of `project`'s `assets/`, without a TQ client copied into them.
    pub fn of(project: &Project) -> (Self, Vec<String>) {
        let assets = project.assets_dir();
        let skip = project
            .tq_client()
            .and_then(|(_, dir)| {
                let rel = dir.strip_prefix(&assets).ok()?;
                Some(rel.to_string_lossy().replace('\\', "/"))
            })
            .into_iter()
            .collect();
        Self::open_without(assets, skip)
    }

    /// Whether `path` is one of the database's assets' places (not left out).
    pub fn covers(&self, path: &str) -> bool {
        !self.skip.iter().any(|s| skipped(path, s))
    }

    /// Reads every asset again (files added, changed or removed on disk); ids stay, as they
    /// live in the sidecars.
    pub fn rescan(&mut self) -> Vec<String> {
        let (db, problems) = Self::open_without(self.root.clone(), self.skip.clone());
        *self = db;
        problems
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every asset, by path.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &AssetMeta)> {
        self.metas.iter().map(|(p, m)| (p.as_str(), m))
    }

    pub fn len(&self) -> usize {
        self.metas.len()
    }

    pub fn is_empty(&self) -> bool {
        self.metas.is_empty()
    }

    pub fn meta(&self, path: &str) -> Option<&AssetMeta> {
        self.metas.get(path)
    }

    pub fn importer(&self, path: &str) -> Option<&Importer> {
        self.meta(path).map(|m| &m.import)
    }

    /// The path of the asset with id `id`.
    pub fn path_of(&self, id: &AssetId) -> Option<&str> {
        self.by_id.get(id).map(String::as_str)
    }

    /// The file of asset `path` on disk.
    pub fn file(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }

    /// Reads the asset at `path` again (its file or sidecar changed on disk); forgets it when
    /// the file is gone.
    pub fn refresh(&mut self, path: &str) -> std::result::Result<(), String> {
        self.remove(path);
        if !self.file(path).is_file() || path.ends_with(META_SUFFIX) {
            return Ok(());
        }
        let meta = self.load_meta(path)?;
        self.insert(path.to_owned(), meta);
        Ok(())
    }

    /// Sets how asset `path` is imported; returns the settings it had.
    pub fn set_importer(&mut self, path: &str, import: Importer) -> Result<Importer> {
        let meta = self.metas.get_mut(path).ok_or_else(|| missing(path))?;
        let before = std::mem::replace(&mut meta.import, import);
        let meta = meta.clone();
        self.write_meta(path, &meta)?;
        Ok(before)
    }

    /// Creates folder `path` (and its parents).
    pub fn create_folder(&self, path: &str) -> Result<()> {
        let dir = self.file(path);
        fs::create_dir_all(&dir).map_err(io(&dir))?;
        Ok(())
    }

    /// Whether `path` is a folder of the assets.
    pub fn is_folder(&self, path: &str) -> bool {
        self.file(path).is_dir()
    }

    /// Moves the file or folder at `from` to `to`, with its sidecars; ids stay. Doesn't
    /// replace anything.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        let (src, dst) = (self.file(from), self.file(to));
        if !src.exists() {
            return Err(missing(from));
        }
        if dst.exists() || self.file(&meta_path(to)).exists() {
            return Err(Error::Asset(format!("\"{to}\" already exists")));
        }
        if to == from || to.starts_with(&format!("{from}/")) {
            return Err(Error::Asset(format!("\"{from}\" can't go inside itself")));
        }
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(io(parent))?;
        }
        fs::rename(&src, &dst).map_err(io(&src))?;
        if src.is_file() || dst.is_file() {
            let meta = self.file(&meta_path(from));
            if meta.exists() {
                fs::rename(&meta, self.file(&meta_path(to))).map_err(io(&meta))?;
            }
        }
        let moved: Vec<String> = self
            .metas
            .keys()
            .filter(|p| *p == from || p.starts_with(&format!("{from}/")))
            .cloned()
            .collect();
        for old in moved {
            if let Some(meta) = self.remove(&old) {
                self.insert(format!("{to}{}", &old[from.len()..]), meta);
            }
        }
        Ok(())
    }

    /// Moves the file or folder at `path` to the project's trash (`trash` is the trash folder);
    /// returns where it went, for [`AssetDb::restore`].
    pub fn delete(&mut self, path: &str, trash: &Path) -> Result<PathBuf> {
        let src = self.file(path);
        if !src.exists() {
            return Err(missing(path));
        }
        let slot = (0..)
            .map(|n| trash.join(n.to_string()))
            .find(|p| !p.exists())
            .unwrap_or_else(|| trash.join("last"));
        let dst = slot.join(path);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(io(parent))?;
        }
        fs::rename(&src, &dst).map_err(io(&src))?;
        let meta = self.file(&meta_path(path));
        if meta.exists() {
            fs::rename(&meta, slot.join(meta_path(path))).map_err(io(&meta))?;
        }
        let gone: Vec<String> = self
            .metas
            .keys()
            .filter(|p| *p == path || p.starts_with(&format!("{path}/")))
            .cloned()
            .collect();
        for p in gone {
            self.remove(&p);
        }
        Ok(slot)
    }

    /// Puts back asset `path` deleted to trash slot `slot`.
    pub fn restore(&mut self, path: &str, slot: &Path) -> Result<()> {
        let dst = self.file(path);
        if dst.exists() {
            return Err(Error::Asset(format!("\"{path}\" already exists")));
        }
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(io(parent))?;
        }
        fs::rename(slot.join(path), &dst).map_err(io(&dst))?;
        let meta = slot.join(meta_path(path));
        if meta.exists() {
            fs::rename(&meta, self.file(&meta_path(path))).map_err(io(&meta))?;
        }
        // The slot's leftovers (empty folders) go.
        let _ = fs::remove_dir_all(slot);
        let mut files = Vec::new();
        if dst.is_dir() {
            walk(&dst, path, &self.skip, &mut files);
        } else {
            files.push(path.to_owned());
        }
        for rel in files {
            let meta = self.load_meta(&rel).map_err(Error::Asset)?;
            self.insert(rel, meta);
        }
        Ok(())
    }

    /// Copies file `source` into folder `folder` of the assets, under a free name; returns its
    /// path. A sidecar beside `source` comes too (so its id and settings stay).
    pub fn import(&mut self, source: &Path, folder: &str) -> Result<String> {
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| Error::Asset(format!("{} has no usable name", source.display())))?;
        let path = self.free_name(folder, name);
        let dst = self.file(&path);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(io(parent))?;
        }
        fs::copy(source, &dst).map_err(io(source))?;
        let sidecar = PathBuf::from(format!("{}{META_SUFFIX}", source.display()));
        let meta = match fs::read_to_string(&sidecar)
            .ok()
            .and_then(|t| toml::from_str::<AssetMeta>(&t).ok())
        {
            // An id already in use gets a fresh one.
            Some(m) if !self.by_id.contains_key(&m.id) => m,
            Some(m) => AssetMeta {
                id: new_id(),
                import: m.import,
            },
            None => default_meta(&path),
        };
        self.write_meta(&path, &meta)?;
        self.insert(path.clone(), meta);
        Ok(path)
    }

    /// `name` in `folder`, numbered when taken (`tree 2.png`).
    pub fn free_name(&self, folder: &str, name: &str) -> String {
        let join = |n: &str| {
            if folder.is_empty() {
                n.to_owned()
            } else {
                format!("{folder}/{n}")
            }
        };
        let (stem, ext) = match name.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
            _ => (name, String::new()),
        };
        (1..)
            .map(|n| {
                if n == 1 {
                    join(name)
                } else {
                    join(&format!("{stem} {n}{ext}"))
                }
            })
            .find(|p| !self.file(p).exists())
            .unwrap_or_else(|| join(name))
    }

    fn load_meta(&self, path: &str) -> std::result::Result<AssetMeta, String> {
        let file = self.file(&meta_path(path));
        let read = match fs::read_to_string(&file) {
            Ok(text) => Some(toml::from_str::<AssetMeta>(&text).map_err(|e| e.to_string())),
            Err(_) => None,
        };
        match read {
            Some(Ok(meta)) if !self.by_id.contains_key(&meta.id) => Ok(meta),
            other => {
                // A new file, a broken sidecar or a copied one (same id): a fresh sidecar.
                let mut meta = default_meta(path);
                if let Some(Ok(m)) = &other {
                    meta.import = m.import.clone();
                }
                self.write_meta(path, &meta)
                    .map_err(|e| format!("{path}: {e}"))?;
                match other {
                    Some(Err(e)) => Err(format!(
                        "{path}{META_SUFFIX} was unreadable ({e}); it was replaced"
                    )),
                    _ => Ok(meta),
                }
            }
        }
    }

    fn write_meta(&self, path: &str, meta: &AssetMeta) -> Result<()> {
        let text = toml::to_string(meta).map_err(|e| Error::Asset(e.to_string()))?;
        let file = self.file(&meta_path(path));
        fs::write(&file, text).map_err(io(&file))?;
        Ok(())
    }

    fn insert(&mut self, path: String, meta: AssetMeta) {
        self.by_id.insert(meta.id.clone(), path.clone());
        self.metas.insert(path, meta);
    }

    fn remove(&mut self, path: &str) -> Option<AssetMeta> {
        let meta = self.metas.remove(path)?;
        self.by_id.remove(&meta.id);
        Some(meta)
    }
}

impl Project {
    /// The folder deleted assets wait in.
    pub fn trash_dir(&self) -> PathBuf {
        self.root.join(TRASH)
    }

    /// Changes the project's references to asset `from` (a file, or a folder and all inside
    /// it) into `to`, in its maps, settings, tables and scripts. Returns the files changed.
    pub fn rename_references(&self, from: &str, to: &str) -> Result<Vec<PathBuf>> {
        let mut changed = Vec::new();
        let mut files = Vec::new();
        for dir in [
            self.maps_dir(),
            self.tables_dir(),
            self.scripts_dir(),
            self.server_dir(),
        ] {
            text_files(&dir, &mut files);
        }
        files.push(self.settings_file());
        for file in files {
            let Ok(text) = fs::read_to_string(&file) else {
                continue;
            };
            let next = replace_references(&text, from, to);
            if next != text {
                fs::write(&file, next).map_err(io(&file))?;
                changed.push(file);
            }
        }
        Ok(changed)
    }
}

/// `text` with the quoted paths `"from"` and `"from/…"` (and their `#frame`s) made `to`.
pub fn replace_references(text: &str, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let needle = format!("\"{from}");
    while let Some(at) = rest.find(&needle) {
        let after = &rest[at + needle.len()..];
        let whole = after.starts_with(['"', '/', '#']);
        out.push_str(&rest[..at]);
        if whole {
            out.push('"');
            out.push_str(to);
        } else {
            out.push_str(&needle);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

fn text_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            text_files(&path, out);
        } else if REFERRING
            .contains(&c3_core::asset::extension(&e.file_name().to_string_lossy()).as_str())
        {
            out.push(path);
        }
    }
}

/// The sidecar path of asset `path`.
pub fn meta_path(path: &str) -> String {
    format!("{path}{META_SUFFIX}")
}

fn default_meta(path: &str) -> AssetMeta {
    AssetMeta {
        id: new_id(),
        import: Importer::for_file(path),
    }
}

fn new_id() -> AssetId {
    AssetId(uuid::Uuid::new_v4().simple().to_string())
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Error + '_ {
    move |e| Error::Io(path.to_path_buf(), e)
}

fn missing(path: &str) -> Error {
    Error::Asset(format!("the project has no asset \"{path}\""))
}

fn skipped(path: &str, folder: &str) -> bool {
    folder.is_empty() || path == folder || path.starts_with(&format!("{folder}/"))
}

/// Files under `dir` (relative to the assets root, `prefix` being `dir`'s own path), without
/// sidecars, hidden folders or the folders `skip`.
fn walk(dir: &Path, prefix: &str, skip: &[String], out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let rel = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if skip.iter().any(|s| skipped(&rel, s)) {
            continue;
        }
        let path = e.path();
        if path.is_dir() {
            walk(&path, &rel, skip, out);
        } else if !name.ends_with(META_SUFFIX) {
            out.push(rel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_core::asset::{Grid, ImageImport};

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("c3-assets-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn files_get_sidecars_that_keep_their_ids() {
        let root = temp("ids");
        fs::create_dir_all(root.join("props")).unwrap();
        fs::write(root.join("props/tree.png"), b"png").unwrap();
        fs::write(root.join("theme.ogg"), b"ogg").unwrap();
        let (db, problems) = AssetDb::open(&root);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(db.len(), 2);
        assert!(root.join("props/tree.png.meta").exists());
        let id = db.meta("props/tree.png").unwrap().id.clone();
        let (again, _) = AssetDb::open(&root);
        assert_eq!(again.meta("props/tree.png").unwrap().id, id);
        assert_eq!(again.path_of(&id), Some("props/tree.png"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rename_move_delete_and_restore_keep_ids() {
        let root = temp("ops");
        let trash = root.join(".trash");
        fs::create_dir_all(root.join("a")).unwrap();
        fs::write(root.join("a/x.png"), b"png").unwrap();
        let (mut db, _) = AssetDb::open(&root);
        let id = db.meta("a/x.png").unwrap().id.clone();
        let sheet = Importer::Image(ImageImport {
            grid: Some(Grid::default()),
            ..ImageImport::default()
        });
        db.set_importer("a/x.png", sheet.clone()).unwrap();
        db.rename("a/x.png", "a/y.png").unwrap();
        db.rename("a", "b").unwrap();
        assert!(db.rename("b", "b/c").is_err());
        assert_eq!(db.path_of(&id), Some("b/y.png"));
        assert_eq!(db.importer("b/y.png"), Some(&sheet));
        let slot = db.delete("b", &trash).unwrap();
        assert!(db.is_empty() && !root.join("b").exists());
        db.restore("b", &slot).unwrap();
        assert_eq!(db.path_of(&id), Some("b/y.png"));
        let (again, _) = AssetDb::open(&root);
        assert_eq!(again.importer("b/y.png"), Some(&sheet));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn imports_take_free_names() {
        let root = temp("import");
        let src = temp("import-src").join("tree.png");
        fs::write(&src, b"png").unwrap();
        let (mut db, _) = AssetDb::open(&root);
        assert_eq!(db.import(&src, "props").unwrap(), "props/tree.png");
        assert_eq!(db.import(&src, "props").unwrap(), "props/tree 2.png");
        assert_eq!(db.len(), 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn references_follow_renames() {
        let text = r#"(source: "props/tree.png", frame: "props/tree.png#3", other: "props/tree.png.bak", dir: "props/trees/a.png")"#;
        assert_eq!(
            replace_references(text, "props/tree.png", "nature/oak.png"),
            r#"(source: "nature/oak.png", frame: "nature/oak.png#3", other: "props/tree.png.bak", dir: "props/trees/a.png")"#
        );
        assert_eq!(
            replace_references(text, "props", "art"),
            r#"(source: "art/tree.png", frame: "art/tree.png#3", other: "art/tree.png.bak", dir: "art/trees/a.png")"#
        );
    }
}
