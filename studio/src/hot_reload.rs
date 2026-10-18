//! Hot reload: the Studio watches the project's `assets/` and, when files or their sidecars
//! change (saved from a paint program, or by the Studio's own Import section), reads them again
//! everywhere they show: the Project panel and Inspector, the Scene, Play, the Preview, the
//! Model view and sounds.

use crate::app::Studio;
use c3_core::asset::META_SUFFIX;
use eframe::egui;
use notify::{RecursiveMode, Watcher};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

/// Changes are read once the folder has been quiet this long (a save writes several times;
/// a drag in the Import section writes every frame).
const SETTLE: Duration = Duration::from_millis(150);

/// Changes under one folder, as asset paths.
pub(crate) struct AssetWatcher {
    root: PathBuf,
    events: Receiver<PathBuf>,
    ctx: egui::Context,
    /// Changes seen but not yet settled, and when the last one came.
    pending: RefCell<(BTreeSet<String>, Option<Instant>)>,
    // Kept alive while watching.
    _watcher: notify::RecommendedWatcher,
}

impl AssetWatcher {
    /// Watches `root`, asking `ctx` to repaint when something changes; `None` when the
    /// folder can't be watched (hot reload is then off).
    pub fn new(root: &Path, ctx: &egui::Context) -> Option<Self> {
        let (send, events) = channel();
        let repaint = ctx.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else { return };
                if matches!(event.kind, notify::EventKind::Access(_)) {
                    return;
                }
                for path in event.paths {
                    let _ = send.send(path);
                }
                repaint.request_repaint();
            })
            .ok()?;
        watcher.watch(root, RecursiveMode::Recursive).ok()?;
        Some(Self {
            root: root.to_path_buf(),
            events,
            ctx: ctx.clone(),
            pending: RefCell::new((BTreeSet::new(), None)),
            _watcher: watcher,
        })
    }

    /// Assets changed since the last call once the changes settled (a sidecar counts as its
    /// asset), without the engine's hidden folders.
    pub fn changed(&self) -> BTreeSet<String> {
        let mut pending = self.pending.borrow_mut();
        let fresh = self.fresh();
        if !fresh.is_empty() {
            pending.0.extend(fresh);
            pending.1 = Some(Instant::now());
        }
        match pending.1 {
            Some(at) if at.elapsed() >= SETTLE => {
                pending.1 = None;
                std::mem::take(&mut pending.0)
            }
            Some(at) => {
                self.ctx
                    .request_repaint_after(SETTLE.saturating_sub(at.elapsed()));
                BTreeSet::new()
            }
            None => BTreeSet::new(),
        }
    }

    fn fresh(&self) -> BTreeSet<String> {
        self.events
            .try_iter()
            .filter_map(|path| {
                let rel = path.strip_prefix(&self.root).ok()?;
                let rel = rel.to_string_lossy().replace('\\', "/");
                if rel.is_empty() || rel.split('/').any(|p| p.starts_with('.')) {
                    return None;
                }
                Some(
                    rel.strip_suffix(META_SUFFIX)
                        .map_or(rel.clone(), str::to_owned),
                )
            })
            .collect()
    }
}

impl Studio {
    /// Whether `path` was one of the project's assets.
    fn is_known_asset(&self, path: &str) -> bool {
        self.assets
            .as_ref()
            .is_some_and(|d| d.db().meta(path).is_some())
    }

    /// Reads every asset again: the database, the game's native assets, sounds and the
    /// Project panel.
    pub(crate) fn reload_all_assets(&mut self) {
        let mut problems = Vec::new();
        if let Some(doc) = &mut self.assets {
            problems.extend(doc.db_mut().rescan());
        }
        if let Some(native) = self.game.as_ref().and_then(|g| g.native()) {
            problems.extend(native.reload_all());
        }
        if let Some(audio) = &mut self.preview_audio {
            audio.forget(None);
        }
        if let Some(problem) = problems.into_iter().next() {
            self.error = Some(problem);
        }
        self.refresh_assets();
    }

    /// Reads changed assets again, everywhere they show.
    pub(crate) fn hot_reload(&mut self) {
        let Some(changed) = self.watcher.as_ref().map(AssetWatcher::changed) else {
            return;
        };
        if changed.is_empty() {
            return;
        }
        // A folder renamed or deleted names only itself: read everything again.
        let root = self.assets.as_ref().map(|d| d.db().root().to_path_buf());
        let whole = root.is_some_and(|root| {
            changed.iter().any(|p| {
                let file = root.join(p);
                file.is_dir() || (!file.exists() && !self.is_known_asset(p))
            })
        });
        if whole {
            self.reload_all_assets();
        }
        for path in changed.iter().filter(|_| !whole) {
            if let Some(doc) = &mut self.assets
                && let Err(e) = doc.db_mut().refresh(path)
            {
                self.error = Some(e);
            }
            if let Some(native) = self.game.as_ref().and_then(|g| g.native())
                && let Err(e) = native.reload(path)
            {
                self.error = Some(e);
            }
            if let Some(audio) = &mut self.preview_audio {
                audio.forget(Some(path));
            }
        }
        if !whole {
            self.refresh_assets();
        }
        if self.doc.is_some() {
            self.rebuild_live_scene();
        }
        if let Some(session) = &mut self.playing {
            session.reload_assets();
        }
        if let Some(stage) = &mut self.stage
            && let Err(e) = stage.reload_assets()
        {
            self.error = Some(format!("Can't play the effect: {e}"));
        }
        let open_model = self.model.as_ref().map(|m| m.entry.clone());
        if let Some(entry) = open_model.filter(|e| changed.contains(&e.path)) {
            self.reopen_model(&entry);
        }
        let shown = changed.len();
        let first = changed.into_iter().next().unwrap_or_default();
        self.notify(if shown == 1 {
            format!("Reloaded {first}")
        } else {
            format!("Reloaded {shown} assets")
        });
    }
}
