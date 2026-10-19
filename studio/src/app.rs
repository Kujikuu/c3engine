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
    /// The project's assets (the Project panel and the Inspector's Import section).
    Assets,
    /// The effect being edited in the Preview panel.
    Effect,
    /// The animation set being edited in the Preview panel.
    Character,
    /// The game UI in the Game UI panel.
    Ui,
    /// The blueprint in the Blueprints panel.
    Blueprint,
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
    /// The game's maps by id (what portals lead to), from the maps and the settings.
    pub(crate) map_list: c3_play::map_list::MapList,
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
    /// The project's assets: their sidecars and the file operations on them.
    pub(crate) assets: Option<c3_edit::AssetDoc>,
    /// Text typed in the Import section, not yet committed.
    pub(crate) import_buffers: std::collections::HashMap<String, String>,
    /// Plays sounds the Inspector previews (opened on first use).
    pub(crate) preview_audio: Option<c3_audio::Audio>,
    /// An effect the Preview panel is to play next.
    pub(crate) preview_effect: Option<String>,
    /// A look the Preview panel is to show next (a monster's, from its form).
    pub(crate) preview_look: Option<c3_play::Look>,
    /// What the Data panel's forms read.
    pub(crate) forms: crate::forms::FormCache,
    /// The Project panel's rename dialog, while open.
    pub(crate) rename: Option<crate::asset_ops::RenameForm>,
    /// Watches the project's assets for changes made outside (or by) the Studio.
    pub(crate) watcher: Option<crate::hot_reload::AssetWatcher>,
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
    /// The game's NPC looks, `(look, name)`, and what the look picker searches for.
    pub(crate) npc_looks: Vec<(u32, String)>,
    pub(crate) look_search: String,
    /// The Inspector's effect chooser is open.
    pub(crate) picking_effect: bool,
    /// Selection the Hierarchy last scrolled to.
    pub(crate) revealed: Option<crate::selection::Selection>,
    /// Things copied from a map, ready to paste on any map.
    pub(crate) clipboard: Option<crate::selection::Clip>,
    /// The Ground tool's palette thumbnails.
    pub(crate) ground_thumbs: crate::ground_panel::Thumbnails,
    pub(crate) world: crate::world_view::WorldView,
    /// The project effect open in the Preview panel's editor.
    pub(crate) effect_editor: Option<crate::effect_editor::EffectEditor>,
    pub(crate) character_editor: Option<crate::character_editor::CharacterEditor>,
    pub(crate) ui_editor: Option<crate::ui_editor::UiEditor>,
    pub(crate) blueprint_editor: Option<crate::blueprint_editor::BlueprintEditor>,
    /// Where agents' calls arrive while the live link is on (Window > Live link).
    pub(crate) live_link: Option<c3_agent::live::Listener>,
    /// Panels asked to the front while the dock was drawing.
    pub(crate) panel_requests: Vec<crate::shell::Panel>,
    pub(crate) drawing_dock: bool,
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
            assets: None,
            import_buffers: std::collections::HashMap::new(),
            preview_audio: None,
            preview_effect: None,
            preview_look: None,
            forms: Default::default(),
            rename: None,
            watcher: None,
            fonts_dirty: false,
            client: None,
            workspace: Workspace::Maps,
            maps: Vec::new(),
            map_list: Default::default(),
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
                surface: false,
                height: false,
                regions: true,
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
            npc_looks: Vec::new(),
            look_search: String::new(),
            picking_effect: false,
            revealed: None,
            clipboard: None,
            ground_thumbs: Default::default(),
            world: Default::default(),
            effect_editor: None,
            character_editor: None,
            ui_editor: None,
            blueprint_editor: None,
            live_link: None,
            panel_requests: Vec::new(),
            drawing_dock: false,
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
        if let Some(file) = flag("--asset-exec") {
            s.run_asset_command_file(&file);
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
            let ground = flag("--ground").unwrap_or_default();
            s.create_map(&name, form.width, form.height, &ground);
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
        if args.iter().any(|a| a == "--live") {
            s.set_live_link(true);
        }
        // A tool by its number key (1-6), and the Ground tool's image.
        if let Some(tool) = flag("--tool")
            .and_then(|t| t.parse::<u8>().ok())
            .and_then(|k| Tool::ALL.into_iter().find(|t| t.key() == k))
        {
            s.editor.tool = tool;
        }
        if let Some(image) = flag("--ground-image") {
            s.editor.ground.image = Some(image);
        }
        if let Some(mode) = flag("--ground-mode") {
            use crate::editor::GroundMode;
            s.editor.ground.mode = match mode.as_str() {
                "fill" => GroundMode::Fill,
                "erase" => GroundMode::Erase,
                "pick" => GroundMode::Pick,
                _ => GroundMode::Paint,
            };
        }
        if let Some(index) = flag("--layer").and_then(|l| l.parse().ok()) {
            s.editor.layer = Some(index);
        }
        if args.iter().any(|a| a == "--minimap") {
            s.generate_minimap();
        }
        // A TQ model by id, or a glTF model of the project's assets by path.
        if let Some(model) = flag("--model") {
            s.workspace = Workspace::Models;
            match model.parse::<u64>() {
                Ok(id) => match s.models.iter().find(|e| e.id == id).cloned() {
                    Some(entry) => s.open_model(&entry),
                    None => s.error = Some(format!("Model {id} is not in 3dobj.ini")),
                },
                Err(_) => s.open_model(&crate::tq_plugin::ModelEntry { id: 0, path: model }),
            }
        }
        if let Some(set) = flag("--character") {
            s.open_character(&set);
            s.show_panel(crate::shell::Panel::Preview);
        }
        if let Some(effect) = flag("--effect") {
            s.preview_effect = Some(effect);
            s.show_panel(crate::shell::Panel::Preview);
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
        if let Some(name) = flag("--new-table") {
            let template = (s.game.as_ref())
                .and_then(|g| g.table_templates().into_iter().find(|t| t.name == name));
            match template {
                Some(table) => s.create_table(table),
                None => s.error = Some(format!("The game has no table {name} to start")),
            }
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
        if let (Some(cell), Some(session)) =
            (flag("--walk").and_then(|c| parse_cell(&c)), &mut s.playing)
        {
            session.walk_to(cell);
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
        if let Some(session) = &mut s.playing {
            // Each `--say` in turn.
            for pair in args.windows(2).filter(|w| w[0] == "--say") {
                session.say(&pair[1]);
            }
        }
        if let (Some(slot), Some(session)) =
            (flag("--cast").and_then(|n| n.parse().ok()), &mut s.playing)
        {
            session.cast_slot(slot);
        }
        let pick = flag("--pick").and_then(|c| parse_cell(&c));
        if let (Some(cell), Some(doc)) = (pick, &s.doc) {
            let world = doc.scene().cell_to_world(cell);
            let everything = crate::selection::Pickable {
                portals: true,
                objects: true,
                npcs: true,
                spawns: true,
                regions: true,
            };
            let picked = crate::selection::pick(doc.scene(), cell, world, 24.0, everything);
            s.editor.select(picked);
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
        self.assets = None;
        self.preview_audio = None;
        self.stage = None;
        self.maps.clear();
        self.models.clear();
        self.effect_names.clear();
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
        let game: Option<std::rc::Rc<dyn c3_play::GameModule>> = match &self.client {
            Some(client) => Some(client.game()),
            None => Some(std::rc::Rc::new(c3_play::basic::BasicGame::default())),
        };
        self.game = match (game, &self.project) {
            (Some(game), Some(project)) => {
                let (game, problems) = c3_play::ProjectGame::open(project, game);
                if let Some(problem) = problems.into_iter().next() {
                    self.error = Some(problem);
                }
                Some(std::rc::Rc::new(game))
            }
            _ => None,
        };
        self.npc_looks = self
            .game
            .as_ref()
            .map(|g| g.npc_looks())
            .unwrap_or_default();
        // The project's own effects come first in the effect lists.
        if let Some(native) = self.game.as_ref().and_then(|g| g.native()) {
            let mut names = native.effect_names();
            names.append(&mut self.effect_names);
            self.effect_names = names;
        }
        self.reload_settings();
        if let Some(project) = &self.project {
            self.watcher = crate::hot_reload::AssetWatcher::new(&project.assets_dir(), &self.ctx);
            let (db, problems) = c3_project::AssetDb::of(project);
            if let Some(problem) = problems.into_iter().next() {
                self.error = Some(problem);
            }
            self.assets = Some(c3_edit::AssetDoc::new(project.clone(), db));
        }
        self.refresh_maps();
        self.refresh_assets();
        self.refresh_tables();
        self.refresh_scripts();
        self.refresh_dialogues();
    }

    /// Reads the project's settings from disk (their editor starts a fresh history).
    pub(crate) fn reload_settings(&mut self) {
        let (Some(game), Some(project)) = (&self.game, &self.project) else {
            return;
        };
        let (settings, problem) = c3_play::settings::load(project, &**game);
        if let Some(problem) = problem {
            self.error = Some(problem);
        }
        let mut editor =
            crate::project_settings::SettingsEditor::new(settings.clone(), game.default_settings());
        if let Some(old) = &self.settings {
            editor.keep_view(old);
        }
        self.settings = Some(editor);
        self.applied_settings = Some(settings);
        self.fonts_dirty = true;
        self.refresh_map_list();
    }

    /// Re-lists the maps.
    pub(crate) fn refresh_maps(&mut self) {
        self.maps = match &self.project {
            Some(project) => maps::list_maps(project, self.client.as_ref()),
            None => Vec::new(),
        };
        self.refresh_map_list();
        self.world.refresh();
    }

    /// Re-reads the game's map list (ids, names, music) from the maps and the settings.
    pub(crate) fn refresh_map_list(&mut self) {
        self.map_list = match (&self.project, &self.game) {
            (Some(project), Some(game)) => {
                let settings =
                    (self.applied_settings.clone()).unwrap_or_else(|| game.default_settings());
                c3_play::map_list::MapList::new(project, &**game, &settings)
            }
            _ => Default::default(),
        };
    }

    /// Closes the open map and model (without asking).
    pub(crate) fn close_documents(&mut self) {
        self.close_map();
        self.model = None;
        self.filter.clear();
    }

    /// Closes the open map (without asking).
    pub(crate) fn close_map(&mut self) {
        self.set_live_scene(None);
        self.doc = None;
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

    /// Deletes what the Scene view has selected (one undo step).
    pub(crate) fn delete_selection(&mut self) {
        let Some(doc) = &mut self.doc else { return };
        let selection = self.editor.selection().to_vec();
        let Some(first) = selection.first() else {
            return;
        };
        let label = match selection.len() {
            1 => format!("Delete {}", first.label(doc.scene())),
            n => format!("Delete {n} things"),
        };
        let results = doc.execute_batch(&label, &crate::selection::remove_all(&selection));
        match results.into_iter().find_map(Result::err) {
            Some(e) => self.error = Some(format!("Can't delete: {e}")),
            None => {
                self.editor.select(None);
                self.editor.forget_object_marks();
                self.notify(label);
            }
        }
    }

    /// Adds a copy of the selection one cell down-right and selects the copies.
    pub(crate) fn duplicate_selection(&mut self) {
        let Some(doc) = &self.doc else { return };
        let selection = self.editor.selection();
        let Some(clip) = crate::selection::Clip::of(doc.scene(), selection) else {
            return;
        };
        let Some(at) = selection[0].cell(doc.scene()) else {
            return;
        };
        self.place_clip(&clip, (at.0 + 1, at.1 + 1), "Duplicate");
    }

    /// Copies the selection to the Studio's clipboard (it pastes on any map).
    pub(crate) fn copy_selection(&mut self) {
        let Some(doc) = &self.doc else { return };
        if let Some(clip) = crate::selection::Clip::of(doc.scene(), self.editor.selection()) {
            let what = format!("Copied {} from {}", clip.len(), doc.name());
            // The system clipboard says what was copied (the copy itself stays in the Studio).
            self.ctx.copy_text(what.clone());
            self.notify(format!("{what} · Ctrl+V pastes it under the pointer"));
            self.clipboard = Some(clip);
        }
    }

    /// Pastes the clipboard with its first thing under the pointer (else the view's middle).
    pub(crate) fn paste(&mut self) {
        let (Some(doc), Some(clip)) = (&self.doc, self.clipboard.clone()) else {
            return;
        };
        let at = self
            .hover
            .unwrap_or_else(|| doc.world_to_cell(self.cam.center));
        self.place_clip(&clip, at, "Paste");
    }

    /// Adds `clip` with its first thing on `at` as one undo step called `verb`, and selects
    /// what it added.
    fn place_clip(&mut self, clip: &crate::selection::Clip, at: CellPos, verb: &str) {
        let Some(doc) = &mut self.doc else { return };
        let census = crate::selection::Census::of(doc.scene());
        let commands = clip.commands(doc.scene(), at);
        let label = format!("{verb} {} things", clip.len());
        let results = doc.execute_batch(&label, &commands);
        let refused = results.into_iter().filter(Result::is_err).count();
        self.editor.select_all(census.added(doc.scene()));
        self.editor.tool = crate::editor::Tool::Select;
        self.notify(match refused {
            0 => label,
            n => format!("{label} ({n} off the map left out)"),
        });
    }

    /// Centres the Scene view on the selection, or fits the map when nothing is selected.
    pub(crate) fn frame_selection(&mut self) {
        let cell = self.doc.as_ref().and_then(|doc| {
            let cell = self.editor.selected()?.cell(doc.scene())?;
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

    /// Reads map `map` from disk: its scene and what was wrong with it.
    pub(crate) fn read_map(&self, map: &MapRef) -> Result<(c3_scene::Scene, Vec<String>), String> {
        let project = self.project.as_ref().ok_or("no project is open")?;
        match map {
            MapRef::Project(name) => c3_scene::native::load_scene(&project.map_file(name))
                .map(|scene| (scene, Vec::new()))
                .map_err(|e| e.to_string()),
            MapRef::Client(path) => {
                let client = self
                    .client
                    .as_ref()
                    .ok_or("the project's content isn't loaded")?;
                client.import_map(path).and_then(|imported| {
                    let mut scene = imported.scene;
                    let file = project.population_file(map.name());
                    let population =
                        c3_scene::native::load_population(&file).map_err(|e| e.to_string())?;
                    scene.set_population(population);
                    Ok((scene, imported.warnings))
                })
            }
        }
    }

    /// Writes `scene` as map `map`: a project scene, or a client map (DMap, its ground when
    /// `ground` changed, and its population beside the project's maps).
    pub(crate) fn write_map(
        &self,
        map: &MapRef,
        scene: &c3_scene::Scene,
        ground: Option<&c3_scene::TileLayer>,
    ) -> Result<(), String> {
        let project = self.project.as_ref().ok_or("no project is open")?;
        match map {
            MapRef::Client(path) => {
                let game = self
                    .game
                    .as_ref()
                    .ok_or("the project's content isn't loaded")?;
                game.save_map(project, path, scene, ground)
            }
            MapRef::Project(name) => c3_scene::native::save_scene(scene, &project.map_file(name))
                .map_err(|e| e.to_string()),
        }
    }

    pub(crate) fn load_map(&mut self, map: &MapRef) {
        let loaded = self.read_map(map);
        match loaded {
            Ok((scene, warnings)) => {
                self.inspect_asset = false;
                let doc = MapDoc::new(map.clone(), scene, warnings);
                // Ids and indices name other things on another map.
                self.editor.select(None);
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
        let Some(doc) = &mut self.doc else {
            return false;
        };
        doc.end_group();
        let name = doc.name().to_owned();
        let doc = self.doc.as_ref().expect("checked above");
        let saved = self.write_map(&doc.map, doc.scene(), doc.changed_ground());
        match saved {
            Ok(()) => {
                if let Some(doc) = &mut self.doc {
                    doc.mark_saved();
                }
                self.world.refresh();
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
        if self.focus == Focus::Assets {
            let done = self.assets.as_mut().map(c3_edit::AssetDoc::undo);
            self.assets_changed(done, "Undid");
            return;
        }
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
            Focus::Script => self.scripts.undo(&self.ctx),
            // A focused text field undoes its own typing.
            Focus::Dialogue if self.dialogues.typing => None,
            Focus::Dialogue => self.dialogues.undo(),
            Focus::Settings | Focus::Assets => None,
            Focus::Effect => self.effect_step(false),
            Focus::Character => self.character_step(false),
            Focus::Ui => self.ui_step(false),
            Focus::Blueprint => self.blueprint_step(false),
        };
        if let Some(doc) = &self.doc {
            self.editor.forget_missing(doc.scene());
        }
        if let Some(label) = label {
            self.notify(format!("Undid: {label}"));
        }
    }

    pub(crate) fn redo(&mut self) {
        if self.focus == Focus::Assets {
            let done = self.assets.as_mut().map(c3_edit::AssetDoc::redo);
            self.assets_changed(done, "Redid");
            return;
        }
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
            Focus::Script => self.scripts.redo(&self.ctx),
            Focus::Dialogue if self.dialogues.typing => None,
            Focus::Dialogue => self.dialogues.redo(),
            Focus::Settings | Focus::Assets => None,
            Focus::Effect => self.effect_step(true),
            Focus::Character => self.character_step(true),
            Focus::Ui => self.ui_step(true),
            Focus::Blueprint => self.blueprint_step(true),
        };
        if let Some(doc) = &self.doc {
            self.editor.forget_missing(doc.scene());
        }
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
            Focus::Script => self.scripts.undo_redo_labels(),
            Focus::Dialogue => self.dialogues.undo_redo_labels(),
            Focus::Settings => {
                let doc = self.settings.as_ref().map(|s| &s.doc);
                (
                    doc.and_then(|d| d.undo_label()).map(str::to_owned),
                    doc.and_then(|d| d.redo_label()).map(str::to_owned),
                )
            }
            Focus::Assets => {
                let doc = self.assets.as_ref();
                (
                    doc.and_then(|d| d.undo_label()).map(str::to_owned),
                    doc.and_then(|d| d.redo_label()).map(str::to_owned),
                )
            }
            Focus::Effect => self.effect_labels(),
            Focus::Character => self.character_labels(),
            Focus::Ui => self.ui_labels(),
            Focus::Blueprint => self.blueprint_labels(),
        }
    }

    /// The focused document's name and undo history, for the Undo History panel.
    pub(crate) fn focused_history(&self) -> Option<(String, Vec<c3_edit::HistoryEntry>)> {
        match self.focus {
            Focus::Scene => {
                let doc = self.doc.as_ref()?;
                Some((format!("Map {}", doc.name()), doc.history()))
            }
            Focus::Data => {
                let open = self.data.open.as_ref()?;
                Some((
                    format!("Table {}", open.doc.table().name),
                    open.doc.history(),
                ))
            }
            Focus::Script => {
                let open = self.scripts.open.as_ref()?;
                Some((format!("Script {}", open.name), self.scripts.history()))
            }
            Focus::Dialogue => {
                let open = self.dialogues.open.as_ref()?;
                Some((format!("Dialogue {}", open.name), self.dialogues.history()))
            }
            Focus::Settings => {
                let s = self.settings.as_ref()?;
                Some(("Project Settings".to_owned(), s.doc.history()))
            }
            Focus::Assets => Some(("Assets".to_owned(), self.assets.as_ref()?.history())),
            Focus::Effect => self.effect_history(),
            Focus::Character => self.character_history(),
            Focus::Ui => self.ui_history(),
            Focus::Blueprint => self.blueprint_history(),
        }
    }

    /// After an asset step was undone or redone (`verb`): the files changed on disk.
    fn assets_changed(
        &mut self,
        done: Option<Result<Option<String>, c3_edit::EditError>>,
        verb: &str,
    ) {
        match done {
            Some(Ok(Some(label))) => {
                self.import_buffers.clear();
                self.asset_files_changed();
                self.notify(format!("{verb}: {label}"));
            }
            Some(Err(e)) => self.error = Some(e.to_string()),
            _ => {}
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
        // Copy and paste arrive as events (the platform turns Ctrl+C / Ctrl+V into them).
        let (copy, paste) = ctx.input(|i| {
            let copy = i.events.iter().any(|e| matches!(e, egui::Event::Copy));
            // Paste comes as an event only while the system clipboard holds text.
            let paste = i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)))
                || (i.modifiers.command && i.key_pressed(Key::V));
            (copy, paste)
        });
        let map_keys = !ctx.egui_wants_keyboard_input()
            && self.playing.is_none()
            && self.workspace != Workspace::Models;
        if map_keys && copy {
            self.copy_selection();
        }
        if map_keys && paste {
            self.paste();
        }
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
                // Settings, assets and effects are saved as they change.
                Focus::Settings
                | Focus::Assets
                | Focus::Effect
                | Focus::Character
                | Focus::Ui
                | Focus::Blueprint => {}
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
        let (mut step, mut pixel) = (None, None);
        let (mut hide, mut show_all, mut lock, mut unlock_all) = (false, false, false, false);
        let selecting = !self.editor.selection().is_empty();
        ctx.input(|i| {
            if i.modifiers.command {
                return;
            }
            // With a selection the arrows move it (Alt: by a pixel); else they pan.
            for (key, cells, pixels) in crate::arrange::SCREEN_STEPS {
                if selecting && i.key_pressed(key) {
                    if i.modifiers.alt {
                        pixel = Some(pixels);
                    } else {
                        step = Some(cells);
                    }
                }
            }
            let shift = i.modifiers.shift;
            hide = i.key_pressed(Key::H) && !shift;
            show_all = i.key_pressed(Key::H) && shift;
            lock = i.key_pressed(Key::L) && !shift;
            unlock_all = i.key_pressed(Key::L) && shift;
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
                (Key::Num6, Tool::Ground),
                (Key::Num7, Tool::Surface),
                (Key::Num8, Tool::Height),
                (Key::Num9, Tool::Region),
            ] {
                if i.key_pressed(key) {
                    self.editor.tool = tool;
                }
            }
            if i.key_pressed(Key::Escape) {
                self.editor.select(None);
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
            let arrows = !selecting;
            if (arrows && i.key_down(Key::ArrowLeft)) || i.key_down(Key::A) {
                self.cam.center.x -= pan;
            }
            if (arrows && i.key_down(Key::ArrowRight)) || i.key_down(Key::D) {
                self.cam.center.x += pan;
            }
            if (arrows && i.key_down(Key::ArrowUp)) || i.key_down(Key::W) {
                self.cam.center.y -= pan;
            }
            if (arrows && i.key_down(Key::ArrowDown)) || i.key_down(Key::S) {
                self.cam.center.y += pan;
            }
        });
        if let Some(delta) = step {
            self.nudge_selection(delta);
        }
        if let Some(delta) = pixel {
            self.nudge_pixels(delta);
        }
        if hide {
            self.toggle_hidden();
        }
        if show_all {
            self.show_all();
        }
        if lock {
            self.toggle_locked();
        }
        if unlock_all {
            self.unlock_all();
        }
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
            (_, Workspace::Models, _, Some(m)) if m.entry.id == 0 => {
                let name = m.entry.path.rsplit('/').next().unwrap_or(&m.entry.path);
                format!("{name} — {project} — C3 Studio")
            }
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
        if let Some((blueprint, node)) = self.playing.as_mut().and_then(|p| p.check_breakpoint()) {
            self.notify(format!(
                "Breakpoint: blueprint {blueprint}, node {node}; Play paused"
            ));
        }
        self.hot_reload();
        self.serve_agents();
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
        self.rename_window(&ctx);
        self.column_text_window(&ctx);
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
