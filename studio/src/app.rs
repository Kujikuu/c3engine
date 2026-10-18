//! Studio application state: client/map loading, editing commands, shortcuts, and panel layout.

use crate::browser::Browser;
use crate::doc::MapDoc;
use crate::dock::Dock;
use crate::editor::{Editor, Tool};
use crate::gpu::GpuView;
use crate::home::{ImportJob, ProjectDialog};
use crate::live_scene::LiveScene;
use crate::map_view::{Camera, Overlays};
use crate::maps::{self, MapRef};
use crate::model_view::{ModelDoc, OrbitCamera};
use crate::shell::{self, LogEntry, Panel};
use crate::theme;
use crate::tq_plugin::{Client, ModelEntry};
use c3_project::{Project, RecentProjects};
use c3_render::scene_view::Layers;
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

/// The document the keyboard's Undo, Redo and Save act on (the last one worked in).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    Scene,
    Data,
    /// The script editor (its text field keeps its own undo).
    Script,
    Dialogue,
    /// The Project Settings window.
    Settings,
}

/// Something waiting on the "unsaved changes" prompt.
#[derive(Clone)]
pub(crate) enum Pending {
    OpenMap(MapRef),
    OpenTable(String),
    OpenScript(String),
    OpenDialogue(String),
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
    /// The game that plays the project: Conquer's when it has a Conquer client, else the
    /// engine's own. The Scene view, Play and the Preview go through it.
    pub(crate) game: Option<std::rc::Rc<dyn c3_play::GameModule>>,
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
    /// For renderers of their own (Play mode).
    pub(crate) render_state: Option<eframe::egui_wgpu::RenderState>,
    /// The map being played in the Game view.
    pub(crate) playing: Option<crate::play::PlaySession>,
    /// The Preview panel's stage, made when first shown.
    pub(crate) stage: Option<crate::stage::Stage>,
    /// The New Map dialog, while it is open.
    pub(crate) new_map: Option<crate::new_map::NewMapForm>,
    /// The project's settings and their window.
    pub(crate) settings: Option<crate::project_settings::SettingsEditor>,
    /// The settings last applied (to tell what changed).
    pub(crate) applied_settings: Option<c3_core::ProjectSettings>,
    /// The game's fonts need installing (Localization changed).
    pub(crate) fonts_dirty: bool,
    /// The Data panel: tables and the one open.
    pub(crate) data: crate::data_panel::DataPanel,
    /// The Script panel: scripts and the one being edited.
    pub(crate) scripts: crate::script_panel::ScriptPanel,
    /// The Dialogue panel: dialogue graphs, the one being edited, and the quests.
    pub(crate) dialogues: crate::dialogue_panel::DialoguePanel,
    pub(crate) focus: Focus,
    /// Play runs a local game server and joins it.
    pub(crate) play_online: bool,
    /// The open map drawn as the game draws it; needs the project's content.
    pub(crate) live_scene: Option<LiveScene>,
    /// Which parts of the scene the Scene view draws.
    pub(crate) layers: Layers,
    /// Objects and effects animate in the Scene view.
    pub(crate) animate: bool,
    /// The Scene view's animation clock (stops while `animate` is off).
    pub(crate) scene_ms: f64,
    /// The content's effect names, for the create menu.
    pub(crate) effect_names: Vec<String>,
    pub(crate) effect_search: String,
    /// The Inspector's effect chooser is open.
    pub(crate) picking_effect: bool,
    /// Selection the Hierarchy last scrolled to.
    pub(crate) revealed: Option<crate::selection::Selection>,
    pub(crate) hover: Option<CellPos>,
    pub(crate) streaming: bool,
    pub(crate) error: Option<String>,
    pub(crate) notice: Option<(String, Instant)>,
    pub(crate) pending: Option<Pending>,
    pub(crate) fit_pending: bool,
    /// The docking layout of the panels.
    pub(crate) dock: Dock<Panel>,
    /// The Project panel's asset browser.
    pub(crate) browser: Browser,
    /// For uploading textures outside of a frame (start-up options).
    pub(crate) ctx: egui::Context,
    /// The Inspector shows the browser's selection (else the open map or model).
    pub(crate) inspect_asset: bool,
    /// Console messages, oldest first.
    pub(crate) log: Vec<LogEntry>,
    pub(crate) console_infos: bool,
    pub(crate) console_errors: bool,
    /// Last error / notice copied into the console.
    pub(crate) logged_error: Option<String>,
    pub(crate) logged_notice: Option<Instant>,
    /// The game client started by "Play in game window".
    pub(crate) game_window: Option<std::process::Child>,
    pub(crate) started: Instant,
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
            game: None,
            new_map: None,
            settings: None,
            applied_settings: None,
            fonts_dirty: false,
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
                // The scene shows the art now; walkability is shown on demand (B).
                blocked: false,
                portals: true,
                markers: false,
                npcs: true,
                spawns: true,
            },
            editor: Editor::default(),
            gpu: cc.wgpu_render_state.as_ref().map(GpuView::new),
            render_state: cc.wgpu_render_state.clone(),
            playing: None,
            stage: None,
            data: Default::default(),
            scripts: Default::default(),
            dialogues: Default::default(),
            play_online: false,
            focus: Focus::Scene,
            live_scene: None,
            layers: Layers::default(),
            animate: true,
            scene_ms: 0.0,
            effect_names: Vec::new(),
            effect_search: String::new(),
            picking_effect: false,
            revealed: None,
            hover: None,
            streaming: false,
            error: None,
            notice: None,
            pending: None,
            fit_pending: false,
            dock: shell::load_layout(),
            browser: Browser::default(),
            ctx: cc.egui_ctx.clone(),
            inspect_asset: false,
            log: Vec::new(),
            console_infos: true,
            console_errors: true,
            logged_error: None,
            logged_notice: None,
            game_window: None,
            started: Instant::now(),
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
        if args.iter().any(|a| a == "--new-map") {
            s.show_new_map(
                flag("--new-map")
                    .filter(|n| !n.starts_with("--"))
                    .as_deref(),
            );
        }
        if let Some(name) = flag("--make-map") {
            let form = crate::new_map::NewMapForm::default();
            s.create_map(&name, form.width, form.height);
        }
        if let Some(folder) = flag("--browse") {
            s.browse_to(&folder, false);
        }
        if let Some(file) = flag("--select") {
            s.browse_to(&file, true);
        }
        if let Some(file) = flag("--import-population") {
            s.import_population_file(std::path::Path::new(&file));
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
        let cell = flag("--cell").and_then(|c| parse_cell(&c));
        if let (Some(cell), Some(doc)) = (cell, &s.doc) {
            s.fit_pending = false;
            s.cam.center = doc.cell_to_world(cell);
        }
        if let Some(file) = flag("--import-tables") {
            // The import dialog with the suggested tables picked, confirmed at once.
            s.open_dump(PathBuf::from(file));
            s.import_picked();
        }
        if let Some(name) = flag("--table") {
            s.load_table(&name);
        }
        if let Some(name) = flag("--script") {
            s.load_script(&name);
        }
        if let Some(name) = flag("--dialogue") {
            s.load_dialogue(&name);
            if let Some(node) = flag("--node").and_then(|n| n.parse().ok()) {
                s.dialogues.select(node);
            }
        }
        if args.iter().any(|a| a == "--settings") {
            s.show_project_settings(
                flag("--settings")
                    .filter(|n| !n.starts_with("--"))
                    .as_deref(),
            );
        }
        if let Some(name) = flag("--show")
            && let Some(panel) = Panel::ALL
                .into_iter()
                .find(|p| p.name().eq_ignore_ascii_case(&name))
        {
            s.show_panel(panel);
        }
        s.play_online = args.iter().any(|a| a == "--online");
        if args.iter().any(|a| a == "--play") {
            s.toggle_play();
        }
        if args.iter().any(|a| a == "--touch")
            && let Some(session) = &mut s.playing
        {
            session.simulate_touch(true);
        }
        if let (Some(id), Some(session)) = (
            flag("--talk").and_then(|id| id.parse().ok()),
            &mut s.playing,
        ) {
            session.talk_to(id);
        }
        if args.iter().any(|a| a == "--attack")
            && let Some(session) = &mut s.playing
        {
            session.attack_nearest();
        }
        if args.iter().any(|a| a == "--controls")
            && let Some(session) = &mut s.playing
        {
            session.open_controls();
        }
        if args.iter().any(|a| a == "--bag")
            && let Some(session) = &mut s.playing
        {
            session.toggle_bag();
        }
        if args.iter().any(|a| a == "--quests")
            && let Some(session) = &mut s.playing
        {
            session.toggle_quests();
        }
        if let (Some(line), Some(session)) = (flag("--say"), &mut s.playing) {
            session.say(&line);
        }
        let pick = flag("--pick").and_then(|c| parse_cell(&c));
        if let (Some(cell), Some(doc)) = (pick, &s.doc) {
            let world = doc.scene().cell_to_world(cell);
            let everything = crate::selection::Pickable {
                portals: true,
                objects: true,
                npcs: true,
                spawns: true,
            };
            s.editor.selected = crate::selection::pick(doc.scene(), cell, world, 24.0, everything);
        }
        s
    }

    // ---- documents ----

    /// Opens the project's TQ client source (models and not-yet-imported maps come from it),
    /// then lists the maps.
    pub(crate) fn open_client(&mut self) {
        self.client = None;
        self.game = None;
        self.settings = None;
        self.applied_settings = None;
        self.stage = None;
        self.maps.clear();
        self.models.clear();
        if let Some((_, dir)) = self.project.as_ref().and_then(Project::tq_client) {
            match Client::open(&dir.display().to_string()) {
                Ok(client) => {
                    self.models = client.list_models();
                    self.effect_names = client.effect_names();
                    self.client = Some(client);
                }
                Err(e) => self.error = Some(format!("Can't open the project's client folder: {e}")),
            }
        }
        self.game = match (&self.client, &self.project) {
            (Some(client), _) => Some(client.game()),
            (None, Some(project)) => {
                Some(std::rc::Rc::new(c3_play::basic::BasicGame::new(project)))
            }
            (None, None) => None,
        };
        if let (Some(game), Some(project)) = (&self.game, &self.project) {
            let (settings, problem) = c3_play::settings::load(project, &**game);
            if let Some(problem) = problem {
                self.error = Some(problem);
            }
            self.settings = Some(crate::project_settings::SettingsEditor::new(
                settings.clone(),
                game.default_settings(),
            ));
            self.applied_settings = Some(settings);
            self.fonts_dirty = true;
        }
        self.refresh_maps();
        self.refresh_assets();
        self.refresh_tables();
        self.refresh_scripts();
        self.refresh_dialogues();
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
        self.set_live_scene(None);
        self.doc = None;
        self.model = None;
        self.filter.clear();
        self.hover = None;
        self.editor = Editor::default();
    }

    /// Replaces the Scene view's renderer state, freeing the old one's textures.
    fn set_live_scene(&mut self, live: Option<LiveScene>) {
        if let (Some(old), Some(gpu)) = (&mut self.live_scene, &mut self.gpu) {
            old.release(&mut gpu.renderer);
        }
        self.live_scene = live;
    }

    /// Deletes what the Scene view has selected.
    pub(crate) fn delete_selection(&mut self) {
        let (Some(doc), Some(selected)) = (&mut self.doc, self.editor.selected) else {
            return;
        };
        let label = selected.label(doc.scene());
        match doc.execute(&selected.remove()) {
            Ok(outcome) => {
                self.editor.selected = None;
                self.notify(outcome.summary);
            }
            Err(e) => self.error = Some(format!("Can't delete {label}: {e}")),
        }
    }

    /// Adds a copy of the selection next to it and selects the copy.
    pub(crate) fn duplicate_selection(&mut self) {
        let command = match (&self.doc, self.editor.selected) {
            (Some(doc), Some(selected)) => selected.duplicate(doc.scene()),
            _ => None,
        };
        if let Some(command) = command {
            self.create(command);
        }
    }

    /// Centres the Scene view on the selection, or fits the map when nothing is selected.
    pub(crate) fn frame_selection(&mut self) {
        let cell = self.doc.as_ref().and_then(|doc| {
            let cell = self.editor.selected?.cell(doc.scene())?;
            Some(doc.cell_to_world(cell))
        });
        match cell {
            Some(p) => {
                self.cam.center = p;
                self.cam.zoom = self.cam.zoom.max(0.75);
            }
            None => self.fit_pending = true,
        }
    }

    /// The open map has unsaved changes.
    pub(crate) fn map_dirty(&self) -> bool {
        self.doc.as_ref().is_some_and(MapDoc::is_dirty)
    }

    /// Anything open has unsaved changes (map, table or script).
    pub(crate) fn is_dirty(&self) -> bool {
        self.map_dirty()
            || self.data.is_dirty()
            || self.scripts.is_dirty()
            || self.dialogues.is_dirty()
    }

    /// Saves every open document with changes; false if one failed.
    pub(crate) fn save_all(&mut self) -> bool {
        (!self.map_dirty() || self.save())
            && (!self.data.is_dirty() || self.save_table())
            && (!self.scripts.is_dirty() || self.save_script())
            && (!self.dialogues.is_dirty() || self.save_dialogue())
    }

    /// What has unsaved changes, for prompts.
    pub(crate) fn unsaved_names(&self) -> String {
        let mut names = Vec::new();
        if let Some(doc) = self.doc.as_ref().filter(|d| d.is_dirty()) {
            names.push(doc.name().to_owned());
        }
        if let Some(t) = self.data.open.as_ref().filter(|t| t.doc.is_dirty()) {
            names.push(format!("table {}", t.name));
        }
        if let Some(s) = self.scripts.open.as_ref().filter(|s| s.is_dirty()) {
            names.push(format!("script {}", s.name));
        }
        if let Some(d) = self.dialogues.open.as_ref().filter(|d| d.is_dirty()) {
            names.push(format!("dialogue {}", d.name));
        }
        names.join(" and ")
    }

    /// Opens a map, asking first if the current one has unsaved changes.
    pub(crate) fn open_map(&mut self, map: &MapRef) {
        if self.doc.as_ref().is_some_and(|d| &d.map == map) {
            return;
        }
        if self.map_dirty() {
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
            MapRef::Client(path) => match (&self.client, &self.project) {
                (Some(client), Some(project)) => client.import_map(path).and_then(|imported| {
                    let mut scene = imported.scene;
                    let file = project.population_file(map.name());
                    let population =
                        c3_scene::native::load_population(&file).map_err(|e| e.to_string())?;
                    scene.set_population(population);
                    Ok((scene, imported.warnings))
                }),
                _ => return,
            },
        };
        match loaded {
            Ok((scene, warnings)) => {
                self.inspect_asset = false;
                let doc = MapDoc::new(map.clone(), scene, warnings);
                self.cam.center = doc.bounds().center();
                self.doc = Some(doc);
                self.rebuild_live_scene();
                self.fit_pending = true;
                self.error = None;
            }
            Err(e) => self.error = Some(format!("Can't open {}: {e}", map.name())),
        }
    }

    /// Builds the Scene view's drawing of the open map anew (a new map, a new tilt).
    pub(crate) fn rebuild_live_scene(&mut self) {
        let tilt = self.tilt_deg();
        let live = match (&self.game, &self.doc) {
            (Some(game), Some(doc)) => Some(LiveScene::new(
                doc.scene(),
                &**game,
                self.scene_ms as u64,
                tilt,
            )),
            _ => None,
        };
        self.set_live_scene(live);
    }

    /// The tilt of 3D roles and effects: the settings', else the game's.
    pub(crate) fn tilt_deg(&self) -> f32 {
        match (&self.game, &self.applied_settings) {
            (Some(game), Some(settings)) => c3_play::settings::tilt_deg(settings, &**game),
            (Some(game), None) => game.tilt_deg(),
            _ => 0.0,
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
                    .and_then(|_| {
                        // DMaps hold no population: it lives next to the project's maps.
                        let file = project.population_file(&name);
                        c3_scene::native::save_population(&doc.scene().population(), &file)
                            .map_err(|e| e.to_string())
                    }),
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
        if save && !self.save_all() {
            return; // keep the user's work if saving failed
        }
        match p {
            Pending::OpenMap(map) => self.load_map(&map),
            Pending::OpenTable(name) => {
                // Not saving drops the table's changes.
                self.data.open = None;
                self.load_table(&name);
            }
            Pending::OpenScript(name) => {
                self.scripts.open = None;
                self.load_script(&name);
            }
            Pending::OpenDialogue(name) => {
                self.dialogues.open = None;
                self.load_dialogue(&name);
            }
            Pending::OpenProject(path) => self.open_project(&path),
            Pending::CloseProject => self.close_project(),
            Pending::Quit => {
                self.allow_quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Installs the game's fonts (Localization) for the Game view's text.
    fn install_game_fonts(&mut self, ctx: &egui::Context) {
        let (Some(game), Some(settings)) = (&self.game, &self.applied_settings) else {
            c3_ui::fonts::install(ctx, Vec::new());
            return;
        };
        let (fonts, problems) = c3_play::settings::game_fonts(settings, &**game);
        if let Some(problem) = problems.into_iter().next() {
            self.error = Some(problem);
        }
        c3_ui::fonts::install(ctx, fonts);
    }

    pub(crate) fn undo(&mut self) {
        if self.focus == Focus::Settings {
            let label = self.settings.as_mut().and_then(|s| s.doc.undo());
            if let Some(label) = label {
                self.settings_changed();
                self.notify(format!("Undid: {label}"));
            }
            return;
        }
        let label = match self.focus {
            Focus::Scene => self.doc.as_mut().and_then(MapDoc::undo),
            Focus::Data => self.data.undo(),
            Focus::Script => None,
            // A focused text field undoes its own typing.
            Focus::Dialogue if self.dialogues.typing => None,
            Focus::Dialogue => self.dialogues.undo(),
            Focus::Settings => None,
        };
        if let Some(label) = label {
            self.notify(format!("Undid: {label}"));
        }
    }

    pub(crate) fn redo(&mut self) {
        if self.focus == Focus::Settings {
            let label = self.settings.as_mut().and_then(|s| s.doc.redo());
            if let Some(label) = label {
                self.settings_changed();
                self.notify(format!("Redid: {label}"));
            }
            return;
        }
        let label = match self.focus {
            Focus::Scene => self.doc.as_mut().and_then(MapDoc::redo),
            Focus::Data => self.data.redo(),
            Focus::Script => None,
            Focus::Dialogue if self.dialogues.typing => None,
            Focus::Dialogue => self.dialogues.redo(),
            Focus::Settings => None,
        };
        if let Some(label) = label {
            self.notify(format!("Redid: {label}"));
        }
    }

    /// Labels of the steps Undo and Redo would take, in the focused document.
    pub(crate) fn undo_redo_labels(&self) -> (Option<String>, Option<String>) {
        match self.focus {
            Focus::Scene => (
                self.doc
                    .as_ref()
                    .and_then(|d| d.undo_label())
                    .map(str::to_owned),
                self.doc
                    .as_ref()
                    .and_then(|d| d.redo_label())
                    .map(str::to_owned),
            ),
            Focus::Data => {
                let doc = self.data.open.as_ref().map(|t| &t.doc);
                (
                    doc.and_then(|d| d.undo_label()).map(str::to_owned),
                    doc.and_then(|d| d.redo_label()).map(str::to_owned),
                )
            }
            Focus::Script => (None, None),
            Focus::Dialogue => self.dialogues.undo_redo_labels(),
            Focus::Settings => {
                let doc = self.settings.as_ref().map(|s| &s.doc);
                (
                    doc.and_then(|d| d.undo_label()).map(str::to_owned),
                    doc.and_then(|d| d.redo_label()).map(str::to_owned),
                )
            }
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
        let (undo, redo, save, duplicate, play, pause) = ctx.input(|i| {
            let cmd = i.modifiers.command;
            let z = cmd && i.key_pressed(Key::Z);
            let p = cmd && i.key_pressed(Key::P);
            (
                z && !i.modifiers.shift,
                (z && i.modifiers.shift) || (cmd && i.key_pressed(Key::Y)),
                cmd && i.key_pressed(Key::S),
                cmd && i.key_pressed(Key::D),
                p && !i.modifiers.shift,
                p && i.modifiers.shift,
            )
        });
        if play {
            self.toggle_play();
        }
        if pause && let Some(session) = &mut self.playing {
            session.paused ^= true;
        }
        if duplicate && !ctx.egui_wants_keyboard_input() {
            self.duplicate_selection();
        }
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        if save {
            match self.focus {
                Focus::Scene => {
                    self.save();
                }
                Focus::Data => {
                    self.save_table();
                }
                Focus::Script => {
                    self.save_script();
                }
                Focus::Dialogue => {
                    self.save_dialogue();
                }
                // Settings are saved as they change.
                Focus::Settings => {}
            }
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        if self.workspace == Workspace::Models {
            self.model_shortcuts(ctx);
            return;
        }
        if self.playing.is_some() {
            // While playing, the Game view takes the keyboard (see `GameScreen`).
            return;
        }
        let (mut delete, mut frame) = (false, false);
        ctx.input(|i| {
            if i.modifiers.command {
                return;
            }
            let (o, l) = (&mut self.overlays, &mut self.layers);
            for (key, flag) in [
                (Key::G, &mut o.grid),
                (Key::B, &mut o.blocked),
                (Key::P, &mut o.portals),
                (Key::M, &mut o.markers),
                (Key::N, &mut o.npcs),
                (Key::O, &mut l.objects),
                (Key::E, &mut l.effects),
            ] {
                if i.key_pressed(key) {
                    *flag ^= true;
                }
            }
            for (key, tool) in [
                (Key::Num1, Tool::Inspect),
                (Key::Num2, Tool::Select),
                (Key::Num3, Tool::Block),
                (Key::Num4, Tool::Clear),
                (Key::Num5, Tool::Portal),
            ] {
                if i.key_pressed(key) {
                    self.editor.tool = tool;
                }
            }
            if i.key_pressed(Key::Escape) {
                self.editor.selected = None;
            }
            if i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace) {
                delete = true;
            }
            if i.key_pressed(Key::OpenBracket) {
                self.editor.grow_brush(-1);
            }
            if i.key_pressed(Key::CloseBracket) {
                self.editor.grow_brush(1);
            }
            if i.key_pressed(Key::F) {
                frame = true;
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
        if delete {
            self.delete_selection();
        }
        if frame {
            self.frame_selection();
        }
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
}

impl eframe::App for Studio {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Some(session) = &mut self.playing {
            session.before_frame(raw_input);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.guard_quit(&ctx);
        self.shortcuts(&ctx);
        self.update_title(&ctx);
        self.poll_play();
        self.sync_console();

        let bar = egui::Frame::new()
            .fill(theme::BAR_FILL)
            .inner_margin(egui::Margin::symmetric(8, 2));
        egui::Panel::top("menu bar")
            .frame(bar)
            .show(ui, |ui| self.menu_bar(ui));
        if self.project.is_some() {
            egui::Panel::top("toolbar")
                .exact_size(34.0)
                .frame(bar)
                .show(ui, |ui| self.main_toolbar(ui));
        }
        egui::Panel::bottom("status")
            .exact_size(24.0)
            .frame(bar)
            .show(ui, |ui| self.status_bar(ui));
        self.project_dialog(&ctx);
        self.new_map_dialog(&ctx);
        self.project_settings_window(&ctx);
        if std::mem::take(&mut self.fonts_dirty) {
            self.install_game_fonts(&ctx);
        }
        self.import_progress(&ctx);
        if self.project.is_none() {
            egui::CentralPanel::default().show(ui, |ui| self.home(ui));
        } else {
            egui::CentralPanel::no_frame().show(ui, |ui| self.dock_ui(ui));
        }
        self.unsaved_prompt(&ctx);
        if self.notice.is_some() || self.game_window.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}

/// `x,y` → a cell.
fn parse_cell(text: &str) -> Option<CellPos> {
    let (x, y) = text.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}
