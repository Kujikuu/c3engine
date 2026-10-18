//! Studio application state: client/map loading, editing commands, shortcuts, and panel layout.

use crate::doc::MapDoc;
use crate::editor::{Editor, Tool};
use crate::gpu::GpuView;
use crate::home::{ImportJob, ProjectDialog};
use crate::map_view::{self, Camera, Overlays, ViewMode, colors};
use crate::maps::{self, MapRef};
use crate::model_view::{ModelDoc, OrbitCamera};
use crate::theme;
use crate::tq_plugin::{Client, ModelEntry};
use c3_project::{Project, RecentProjects};
use c3_render::ground::GroundCache;
use c3_scene::CellPos;
use eframe::egui::{self, Key};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const NOTICE_DURATION: Duration = Duration::from_secs(4);

/// Top-level editor area.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Workspace {
    Maps,
    Models,
}

/// Something waiting on the "unsaved changes" prompt.
#[derive(Clone)]
pub(crate) enum Pending {
    OpenMap(MapRef),
    OpenProject(PathBuf),
    CloseProject,
    Quit,
}

pub struct Studio {
    /// The open project; `None` shows the start page.
    pub(crate) project: Option<Project>,
    pub(crate) recent: RecentProjects,
    pub(crate) dialog: Option<ProjectDialog>,
    /// A client being copied into a new project.
    pub(crate) import_job: Option<ImportJob>,
    /// The project's TQ client source, if it has one.
    pub(crate) client: Option<Client>,
    pub(crate) workspace: Workspace,
    /// The content's map files, then engine-format scenes.
    pub(crate) maps: Vec<MapRef>,
    pub(crate) models: Vec<ModelEntry>,
    pub(crate) model: Option<ModelDoc>,
    pub(crate) orbit: OrbitCamera,
    pub(crate) filter: String,
    pub(crate) doc: Option<MapDoc>,
    pub(crate) cam: Camera,
    pub(crate) overlays: Overlays,
    pub(crate) editor: Editor,
    /// Engine renderer; `None` if eframe isn't running on wgpu.
    pub(crate) gpu: Option<GpuView>,
    pub(crate) ground: GroundCache,
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
        let project_arg = args.first().filter(|a| !a.starts_with("--")).cloned();
        let mut s = Self {
            project: None,
            recent: RecentProjects::load_default(),
            dialog: None,
            import_job: None,
            client: None,
            workspace: Workspace::Maps,
            maps: Vec::new(),
            models: Vec::new(),
            model: None,
            orbit: OrbitCamera::default(),
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
            gpu: cc.wgpu_render_state.as_ref().map(GpuView::new),
            ground: GroundCache::default(),
            hover: None,
            streaming: false,
            error: None,
            notice: None,
            pending: None,
            fit_pending: false,
            allow_quit: false,
            title: String::new(),
        };
        if let Some(path) = project_arg {
            s.open_project(std::path::Path::new(&path));
        }
        if args.iter().any(|a| a == "--new") {
            s.show_new_project();
        }
        if args.iter().any(|a| a == "--import") {
            s.show_import_conquer(flag("--import").as_deref());
        }
        if let Some(m) = flag("--map") {
            s.load_map(&MapRef::parse(&m));
        }
        if let Some(file) = flag("--exec") {
            s.run_command_file(&file);
        }
        if let Some(id) = flag("--model").and_then(|m| m.parse::<u64>().ok()) {
            s.workspace = Workspace::Models;
            match s.models.iter().find(|e| e.id == id).cloned() {
                Some(entry) => s.open_model(&entry),
                None => s.error = Some(format!("Model {id} is not in 3dobj.ini")),
            }
        }
        if let Some(z) = flag("--zoom").and_then(|z| z.parse().ok()) {
            s.fit_pending = false;
            s.cam.zoom = z;
        }
        s
    }

    // ---- documents ----

    /// Opens the project's TQ client source (models and not-yet-imported maps come from it),
    /// then lists the maps.
    pub(crate) fn open_client(&mut self) {
        self.client = None;
        self.maps.clear();
        self.models.clear();
        if let Some((_, dir)) = self.project.as_ref().and_then(Project::tq_client) {
            match Client::open(&dir.display().to_string()) {
                Ok(client) => {
                    self.models = client.list_models();
                    self.client = Some(client);
                }
                Err(e) => self.error = Some(format!("Can't open the project's client folder: {e}")),
            }
        }
        self.refresh_maps();
    }

    /// Re-lists the maps.
    pub(crate) fn refresh_maps(&mut self) {
        self.maps = match &self.project {
            Some(project) => maps::list_maps(project, self.client.as_ref()),
            None => Vec::new(),
        };
    }

    /// Closes the open map and model (without asking).
    pub(crate) fn close_documents(&mut self) {
        self.doc = None;
        self.model = None;
        self.filter.clear();
        self.hover = None;
        self.editor = Editor::default();
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.doc.as_ref().is_some_and(MapDoc::is_dirty)
    }

    /// Opens a map, asking first if the current one has unsaved changes.
    pub(crate) fn open_map(&mut self, map: &MapRef) {
        if self.doc.as_ref().is_some_and(|d| &d.map == map) {
            return;
        }
        if self.is_dirty() {
            self.pending = Some(Pending::OpenMap(map.clone()));
        } else {
            self.load_map(map);
        }
    }

    fn load_map(&mut self, map: &MapRef) {
        let loaded = match map {
            MapRef::Project(name) => match &self.project {
                Some(project) => c3_scene::native::load_scene(&project.map_file(name))
                    .map(|scene| (scene, Vec::new()))
                    .map_err(|e| e.to_string()),
                None => return,
            },
            MapRef::Client(path) => match &self.client {
                Some(client) => client
                    .import_map(path)
                    .map(|imported| (imported.scene, imported.warnings)),
                None => return,
            },
        };
        match loaded {
            Ok((scene, warnings)) => {
                let doc = MapDoc::new(map.clone(), scene, warnings);
                self.cam.center = doc.bounds().center();
                self.doc = Some(doc);
                self.fit_pending = true;
                self.error = None;
            }
            Err(e) => self.error = Some(format!("Can't open {}: {e}", map.name())),
        }
    }

    /// Runs a JSON file of scene commands on the open map as one undo step.
    fn run_command_file(&mut self, file: &str) {
        let Some(doc) = &mut self.doc else {
            self.error = Some("--exec needs a map: pass --map too".into());
            return;
        };
        let commands = std::fs::read_to_string(file)
            .map_err(|e| e.to_string())
            .and_then(|json| c3_edit::parse_scene_commands(&json));
        match commands {
            Ok(commands) => {
                let name = std::path::Path::new(file)
                    .file_name()
                    .map_or(file.into(), |n| n.to_string_lossy());
                let results = doc.execute_batch(&format!("Run {name}"), &commands);
                let refused: Vec<String> = results
                    .iter()
                    .filter_map(|r| r.as_ref().err().map(ToString::to_string))
                    .collect();
                match refused.first() {
                    Some(first) => {
                        let n = refused.len();
                        let commands = if n == 1 { "command" } else { "commands" };
                        self.error = Some(format!("{n} {commands} refused, first: {first}"));
                    }
                    None => self.notify(format!("Ran {} commands from {name}", commands.len())),
                }
            }
            Err(e) => self.error = Some(format!("{file}: {e}")),
        }
    }

    /// Saves the open map to its own file: a DMap in place, or an engine-format scene.
    pub(crate) fn save(&mut self) -> bool {
        let (Some(project), Some(doc)) = (&self.project, &mut self.doc) else {
            return false;
        };
        doc.end_group();
        let name = doc.name().to_owned();
        let saved = match &doc.map {
            MapRef::Client(path) => match &self.client {
                // Content used in place belongs to the original client: keep its original.
                Some(client) => client
                    .save_map(doc.scene(), path, !project.content_in_project())
                    .map(|_| ()),
                None => Err("the project's content isn't loaded".to_owned()),
            },
            MapRef::Project(_) => {
                c3_scene::native::save_scene(doc.scene(), &project.map_file(&name))
                    .map_err(|e| e.to_string())
            }
        };
        match saved {
            Ok(()) => {
                doc.mark_saved();
                self.notify(format!("Saved {name}"));
                true
            }
            Err(e) => {
                self.error = Some(format!("Can't save {name}: {e}"));
                false
            }
        }
    }

    /// Where saving the open map writes, for hints.
    pub(crate) fn save_target(&self) -> Option<String> {
        let (project, doc) = (self.project.as_ref()?, self.doc.as_ref()?);
        Some(match &doc.map {
            MapRef::Client(path) if project.content_in_project() => format!("assets/{path}"),
            MapRef::Client(path) => {
                let (_, dir) = project.tq_client()?;
                format!(
                    "{} (the original is kept as .bak)",
                    dir.join(path).display()
                )
            }
            MapRef::Project(name) => format!("maps/{name}.scene.ron"),
        })
    }

    /// Resolves the unsaved-changes prompt.
    pub(crate) fn resolve_pending(&mut self, save: bool, ctx: &egui::Context) {
        let Some(p) = self.pending.take() else { return };
        if save && !self.save() {
            return; // keep the user's work if saving failed
        }
        match p {
            Pending::OpenMap(map) => self.load_map(&map),
            Pending::OpenProject(path) => self.open_project(&path),
            Pending::CloseProject => self.close_project(),
            Pending::Quit => {
                self.allow_quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    pub(crate) fn undo(&mut self) {
        if let Some(label) = self.doc.as_mut().and_then(MapDoc::undo) {
            self.notify(format!("Undid: {label}"));
        }
    }

    pub(crate) fn redo(&mut self) {
        if let Some(label) = self.doc.as_mut().and_then(MapDoc::redo) {
            self.notify(format!("Redid: {label}"));
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

    pub(crate) fn filtered_maps(&self) -> Vec<MapRef> {
        let f = self.filter.to_lowercase();
        self.maps
            .iter()
            .filter(|m| m.name().to_lowercase().contains(&f))
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
        if self.workspace == Workspace::Models {
            self.model_shortcuts(ctx);
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
                    .and_then(|d| list.iter().position(|m| *m == d.map));
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
        let project = self.project_name();
        let title = match (&self.project, self.workspace, &self.doc, &self.model) {
            (None, ..) => "C3 Studio".to_owned(),
            (_, Workspace::Models, _, Some(m)) => {
                format!("Model {} — {project} — C3 Studio", m.entry.id)
            }
            (_, Workspace::Maps, Some(d), _) => format!(
                "{}{} — {project} — C3 Studio",
                if d.is_dirty() { "● " } else { "" },
                d.name()
            ),
            _ => format!("{project} — C3 Studio"),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn viewport(&mut self, ui: &mut egui::Ui) {
        let Some(gpu) = &mut self.gpu else {
            ui.centered_and_justified(|ui| {
                ui.colored_label(
                    theme::DANGER,
                    "GPU renderer unavailable (wgpu backend required).",
                )
            });
            return;
        };
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
        let r = map_view::show(
            ui,
            client,
            gpu,
            &mut self.ground,
            doc,
            &mut self.cam,
            self.overlays,
            mode,
        );
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
        self.project_dialog(&ctx);
        self.import_progress(&ctx);
        if self.project.is_none() {
            egui::CentralPanel::default().show(ui, |ui| self.home(ui));
            self.unsaved_prompt(&ctx);
            return;
        }
        let models = self.workspace == Workspace::Models;
        egui::Panel::left("browser")
            .default_size(220.0)
            .size_range(160.0..=400.0)
            .show(ui, |ui| {
                if models {
                    self.model_list(ui)
                } else {
                    self.map_list(ui)
                }
            });
        egui::Panel::right("inspector")
            .default_size(270.0)
            .size_range(220.0..=440.0)
            .show(ui, |ui| {
                if models {
                    self.model_inspector(ui)
                } else {
                    self.inspector(ui)
                }
            });
        egui::CentralPanel::no_frame().show(ui, |ui| {
            if models {
                self.model_viewport(ui)
            } else {
                self.viewport(ui)
            }
        });

        self.unsaved_prompt(&ctx);
        if self.notice.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}
