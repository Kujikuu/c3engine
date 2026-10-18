//! The Project panel: a Unity-style asset browser over the project's files. A folder tree on the
//! left; the open folder (or search results) on the right as a thumbnail grid or a list.
//! Textures show real thumbnails, loaded lazily for what's on screen; double-click opens maps
//! and models.

use crate::app::{Studio, Workspace};
use crate::asset_ops::AssetOp;
use crate::icons;
use crate::maps::MapRef;
use crate::theme;
use crate::tq_plugin::{self, Client};
use c3_assets::Image;
use eframe::egui::{self, Color32, RichText, Sense, TextureHandle, vec2};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;

/// Thumbnails decoded per frame (keeps scrolling smooth).
const THUMBS_PER_FRAME: usize = 4;
/// Longest side of a thumbnail texture, in pixels.
const THUMB_PIXELS: u32 = 128;
/// Longest side of the Inspector preview.
const PREVIEW_PIXELS: u32 = 512;
/// Below this tile size the browser shows a list instead of a grid.
const LIST_BELOW: f32 = 40.0;
/// Most search results shown.
const MAX_RESULTS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssetKind {
    Folder,
    Map,
    Scene,
    Model,
    /// A glTF model of the project's own.
    Gltf,
    /// An effect in the engine's own format.
    Effect,
    /// An animation set: a glTF model as one of the game's looks.
    Character,
    Texture,
    Font,
    Sprites,
    Puzzle,
    Terrain,
    Package,
    Data,
    Sound,
    Other,
}

impl AssetKind {
    pub(crate) fn of(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.ends_with(c3_project::MAP_SUFFIX) {
            return Self::Scene;
        }
        let ext = lower.rsplit_once('.').map_or("", |(_, e)| e);
        tq_plugin::tq_kind(ext).unwrap_or(match ext {
            "dds" | "tga" | "png" | "jpg" | "jpeg" | "bmp" => Self::Texture,
            "gltf" | "glb" => Self::Gltf,
            "c3fx" => Self::Effect,
            "c3char" => Self::Character,
            "ttf" | "otf" => Self::Font,
            "wav" | "mp3" | "ogg" => Self::Sound,
            "txt" | "ron" | "toml" | "json" | "csv" | "lua" => Self::Data,
            _ => Self::Other,
        })
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Folder => icons::FOLDER,
            Self::Map | Self::Scene => icons::MAP_TRIFOLD,
            Self::Model | Self::Gltf => icons::CUBE,
            Self::Font => icons::TEXT_AA,
            Self::Effect => icons::SPARKLE,
            Self::Character => icons::PERSON_SIMPLE,
            Self::Texture => icons::IMAGE,
            Self::Sprites => icons::FILM_STRIP,
            Self::Puzzle => icons::PUZZLE_PIECE,
            Self::Terrain => icons::MOUNTAINS,
            Self::Package => icons::PACKAGE,
            Self::Data => icons::FILE_TEXT,
            Self::Sound => icons::FILE_AUDIO,
            Self::Other => icons::FILE,
        }
    }

    pub fn color(self) -> Color32 {
        match self {
            Self::Folder => Color32::from_rgb(214, 180, 96),
            Self::Map | Self::Scene => Color32::from_rgb(120, 190, 120),
            Self::Model | Self::Gltf => Color32::from_rgb(110, 165, 230),
            Self::Texture => Color32::from_rgb(196, 140, 220),
            Self::Sprites | Self::Puzzle | Self::Terrain => Color32::from_rgb(230, 160, 100),
            Self::Effect => Color32::from_rgb(240, 200, 90),
            Self::Character => Color32::from_rgb(110, 200, 190),
            Self::Package => Color32::from_rgb(180, 150, 110),
            Self::Data | Self::Sound | Self::Font | Self::Other => Color32::from_gray(170),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Folder => "Folder",
            Self::Map => "Map (DMap)",
            Self::Scene => "Engine scene",
            Self::Model => "Model / effect (C3)",
            Self::Gltf => "Model (glTF)",
            Self::Font => "Font",
            Self::Effect => "Effect (C3FX)",
            Self::Character => "Animation set",
            Self::Texture => "Texture",
            Self::Sprites => "Sprite set (ANI)",
            Self::Puzzle => "Ground tiles (PUL)",
            Self::Terrain => "Terrain object",
            Self::Package => "Package (WDF)",
            Self::Data => "Data",
            Self::Sound => "Sound",
            Self::Other => "File",
        }
    }

    /// Whether double-click opens it in the Scene view.
    fn opens(self) -> bool {
        matches!(
            self,
            Self::Map | Self::Scene | Self::Model | Self::Gltf | Self::Effect | Self::Character
        )
    }
}

pub(crate) struct AssetFile {
    /// Path relative to the root, with `/`.
    pub rel: String,
    pub kind: AssetKind,
    pub size: u64,
    /// Inside a package (WDF) rather than a loose file.
    pub packed: bool,
}

/// A folder tree the browser shows (the project's Assets, or a client used in place).
pub(crate) struct AssetRoot {
    pub label: String,
    pub dir: PathBuf,
    /// The project's TQ content lives here (maps and models open from it).
    pub is_content: bool,
    pub files: Vec<AssetFile>,
    /// Packed files named / packed entries in all (content roots with packages).
    pub packed: Option<(usize, usize)>,
    /// Folder (relative, `""` = root) → its subfolders, sorted.
    children: BTreeMap<String, Vec<String>>,
    /// Folder → indices of its files, sorted by name.
    files_in: BTreeMap<String, Vec<usize>>,
}

impl AssetRoot {
    pub fn scan(label: &str, dir: PathBuf, is_content: bool) -> Self {
        let mut files = Vec::new();
        let mut folders = vec![String::new()];
        let mut stack = vec![String::new()];
        while let Some(folder) = stack.pop() {
            let Ok(read) = std::fs::read_dir(dir.join(&folder)) else {
                continue;
            };
            for entry in read.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                // Sidecars, scene cells and the engine's hidden folders aren't assets.
                if name.starts_with('.')
                    || name.ends_with(c3_core::asset::META_SUFFIX)
                    || name.ends_with(c3_scene::native::DATA_SUFFIX)
                {
                    continue;
                }
                let rel = if folder.is_empty() {
                    name.clone()
                } else {
                    format!("{folder}/{name}")
                };
                match entry.metadata() {
                    Ok(meta) if meta.is_dir() => {
                        folders.push(rel.clone());
                        stack.push(rel);
                    }
                    Ok(meta) => files.push(AssetFile {
                        kind: AssetKind::of(&name),
                        rel,
                        size: meta.len(),
                        packed: false,
                    }),
                    Err(_) => {}
                }
            }
        }
        let mut root = Self {
            label: label.to_owned(),
            dir,
            is_content,
            files,
            packed: None,
            children: BTreeMap::new(),
            files_in: BTreeMap::new(),
        };
        root.index(folders);
        root
    }

    /// Adds files found inside packages, `(path, size)`; `total` = packed entries in all.
    pub fn with_packed(mut self, packed: Vec<(String, u64)>, total: usize) -> Self {
        self.packed = Some((packed.len(), total));
        self.files
            .extend(packed.into_iter().map(|(rel, size)| AssetFile {
                kind: AssetKind::of(&rel),
                rel,
                size,
                packed: true,
            }));
        let folders = self.children.keys().cloned().collect();
        self.index(folders);
        self
    }

    /// Rebuilds the folder and file lookups (folders of files are added as needed).
    fn index(&mut self, mut folders: Vec<String>) {
        for f in &self.files {
            let mut folder = parent(&f.rel);
            while !folder.is_empty() {
                folders.push(folder.to_owned());
                folder = parent(folder);
            }
        }
        folders.sort_by_key(|f| f.to_lowercase());
        folders.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for folder in folders {
            children.entry(folder.clone()).or_default();
            if !folder.is_empty() {
                children
                    .entry(parent(&folder).to_owned())
                    .or_default()
                    .push(folder);
            }
        }
        self.files.sort_by_key(|f| f.rel.to_lowercase());
        let mut files_in: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, f) in self.files.iter().enumerate() {
            files_in
                .entry(parent(&f.rel).to_owned())
                .or_default()
                .push(i);
        }
        self.children = children;
        self.files_in = files_in;
    }

    fn file(&self, rel: &str) -> Option<&AssetFile> {
        let folder = self.files_in.get(parent(rel))?;
        folder
            .iter()
            .map(|&i| &self.files[i])
            .find(|f| f.rel == rel)
    }
}

/// `a/b/c.dds` → `a/b`; `c.dds` → ``.
fn parent(rel: &str) -> &str {
    rel.rsplit_once('/').map_or("", |(p, _)| p)
}

fn file_name(rel: &str) -> &str {
    rel.rsplit_once('/').map_or(rel, |(_, n)| n)
}

/// An item in the browser: a folder or a file of a root.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AssetKey {
    pub root: usize,
    pub rel: String,
}

enum Thumb {
    Queued,
    Ready(TextureHandle),
    /// Not an image, or unreadable.
    None,
}

/// A large texture preview for the Inspector.
pub(crate) struct Preview {
    pub key: AssetKey,
    pub texture: Option<TextureHandle>,
    /// Original size in pixels.
    pub size: [u32; 2],
    /// One-line description (C3 contents etc.).
    pub detail: Option<String>,
}

pub(crate) struct Browser {
    pub roots: Vec<AssetRoot>,
    /// The open folder.
    pub folder: AssetKey,
    pub selected: Option<AssetKey>,
    pub search: String,
    /// Grid tile size (list below `LIST_BELOW`).
    pub tile: f32,
    thumbs: HashMap<AssetKey, Thumb>,
    queue: VecDeque<AssetKey>,
    pub preview: Option<Preview>,
}

impl Default for Browser {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            folder: AssetKey {
                root: 0,
                rel: String::new(),
            },
            selected: None,
            search: String::new(),
            tile: 72.0,
            thumbs: HashMap::new(),
            queue: VecDeque::new(),
            preview: None,
        }
    }
}

impl Browser {
    pub fn file(&self, key: &AssetKey) -> Option<&AssetFile> {
        self.roots.get(key.root)?.file(&key.rel)
    }

    pub fn path(&self, key: &AssetKey) -> Option<PathBuf> {
        Some(self.roots.get(key.root)?.dir.join(&key.rel))
    }

    /// Bytes of a file: loose from disk, packed through the client's packages.
    fn read(&self, key: &AssetKey, client: Option<&Client>) -> Option<Vec<u8>> {
        match self.file(key) {
            Some(f) if f.packed => client?.read(&key.rel),
            _ => std::fs::read(self.path(key)?).ok(),
        }
    }

    /// Decodes a few queued thumbnails.
    fn load_thumbs(&mut self, ctx: &egui::Context, client: Option<&Client>) {
        for _ in 0..THUMBS_PER_FRAME {
            let Some(key) = self.queue.pop_front() else {
                return;
            };
            let thumb = self
                .read(&key, client)
                .and_then(|b| decode_image(&b))
                .map(|img| Thumb::Ready(texture(ctx, &key.rel, &img, THUMB_PIXELS)))
                .unwrap_or(Thumb::None);
            self.thumbs.insert(key, thumb);
        }
        if !self.queue.is_empty() {
            ctx.request_repaint();
        }
    }

    fn thumb(&mut self, key: &AssetKey) -> Option<&TextureHandle> {
        match self.thumbs.entry(key.clone()).or_insert_with(|| {
            self.queue.push_back(key.clone());
            Thumb::Queued
        }) {
            Thumb::Ready(t) => Some(t),
            Thumb::Queued | Thumb::None => None,
        }
    }

    /// Kind of `key`: its file's, or a folder.
    pub fn kind(&self, key: &AssetKey) -> AssetKind {
        match self.file(key) {
            Some(f) => f.kind,
            None if self
                .roots
                .get(key.root)
                .is_some_and(|r| r.children.contains_key(&key.rel)) =>
            {
                AssetKind::Folder
            }
            None => AssetKind::of(&key.rel),
        }
    }

    /// Selects `key` and prepares its Inspector preview.
    fn select(&mut self, ctx: &egui::Context, key: AssetKey, client: Option<&Client>) {
        if self.selected.as_ref() == Some(&key) {
            return;
        }
        let kind = self.kind(&key);
        let mut preview = Preview {
            key: key.clone(),
            texture: None,
            size: [0, 0],
            detail: None,
        };
        match kind {
            AssetKind::Texture => {
                if let Some(img) = self.read(&key, client).and_then(|b| decode_image(&b)) {
                    preview.size = [img.width, img.height];
                    preview.texture = Some(texture(ctx, &key.rel, &img, PREVIEW_PIXELS));
                }
            }
            AssetKind::Model => {
                preview.detail = self.read(&key, client).map(|b| tq_plugin::c3_summary(&b));
            }
            _ => {}
        }
        self.preview = Some(preview);
        self.selected = Some(key);
    }
}

fn decode_image(bytes: &[u8]) -> Option<Image> {
    tq_plugin::decode_tq_image(bytes).or_else(|| c3_play::native::decode_image(bytes))
}

/// Uploads `img` scaled down to at most `max` pixels on its longest side.
fn texture(ctx: &egui::Context, name: &str, img: &Image, max: u32) -> TextureHandle {
    let scale = (max as f32 / img.width.max(img.height).max(1) as f32).min(1.0);
    let (w, h) = (
        ((img.width as f32 * scale) as u32).max(1),
        ((img.height as f32 * scale) as u32).max(1),
    );
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let sy = (y * img.height / h).min(img.height - 1);
        for x in 0..w {
            let sx = (x * img.width / w).min(img.width - 1);
            let i = ((sy * img.width + sx) * 4) as usize;
            rgba.extend_from_slice(&img.rgba[i..i + 4]);
        }
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
    ctx.load_texture(name, image, egui::TextureOptions::LINEAR)
}

/// `1.3 GB`, `420 KB`.
/// Lets assets dragged in the Project panel drop on folder `folder` (highlighted while one
/// hovers it): they move there.
fn drop_target(response: &egui::Response, folder: &AssetKey, pick: &mut Option<Pick>) {
    if response
        .dnd_hover_payload::<AssetKey>()
        .is_some_and(|k| k.root == folder.root)
    {
        response.ctx.layer_painter(response.layer_id).rect_stroke(
            response.rect,
            4.0,
            egui::Stroke::new(2.0, theme::ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    if let Some(what) = response.dnd_release_payload::<AssetKey>()
        && what.root == folder.root
        && *what != *folder
    {
        *pick = Some(Pick::Op(AssetOp::Move {
            what: (*what).clone(),
            into: folder.clone(),
        }));
    }
}

pub(crate) fn size_text(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.0} KB", b as f64 / 1024.0),
        b => format!("{b} B"),
    }
}

/// What the user did in the browser this frame.
enum Pick {
    Folder(AssetKey),
    Select(AssetKey),
    Open(AssetKey),
    /// A file operation on the project's assets.
    Op(AssetOp),
}

impl Studio {
    /// Rescans the project's folders for the Project panel.
    pub(crate) fn refresh_assets(&mut self) {
        let Some(project) = &self.project else {
            self.browser = Browser::default();
            return;
        };
        let mut roots = vec![AssetRoot::scan(
            "Assets",
            project.assets_dir(),
            project.content_in_project(),
        )];
        if let Some((_, dir)) = project.tq_client()
            && !project.content_in_project()
        {
            roots.push(AssetRoot::scan("Client (used in place)", dir, true));
        }
        // Packed files appear in their folders like loose ones.
        if let (Some(client), Some(i)) = (&self.client, roots.iter().position(|r| r.is_content)) {
            let (packed, total) = client.packed_files();
            let root = roots.remove(i);
            roots.insert(i, root.with_packed(packed, total));
        }
        if !project.map_names().is_empty() {
            roots.push(AssetRoot::scan("Scenes", project.maps_dir(), false));
        }
        let folder = self.browser.folder.clone();
        let keep =
            folder.root < roots.len() && roots[folder.root].children.contains_key(&folder.rel);
        // The selection stays, read again (its preview may have changed).
        let selected = self
            .browser
            .selected
            .clone()
            .filter(|k| roots.get(k.root).is_some_and(|r| r.file(&k.rel).is_some()));
        self.browser = Browser {
            roots,
            tile: self.browser.tile,
            ..Browser::default()
        };
        if keep {
            self.browser.folder = folder;
        }
        if let Some(key) = selected {
            let ctx = self.ctx.clone();
            self.browser.select(&ctx, key, self.client.as_ref());
        }
    }

    pub(crate) fn project_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.browser.load_thumbs(&ctx, self.client.as_ref());
        if self.browser.roots.is_empty() {
            theme::empty_state(
                ui,
                icons::FOLDER_OPEN,
                "No project open",
                "Open a project to browse its assets.",
            );
            return;
        }
        let mut pick = None;
        egui::Panel::left("project tree")
            .resizable(true)
            .default_size(200.0)
            .size_range(140.0..=420.0)
            .show(ui, |ui| {
                egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                    for root in 0..self.browser.roots.len() {
                        self.folder_tree(ui, root, String::new(), &mut pick);
                    }
                });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            self.browser_bar(ui, &mut pick);
            ui.separator();
            self.browser_items(ui, &mut pick);
        });
        match pick {
            Some(Pick::Folder(key)) => {
                self.browser.search.clear();
                self.browser.folder = key;
            }
            Some(Pick::Select(key)) => {
                self.browser.select(&ctx, key, self.client.as_ref());
                self.inspect_asset = true;
            }
            Some(Pick::Open(key)) => self.open_asset(&ctx, key),
            Some(Pick::Op(op)) => self.asset_op(op),
            None => {}
        }
        self.dropped_files(ui);
    }

    /// Files dragged in from the system: a hint while they hover the panel, an import into
    /// the open folder (of the project's assets) when dropped.
    fn dropped_files(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let (hovering, dropped, pointer) = ui.input(|i| {
            (
                !i.raw.hovered_files.is_empty(),
                i.raw.dropped_files.clone(),
                i.pointer.hover_pos(),
            )
        });
        let over = pointer.is_none_or(|p| rect.contains(p));
        let folder = self.drop_folder();
        if hovering && over {
            let text = match &folder {
                Some(f) if f.rel.is_empty() => "Drop to import into Assets".to_owned(),
                Some(f) => format!("Drop to import into {}", f.rel),
                None => "Open a project to import files".to_owned(),
            };
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::unique("asset drop"),
            ));
            painter.rect_filled(rect, 4.0, theme::ACCENT.gamma_multiply(0.18));
            painter.rect_stroke(
                rect.shrink(2.0),
                4.0,
                egui::Stroke::new(2.0, theme::ACCENT),
                egui::StrokeKind::Inside,
            );
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                theme::icon_text(icons::DOWNLOAD_SIMPLE, &text),
                egui::FontId::proportional(16.0),
                theme::TEXT,
            );
        }
        let files: Vec<std::path::PathBuf> = dropped
            .iter()
            .map(|f| f.path().to_path_buf())
            .filter(|p| !p.as_os_str().is_empty())
            .collect();
        if over
            && !files.is_empty()
            && let Some(folder) = folder
        {
            self.asset_op(AssetOp::ImportFiles(files, folder));
        }
    }

    /// Where dropped files go: the open folder when it is in the project's assets, else the
    /// assets' top.
    fn drop_folder(&self) -> Option<AssetKey> {
        let folder = &self.browser.folder;
        if self.editable_root(folder.root) {
            return Some(folder.clone());
        }
        let root = (0..self.browser.roots.len()).find(|&r| self.editable_root(r))?;
        Some(AssetKey {
            root,
            rel: String::new(),
        })
    }

    fn folder_tree(&self, ui: &mut egui::Ui, root: usize, folder: String, pick: &mut Option<Pick>) {
        let r = &self.browser.roots[root];
        let key = AssetKey {
            root,
            rel: folder.clone(),
        };
        let open_here = self.browser.folder == key;
        let children = r.children.get(&folder).map_or(&[][..], Vec::as_slice);
        let label = if folder.is_empty() {
            theme::icon_text(icons::FOLDER_OPEN, &r.label)
        } else {
            theme::icon_text(icons::FOLDER, file_name(&folder))
        };
        let id = ui.make_persistent_id(("tree", root, &folder));
        let editable = self.editable_root(root);
        let mut header = |ui: &mut egui::Ui| {
            let r = ui.add(egui::Button::selectable(open_here, label.clone()));
            if r.clicked() {
                *pick = Some(Pick::Folder(key.clone()));
            }
            if editable {
                drop_target(&r, &key, pick);
            }
        };
        if children.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                header(ui);
            });
            return;
        }
        egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            id,
            folder.is_empty(),
        )
        .show_header(ui, header)
        .body(|ui| {
            for child in children {
                self.folder_tree(ui, root, child.clone(), pick);
            }
        });
    }

    /// Breadcrumb, search and tile size.
    fn browser_bar(&mut self, ui: &mut egui::Ui, pick: &mut Option<Pick>) {
        ui.horizontal(|ui| {
            let folder = self.browser.folder.clone();
            let root = &self.browser.roots[folder.root];
            if ui.link(&root.label).clicked() {
                *pick = Some(Pick::Folder(AssetKey {
                    root: folder.root,
                    rel: String::new(),
                }));
            }
            let mut path = String::new();
            for part in folder.rel.split('/').filter(|p| !p.is_empty()) {
                if !path.is_empty() {
                    path.push('/');
                }
                path.push_str(part);
                ui.label(RichText::new(icons::CARET_RIGHT).color(theme::TEXT_DIM));
                if ui.link(part).clicked() {
                    *pick = Some(Pick::Folder(AssetKey {
                        root: folder.root,
                        rel: path.clone(),
                    }));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(target) = self.drop_folder() {
                    ui.menu_button(RichText::new(icons::PLUS).strong(), |ui| {
                        if ui
                            .button(theme::icon_text(icons::FOLDER_PLUS, "New Folder"))
                            .clicked()
                        {
                            *pick = Some(Pick::Op(AssetOp::NewFolder(target.clone())));
                            ui.close();
                        }
                        if ui
                            .button(theme::icon_text(icons::DOWNLOAD_SIMPLE, "Import Files…"))
                            .clicked()
                        {
                            *pick = Some(Pick::Op(AssetOp::Import(target.clone())));
                            ui.close();
                        }
                    })
                    .response
                    .on_hover_text("Add to the project's assets (or drop files here)");
                }
                ui.add(egui::Slider::new(&mut self.browser.tile, 24.0..=128.0).show_value(false))
                    .on_hover_text("Item size (smallest shows a list)");
                let icon = if self.browser.tile < LIST_BELOW {
                    icons::LIST_BULLETS
                } else {
                    icons::SQUARES_FOUR
                };
                ui.label(RichText::new(icon).color(theme::TEXT_DIM));
                if theme::icon_toggle(
                    ui,
                    false,
                    icons::ARROW_CLOCKWISE,
                    "Rescan the project's files",
                )
                .clicked()
                {
                    if let Some(doc) = &mut self.assets
                        && let Some(problem) = doc.db_mut().rescan().into_iter().next()
                    {
                        self.error = Some(problem);
                    }
                    self.refresh_assets();
                }
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.browser.search)
                        .hint_text(format!("{}  Search", icons::MAGNIFYING_GLASS))
                        .desired_width(200.0),
                );
                if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
                    search.request_focus();
                }
            });
        });
    }

    /// The open folder's subfolders and files, or the search results.
    fn browser_items(&mut self, ui: &mut egui::Ui, pick: &mut Option<Pick>) {
        let folder = self.browser.folder.clone();
        let root = &self.browser.roots[folder.root];
        let query = self.browser.search.trim().to_lowercase();
        let mut items: Vec<(AssetKey, AssetKind, u64)> = Vec::new();
        if query.is_empty() {
            for sub in root.children.get(&folder.rel).into_iter().flatten() {
                items.push((
                    AssetKey {
                        root: folder.root,
                        rel: sub.clone(),
                    },
                    AssetKind::Folder,
                    0,
                ));
            }
            for &i in root.files_in.get(&folder.rel).into_iter().flatten() {
                let f = &root.files[i];
                items.push((
                    AssetKey {
                        root: folder.root,
                        rel: f.rel.clone(),
                    },
                    f.kind,
                    f.size,
                ));
            }
        } else {
            items.extend(
                root.files
                    .iter()
                    .filter(|f| file_name(&f.rel).to_lowercase().contains(&query))
                    .take(MAX_RESULTS)
                    .map(|f| {
                        (
                            AssetKey {
                                root: folder.root,
                                rel: f.rel.clone(),
                            },
                            f.kind,
                            f.size,
                        )
                    }),
            );
        }
        if items.is_empty() {
            let (title, hint) = if query.is_empty() {
                (
                    "This folder is empty",
                    "Put files here and they appear, like in Unity.",
                )
            } else {
                ("Nothing found", "No file name contains that text.")
            };
            theme::empty_state(ui, icons::FOLDER_OPEN, title, hint);
            return;
        }
        let list = self.browser.tile < LIST_BELOW;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                if list {
                    for (key, kind, size) in &items {
                        self.list_row(ui, key, *kind, *size, pick);
                    }
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                        for (key, kind, _) in &items {
                            self.grid_tile(ui, key, *kind, pick);
                        }
                    });
                }
            });
    }

    fn grid_tile(
        &mut self,
        ui: &mut egui::Ui,
        key: &AssetKey,
        kind: AssetKind,
        pick: &mut Option<Pick>,
    ) {
        let s = self.browser.tile;
        let (rect, response) =
            ui.allocate_exact_size(vec2(s + 12.0, s + 34.0), Sense::click_and_drag());
        if !ui.is_rect_visible(rect) {
            return;
        }
        let selected = self.browser.selected.as_ref() == Some(key);
        let painter = ui.painter_at(rect);
        if selected {
            painter.rect_filled(rect, 4.0, ui.visuals().selection.bg_fill);
        } else if response.hovered() {
            painter.rect_filled(rect, 4.0, Color32::from_gray(66));
        }
        let image_rect = egui::Rect::from_min_size(rect.min + vec2(6.0, 4.0), vec2(s, s));
        let thumb = if kind == AssetKind::Texture {
            self.browser.thumb(key)
        } else {
            None
        };
        match thumb {
            Some(t) => {
                let size = t.size_vec2();
                let fit = (s / size.x.max(size.y)).min(4.0);
                let r = egui::Rect::from_center_size(image_rect.center(), size * fit);
                painter.image(
                    t.id(),
                    r,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                painter.text(
                    image_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    kind.icon(),
                    egui::FontId::proportional(s * 0.55),
                    kind.color(),
                );
            }
        }
        if self.browser.file(key).is_some_and(|f| f.packed) {
            painter.text(
                image_rect.right_top() + vec2(-2.0, 2.0),
                egui::Align2::RIGHT_TOP,
                icons::PACKAGE,
                egui::FontId::proportional(13.0),
                AssetKind::Package.color(),
            );
        }
        let name = file_name(&key.rel);
        let galley = painter.layout(
            name.to_owned(),
            egui::FontId::proportional(11.5),
            theme::TEXT,
            s + 8.0,
        );
        let text_pos = egui::pos2(
            rect.center().x - galley.size().x.min(s + 8.0) / 2.0,
            image_rect.bottom() + 3.0,
        );
        painter.galley(text_pos, galley, theme::TEXT);
        self.item_interaction(&response, key, kind, pick);
    }

    fn list_row(
        &mut self,
        ui: &mut egui::Ui,
        key: &AssetKey,
        kind: AssetKind,
        size: u64,
        pick: &mut Option<Pick>,
    ) {
        let selected = self.browser.selected.as_ref() == Some(key);
        let mut response = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new(kind.icon()).color(kind.color()));
            let r = ui.add(
                egui::Button::selectable(selected, file_name(&key.rel))
                    .sense(Sense::click_and_drag()),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if kind != AssetKind::Folder {
                    ui.label(
                        RichText::new(size_text(size))
                            .color(theme::TEXT_DIM)
                            .small(),
                    );
                }
                ui.label(RichText::new(kind.label()).color(theme::TEXT_DIM).small());
            });
            response = Some(r);
        });
        if let Some(r) = response {
            self.item_interaction(&r, key, kind, pick);
        }
    }

    fn item_interaction(
        &self,
        response: &egui::Response,
        key: &AssetKey,
        kind: AssetKind,
        pick: &mut Option<Pick>,
    ) {
        let hint = if kind == AssetKind::Folder {
            "Double-click to open the folder".to_owned()
        } else if kind.opens() {
            format!("{} · double-click to open it in the Scene", kind.label())
        } else {
            kind.label().to_owned()
        };
        let editable = self.editable_root(key.root);
        // Files drag anywhere (onto the map to place them); folders only within the assets.
        if kind != AssetKind::Folder {
            response.dnd_set_drag_payload(key.clone());
        } else if editable {
            response.dnd_set_drag_payload(key.clone());
            drop_target(response, key, pick);
        }
        let response = response
            .clone()
            .on_hover_text(format!("{}\n{hint}", key.rel));
        if response.double_clicked() {
            *pick = Some(if kind == AssetKind::Folder {
                Pick::Folder(key.clone())
            } else {
                Pick::Open(key.clone())
            });
        } else if response.clicked() {
            *pick = Some(Pick::Select(key.clone()));
        }
        let path = self.browser.path(key);
        response.context_menu(|ui| {
            if kind.opens()
                && ui
                    .button(theme::icon_text(icons::FOLDER_OPEN, "Open"))
                    .clicked()
            {
                *pick = Some(Pick::Open(key.clone()));
                ui.close();
            }
            if ui
                .button(theme::icon_text(icons::FOLDER, "Show in Explorer"))
                .clicked()
            {
                if let Some(p) = &path {
                    // Opening a file browser is a convenience; failing to is harmless.
                    let _ = std::process::Command::new("explorer")
                        .arg("/select,")
                        .arg(p)
                        .spawn();
                }
                ui.close();
            }
            if ui
                .button(theme::icon_text(icons::COPY, "Copy Path"))
                .clicked()
            {
                ui.ctx().copy_text(key.rel.clone());
                ui.close();
            }
            if kind == AssetKind::Scene {
                let name = file_name(&key.rel)
                    .trim_end_matches(c3_project::MAP_SUFFIX)
                    .to_owned();
                ui.separator();
                map_menu(ui, &name, pick);
                return;
            }
            if !editable {
                return;
            }
            ui.separator();
            if kind == AssetKind::Folder {
                if ui
                    .button(theme::icon_text(icons::FOLDER_PLUS, "New Folder"))
                    .clicked()
                {
                    *pick = Some(Pick::Op(AssetOp::NewFolder(key.clone())));
                    ui.close();
                }
                if ui
                    .button(theme::icon_text(icons::DOWNLOAD_SIMPLE, "Import Files…"))
                    .clicked()
                {
                    *pick = Some(Pick::Op(AssetOp::Import(key.clone())));
                    ui.close();
                }
            }
            if ui
                .button(theme::icon_text(icons::PENCIL_SIMPLE, "Rename…"))
                .clicked()
            {
                *pick = Some(Pick::Op(AssetOp::Rename(key.clone())));
                ui.close();
            }
            if ui
                .button(theme::icon_text(icons::TRASH, "Delete"))
                .on_hover_text("Moves it to the project's trash · Ctrl+Z puts it back")
                .clicked()
            {
                *pick = Some(Pick::Op(AssetOp::Delete(key.clone())));
                ui.close();
            }
        });
    }

    /// Shows content folder `rel` in the Project panel, or (`select`) the folder of file
    /// `rel` with that file selected.
    pub(crate) fn browse_to(&mut self, rel: &str, select: bool) {
        let rel = rel.replace('\\', "/");
        // The root holding `rel`, else the TQ content's.
        let holds = |r: &AssetRoot| r.file(&rel).is_some() || r.children.contains_key(&rel);
        let Some(root) = (self.browser.roots.iter().position(holds))
            .or_else(|| self.browser.roots.iter().position(|r| r.is_content))
        else {
            return;
        };
        let folder = if select {
            parent(&rel).to_owned()
        } else {
            rel.clone()
        };
        self.browser.folder = AssetKey { root, rel: folder };
        if select {
            let ctx = self.ctx.clone();
            self.browser
                .select(&ctx, AssetKey { root, rel }, self.client.as_ref());
            self.inspect_asset = true;
        }
        self.show_panel(crate::shell::Panel::Project);
    }

    /// Opens a map or model in the Scene view.
    fn open_asset(&mut self, ctx: &egui::Context, key: AssetKey) {
        self.browser.select(ctx, key.clone(), self.client.as_ref());
        self.inspect_asset = true;
        let Some(file) = self.browser.file(&key) else {
            return;
        };
        let root = &self.browser.roots[key.root];
        match file.kind {
            AssetKind::Map if root.is_content => {
                self.workspace = Workspace::Maps;
                self.inspect_asset = false;
                self.open_map(&MapRef::Client(key.rel.clone()));
            }
            AssetKind::Scene => {
                let name = file_name(&key.rel)
                    .trim_end_matches(c3_project::MAP_SUFFIX)
                    .to_owned();
                self.workspace = Workspace::Maps;
                self.inspect_asset = false;
                self.open_map(&MapRef::Project(name));
            }
            AssetKind::Effect => {
                self.inspect_asset = false;
                self.preview_effect = Some(key.rel.clone());
                self.show_panel(crate::shell::Panel::Preview);
                return;
            }
            AssetKind::Character => {
                self.inspect_asset = false;
                self.open_character(&key.rel);
                self.show_panel(crate::shell::Panel::Preview);
                return;
            }
            AssetKind::Gltf => {
                self.workspace = Workspace::Models;
                self.inspect_asset = false;
                self.open_model(&tq_plugin::ModelEntry {
                    id: 0,
                    path: key.rel.clone(),
                });
            }
            AssetKind::Model if root.is_content => {
                let entry = self
                    .models
                    .iter()
                    .find(|m| m.path.eq_ignore_ascii_case(&key.rel))
                    .cloned()
                    .unwrap_or(tq_plugin::ModelEntry {
                        id: 0,
                        path: key.rel.clone(),
                    });
                self.workspace = Workspace::Models;
                self.inspect_asset = false;
                self.open_model(&entry);
            }
            _ => {}
        }
        self.show_panel(crate::shell::Panel::Scene);
    }

    /// Inspector body for the selected asset.
    pub(crate) fn asset_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(key) = self.browser.selected.clone() else {
            return;
        };
        let kind = self.browser.kind(&key);
        let size = self.browser.file(&key).map_or(0, |f| f.size);
        let packed = self.browser.file(&key).is_some_and(|f| f.packed);
        let name = if key.rel.is_empty() {
            self.browser.roots[key.root].label.clone()
        } else {
            file_name(&key.rel).to_owned()
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(kind.icon()).size(28.0).color(kind.color()));
            ui.vertical(|ui| {
                ui.label(RichText::new(&name).strong().size(15.0));
                ui.label(RichText::new(kind.label()).color(theme::TEXT_DIM));
            });
        });
        ui.add_space(6.0);
        theme::section(ui, "File", |ui| {
            theme::kv(ui, "Path", &key.rel);
            if kind != AssetKind::Folder {
                theme::kv(ui, "Size", &size_text(size));
                theme::kv(
                    ui,
                    "Stored",
                    if packed {
                        "In a package (WDF)"
                    } else {
                        "Loose file"
                    },
                );
            }
            if key.rel.is_empty()
                && let Some((named, total)) = self.browser.roots[key.root].packed
            {
                theme::kv(ui, "Packed files", &format!("{named} named of {total}"));
                ui.label(
                    RichText::new(
                        "Packages keep only hashed names, so packed files show when the client \
                         refers to them (ANI frames, ini tables).",
                    )
                    .color(theme::TEXT_DIM)
                    .small(),
                );
            }
        });
        let image_size = self
            .browser
            .preview
            .as_ref()
            .filter(|p| p.key == key && p.texture.is_some())
            .map(|p| p.size);
        self.import_section(ui, key.root, &key.rel, image_size);
        if kind == AssetKind::Sound {
            self.sound_preview(ui, &key.rel);
        }
        let import = match self.asset_meta(key.root, &key.rel).map(|m| &m.import) {
            Some(c3_core::asset::Importer::Image(i)) => Some(i.clone()),
            _ => None,
        };
        let mut open = false;
        if let Some(preview) = self.browser.preview.as_ref().filter(|p| p.key == key) {
            if let Some(detail) = &preview.detail {
                theme::section(ui, "Contents", |ui| {
                    ui.label(detail);
                });
            }
            if let Some(texture) = &preview.texture {
                theme::section(ui, "Preview", |ui| {
                    theme::kv(
                        ui,
                        "Dimensions",
                        &format!("{} × {}", preview.size[0], preview.size[1]),
                    );
                    let max = ui.available_width();
                    let size = texture.size_vec2();
                    // Small images (pixel art, sheets) show larger, up to 4x.
                    let fit = (max / size.x).min(4.0).min(256.0 / size.y.max(1.0));
                    let shown = ui.add(egui::Image::new((texture.id(), size * fit)));
                    if let Some(import) = &import {
                        crate::asset_import::draw_grid(
                            ui.painter(),
                            shown.rect,
                            import,
                            preview.size,
                        );
                    }
                });
            }
        }
        if kind.opens() {
            ui.add_space(6.0);
            let button = egui::Button::new(
                RichText::new(theme::icon_text(icons::FOLDER_OPEN, "Open in Scene"))
                    .color(Color32::WHITE),
            )
            .fill(theme::ACCENT);
            open = ui.add(button).clicked();
        }
        if open {
            let ctx = ui.ctx().clone();
            self.open_asset(&ctx, key);
        }
    }
}

/// The menu of one of the project's maps: duplicate, rename, delete.
fn map_menu(ui: &mut egui::Ui, name: &str, pick: &mut Option<Pick>) {
    let items = [
        (
            icons::COPY_SIMPLE,
            "Duplicate",
            "A copy of the map, listed with an id of its own",
        ),
        (
            icons::PENCIL_SIMPLE,
            "Rename…",
            "Its entry in Project Settings > Maps follows",
        ),
        (
            icons::TRASH,
            "Delete",
            "Moves it to the project's trash · Ctrl+Z puts it back",
        ),
    ];
    for (i, (icon, label, tip)) in items.into_iter().enumerate() {
        if ui
            .button(theme::icon_text(icon, label))
            .on_hover_text(tip)
            .clicked()
        {
            let name = name.to_owned();
            *pick = Some(Pick::Op(match i {
                0 => AssetOp::DuplicateMap(name),
                1 => AssetOp::RenameMap(name),
                _ => AssetOp::DeleteMap(name),
            }));
            ui.close();
        }
    }
}
