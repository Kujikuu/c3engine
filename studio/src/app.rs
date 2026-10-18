//! Studio application state: client/map loading, shortcuts, and panel layout.

use crate::map_view::{self, Camera, MapDoc, Overlays};
use crate::theme;
use c3_formats_tq::vfs::ClientFs;
use eframe::egui::{self, Key};

const DEFAULT_CLIENT: &str = r"D:\Conquer2\Tools\eosource\client5065";

pub struct Studio {
    pub(crate) client_dir: String,
    pub(crate) fs: Option<ClientFs>,
    pub(crate) maps: Vec<String>,
    pub(crate) filter: String,
    pub(crate) doc: Option<MapDoc>,
    pub(crate) cam: Camera,
    pub(crate) overlays: Overlays,
    pub(crate) hover: Option<(i32, i32)>,
    pub(crate) streaming: bool,
    pub(crate) error: Option<String>,
    pub(crate) fit_pending: bool,
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
            fs: None,
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
            hover: None,
            streaming: false,
            error: None,
            fit_pending: false,
        };
        s.open_client();
        if let Some(m) = flag("--map") {
            s.open_map(&m);
        }
        if let Some(z) = flag("--zoom").and_then(|z| z.parse().ok()) {
            s.fit_pending = false;
            s.cam.zoom = z;
        }
        s
    }

    pub(crate) fn open_client(&mut self) {
        match ClientFs::open(&self.client_dir) {
            Ok(fs) => {
                let dir = std::path::Path::new(&self.client_dir)
                    .join("map")
                    .join("map");
                self.maps = std::fs::read_dir(dir)
                    .map(|rd| {
                        rd.flatten()
                            .filter_map(|e| e.file_name().into_string().ok())
                            .filter(|n| n.to_ascii_lowercase().ends_with(".dmap"))
                            .map(|n| format!("map/map/{n}"))
                            .collect()
                    })
                    .unwrap_or_default();
                self.maps.sort_by_key(|m| m.to_ascii_lowercase());
                self.fs = Some(fs);
                self.error = None;
            }
            Err(e) => {
                self.fs = None;
                self.maps.clear();
                self.error = Some(format!("Can't open client folder: {e}"));
            }
        }
    }

    pub(crate) fn open_map(&mut self, path: &str) {
        let Some(fs) = &self.fs else { return };
        match MapDoc::load(fs, path) {
            Ok(doc) => {
                self.cam.center = doc.bg_rect().center();
                self.doc = Some(doc);
                self.fit_pending = true;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub(crate) fn filtered_maps(&self) -> Vec<String> {
        let f = self.filter.to_ascii_lowercase();
        self.maps
            .iter()
            .filter(|m| m.to_ascii_lowercase().contains(&f))
            .cloned()
            .collect()
    }

    pub(crate) fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        ctx.input(|i| {
            let o = &mut self.overlays;
            if i.key_pressed(Key::G) {
                o.grid ^= true;
            }
            if i.key_pressed(Key::B) {
                o.blocked ^= true;
            }
            if i.key_pressed(Key::P) {
                o.portals ^= true;
            }
            if i.key_pressed(Key::O) {
                o.objects ^= true;
            }
            if i.key_pressed(Key::F) && !i.modifiers.command {
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
                let next = match cur {
                    Some(c) => (c as i32 + step).rem_euclid(list.len() as i32) as usize,
                    None => 0,
                };
                self.open_map(&list[next]);
            }
        }
    }
}

impl eframe::App for Studio {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);

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

        egui::CentralPanel::no_frame().show(ui, |ui| match (&self.fs, &mut self.doc) {
            (Some(fs), Some(doc)) => {
                if self.fit_pending && self.cam.viewport.x > 1.0 {
                    self.cam.fit(doc.bg_rect());
                    self.fit_pending = false;
                }
                let r = map_view::show(ui, fs, doc, &mut self.cam, self.overlays);
                self.hover = r.hover_cell;
                self.streaming = r.streaming;
                if self.fit_pending {
                    ctx.request_repaint();
                }
            }
            _ => theme::empty_state(ui, self.fs.is_some()),
        });
    }
}
