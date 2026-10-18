//! Studio application state: client/map loading, editing commands, shortcuts, and panel layout.

use crate::doc::MapDoc;
use crate::editor::{Editor, Tool};
use crate::map_view::{self, Camera, Overlays, ViewMode, colors};
use crate::theme;
use crate::tq_plugin::Client;
use c3_scene::CellPos;
use eframe::egui::{self, Key};
use std::time::{Duration, Instant};

const DEFAULT_CLIENT: &str = r"D:\Conquer2\Tools\eosource\client5065";
const NOTICE_DURATION: Duration = Duration::from_secs(4);

/// Something waiting on the "unsaved changes" prompt.
#[derive(Clone)]
pub(crate) enum Pending {
    OpenMap(String),
    Quit,
}

pub struct Studio {
    pub(crate) client_dir: String,
    pub(crate) client: Option<Client>,
    pub(crate) maps: Vec<String>,
    pub(crate) filter: String,
    pub(crate) doc: Option<MapDoc>,
    pub(crate) cam: Camera,
    pub(crate) overlays: Overlays,
    pub(crate) editor: Editor,
    pub(crate) hover: Option<CellPos>,
    pub(crate) streaming: bool,
    pub(crate) error: Option<String>,
    pub(crate) notice: Option<(String, Instant)>,
    pub(crate) pending: Option<Pending>,
    pub(crate) fit_pending: bool,
    allow_quit: bool,
    title: String,
}

impl Studio {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        theme::apply(&cc.egui_ctx);
        let args: Vec<String> = std::env::args().skip(1).collect();
        let flag = |name: &str| {
            args.iter()
                .position(|a| a == name)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        let client_dir = args
            .first()
            .filter(|a| !a.starts_with("--"))
            .cloned()
            .unwrap_or_else(|| DEFAULT_CLIENT.to_owned());
        let mut s = Self {
            client_dir,
            client: None,
            maps: Vec::new(),
            filter: String::new(),
            doc: None,
            cam: Camera {
                center: egui::Pos2::ZERO,
                zoom: 0.5,
                viewport: egui::Vec2::ZERO,
            },
            overlays: Overlays {
                grid: true,
                blocked: true,
                portals: true,
                objects: false,
            },
            editor: Editor::default(),
            hover: None,
            streaming: false,
            error: None,
            notice: None,
            pending: None,
            fit_pending: false,
            allow_quit: false,
            title: String::new(),
        };
        s.open_client();
        if let Some(m) = flag("--map") {
            s.load_map(&m);
        }
        if let Some(z) = flag("--zoom").and_then(|z| z.parse().ok()) {
            s.fit_pending = false;
            s.cam.zoom = z;
        }
        s
    }

    // ---- documents ----

    pub(crate) fn open_client(&mut self) {
        match Client::open(&self.client_dir) {
            Ok(client) => {
                self.maps = client.list_maps();
                self.client = Some(client);
                self.error = None;
            }
            Err(e) => {
                self.client = None;
                self.maps.clear();
                self.error = Some(format!("Can't open client folder: {e}"));
            }
        }
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.doc.as_ref().is_some_and(MapDoc::is_dirty)
    }

    /// Opens a map, asking first if the current one has unsaved changes.
    pub(crate) fn open_map(&mut self, path: &str) {
        if self.doc.as_ref().is_some_and(|d| d.path == path) {
            return;
        }
        if self.is_dirty() {
            self.pending = Some(Pending::OpenMap(path.to_owned()));
        } else {
            self.load_map(path);
        }
    }

    fn load_map(&mut self, path: &str) {
        let Some(client) = &self.client else { return };
        match client.import_map(path) {
            Ok(imported) => {
                let doc = MapDoc::new(path, imported.scene, imported.warnings);
                self.cam.center = doc.bounds().center();
                self.doc = Some(doc);
                self.fit_pending = true;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub(crate) fn save(&mut self) -> bool {
        let (Some(client), Some(doc)) = (&self.client, &mut self.doc) else {
            return false;
        };
        doc.end_stroke();
        match client.save_map(&doc.scene, &doc.path) {
            Ok(path) => {
                doc.mark_saved();
                self.notify(format!("Saved {}", path.display()));
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    /// Resolves the unsaved-changes prompt.
    pub(crate) fn resolve_pending(&mut self, save: bool, ctx: &egui::Context) {
        let Some(p) = self.pending.take() else { return };
        if save && !self.save() {
            return; // keep the user's work if saving failed
        }
        match p {
            Pending::OpenMap(path) => self.load_map(&path),
            Pending::Quit => {
                self.allow_quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    pub(crate) fn undo(&mut self) {
        if let Some(d) = &mut self.doc {
            d.undo();
        }
    }

    pub(crate) fn redo(&mut self) {
        if let Some(d) = &mut self.doc {
            d.redo();
        }
    }

    pub(crate) fn notify(&mut self, msg: String) {
        self.error = None;
        self.notice = Some((msg, Instant::now()));
    }

    pub(crate) fn active_notice(&self) -> Option<&str> {
        self.notice
            .as_ref()
            .filter(|(_, t)| t.elapsed() < NOTICE_DURATION)
            .map(|(m, _)| m.as_str())
    }

    pub(crate) fn filtered_maps(&self) -> Vec<String> {
        let f = self.filter.to_ascii_lowercase();
        self.maps
            .iter()
            .filter(|m| m.to_ascii_lowercase().contains(&f))
            .cloned()
            .collect()
    }

    // ---- input ----

    pub(crate) fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        // Command shortcuts work even while a text field has focus.
        let (undo, redo, save) = ctx.input(|i| {
            let cmd = i.modifiers.command;
            let z = cmd && i.key_pressed(Key::Z);
            (
                z && !i.modifiers.shift,
                (z && i.modifiers.shift) || (cmd && i.key_pressed(Key::Y)),
                cmd && i.key_pressed(Key::S),
            )
        });
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        if save {
            self.save();
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        ctx.input(|i| {
            if i.modifiers.command {
                return;
            }
            let o = &mut self.overlays;
            for (key, flag) in [
                (Key::G, &mut o.grid),
                (Key::B, &mut o.blocked),
                (Key::P, &mut o.portals),
                (Key::O, &mut o.objects),
            ] {
                if i.key_pressed(key) {
                    *flag ^= true;
                }
            }
            for (key, tool) in [
                (Key::Num1, Tool::Inspect),
                (Key::Num2, Tool::Block),
                (Key::Num3, Tool::Clear),
                (Key::Num4, Tool::Portal),
            ] {
                if i.key_pressed(key) {
                    self.editor.tool = tool;
                }
            }
            if i.key_pressed(Key::OpenBracket) {
                self.editor.grow_brush(-1);
            }
            if i.key_pressed(Key::CloseBracket) {
                self.editor.grow_brush(1);
            }
            if i.key_pressed(Key::F) {
                self.fit_pending = true;
            }
            if i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals) {
                self.cam.zoom_by(1.25);
            }
            if i.key_pressed(Key::Minus) {
                self.cam.zoom_by(0.8);
            }
            let pan = 600.0 * i.stable_dt / self.cam.zoom;
            if i.key_down(Key::ArrowLeft) || i.key_down(Key::A) {
                self.cam.center.x -= pan;
            }
            if i.key_down(Key::ArrowRight) || i.key_down(Key::D) {
                self.cam.center.x += pan;
            }
            if i.key_down(Key::ArrowUp) || i.key_down(Key::W) {
                self.cam.center.y -= pan;
            }
            if i.key_down(Key::ArrowDown) || i.key_down(Key::S) {
                self.cam.center.y += pan;
            }
        });
        // Step through the filtered map list with PageUp/PageDown.
        let step =
            ctx.input(|i| i.key_pressed(Key::PageDown) as i32 - i.key_pressed(Key::PageUp) as i32);
        if step != 0 {
            let list = self.filtered_maps();
            if !list.is_empty() {
                let cur = self
                    .doc
                    .as_ref()
                    .and_then(|d| list.iter().position(|m| *m == d.path));
                let next = cur.map_or(0, |c| {
                    (c as i32 + step).rem_euclid(list.len() as i32) as usize
                });
                self.open_map(&list[next]);
            }
        }
    }

    /// Intercepts window close while there are unsaved changes.
    fn guard_quit(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.is_dirty() && !self.allow_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = match &self.doc {
            Some(d) => format!(
                "{}{} — C3 Studio",
                if d.is_dirty() { "● " } else { "" },
                d.name()
            ),
            None => "C3 Studio".to_owned(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn viewport(&mut self, ui: &mut egui::Ui) {
        let (Some(client), Some(doc)) = (&self.client, &mut self.doc) else {
            theme::empty_state(ui, self.client.is_some());
            return;
        };
        if self.fit_pending && self.cam.viewport.x > 1.0 {
            self.cam.fit(doc.bounds());
            self.fit_pending = false;
        }
        let tool = self.editor.tool;
        let mode = ViewMode {
            pan_with_primary: tool == Tool::Inspect,
            brush: tool.uses_brush().then(|| {
                let color = if tool == Tool::Block {
                    colors::BRUSH_BLOCK
                } else {
                    colors::BRUSH_CLEAR
                };
                (self.editor.brush, color)
            }),
            dragging_portal: self.editor.dragging_portal(),
        };
        let viewport_rect = ui.max_rect();
        let r = map_view::show(ui, client, doc, &mut self.cam, self.overlays, mode);
        self.hover = r.pointer.cell;
        self.streaming = r.streaming;
        if let Some(msg) = self.editor.handle(doc, &r.pointer) {
            self.notify(msg);
        }
        if self.fit_pending {
            ui.ctx().request_repaint();
        }
        self.tool_palette(ui.ctx(), viewport_rect);
    }
}

impl eframe::App for Studio {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.guard_quit(&ctx);
        self.shortcuts(&ctx);
        self.update_title(&ctx);

        egui::Panel::top("toolbar")
            .exact_size(36.0)
            .show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status")
            .exact_size(26.0)
            .show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("maps")
            .default_size(220.0)
            .size_range(160.0..=400.0)
            .show(ui, |ui| self.map_list(ui));
        egui::Panel::right("inspector")
            .default_size(270.0)
            .size_range(220.0..=440.0)
            .show(ui, |ui| self.inspector(ui));
        egui::CentralPanel::no_frame().show(ui, |ui| self.viewport(ui));

        self.unsaved_prompt(&ctx);
        if self.notice.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}
