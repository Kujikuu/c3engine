//! The live link: AI agents' tool calls (`c3-mcp --live`) run on what the Studio has open.
//! A call opens what it works on (the map, the table, the dialogue) where a person would see
//! it, its changes show at once, and Ctrl+Z undoes them. A call never throws away a person's
//! unsaved work: one needing another document while the open one has changes is refused.

use crate::app::{Focus, Studio};
use crate::maps::MapRef;
use crate::shell::Panel;
use c3_agent::live::{DEFAULT_ADDR, Listener};
use c3_agent::{DocRef, Workspace};
use c3_edit::{AssetDoc, DialogueDoc, SceneDoc, ScriptDoc, SettingsDoc, TableDoc, ValueDoc};
use c3_play::GameModule;
use c3_project::Project;
use c3_script::blueprint::Blueprint;

impl Studio {
    /// Starts (or stops) taking agents' calls.
    pub(crate) fn set_live_link(&mut self, on: bool) {
        if !on {
            self.live_link = None;
            self.notify("Live link off".into());
            return;
        }
        let ctx = self.ctx.clone();
        match Listener::start(DEFAULT_ADDR, move || ctx.request_repaint()) {
            Ok(link) => {
                self.notify(format!(
                    "Live link on: agents reach this Studio at {}",
                    link.addr
                ));
                self.live_link = Some(link);
            }
            Err(e) => {
                self.error = Some(format!("Can't start the live link at {DEFAULT_ADDR}: {e}"))
            }
        }
    }

    /// Runs the agents' calls waiting.
    pub(crate) fn serve_agents(&mut self) {
        let Some(link) = &self.live_link else { return };
        let calls: Vec<_> = std::iter::from_fn(|| link.next()).collect();
        for call in calls {
            let result = match (&self.project, &self.game) {
                (Some(_), Some(_)) => c3_agent::call(&mut Live(self), &call.tool, &call.args),
                _ => Err("the Studio has no project open".into()),
            };
            match &result {
                Ok(_) => self.log_info(format!("Agent: {}", call.tool)),
                Err(e) => self.log_error(format!("Agent: {} refused: {e}", call.tool)),
            }
            call.reply(result);
        }
    }
}

/// The Studio as an agent's workspace (only made while a project and its game are open).
struct Live<'a>(&'a mut Studio);

/// Why a call can't open another document of a kind.
fn busy(what: &str) -> String {
    format!("the Studio has unsaved changes to {what}: save them first (the save tool)")
}

/// Whether two maps are the same (content paths compare without case or slash style).
fn same_map(a: &MapRef, b: &MapRef) -> bool {
    let norm = |m: &MapRef| match m {
        MapRef::Client(path) => format!("client:{}", path.replace('\\', "/").to_lowercase()),
        MapRef::Project(name) => format!("project:{name}"),
    };
    norm(a) == norm(b)
}

impl Live<'_> {
    /// The Studio's last error, or `fallback`.
    fn failed(&mut self, fallback: String) -> String {
        self.0.error.take().unwrap_or(fallback)
    }
}

impl Workspace for Live<'_> {
    fn project(&self) -> &Project {
        self.0.project.as_ref().expect("checked before the call")
    }

    fn game(&self) -> &dyn GameModule {
        &**self.0.game.as_ref().expect("checked before the call")
    }

    fn scene(&mut self, map: &str) -> Result<&mut SceneDoc, String> {
        let wanted = MapRef::parse(map);
        if !(self.0.doc.as_ref()).is_some_and(|d| same_map(&d.map, &wanted)) {
            if let Some(doc) = self.0.doc.as_ref().filter(|d| d.is_dirty()) {
                return Err(busy(&format!("map {}", doc.name())));
            }
            self.0.load_map(&wanted);
            if !(self.0.doc.as_ref()).is_some_and(|d| same_map(&d.map, &wanted)) {
                return Err(self.failed(format!("can't open map {map}")));
            }
        }
        self.0.show_panel(Panel::Scene);
        Ok(self.0.doc.as_mut().expect("opened above").scene_doc())
    }

    fn table(&mut self, name: &str) -> Result<&mut TableDoc, String> {
        if self.0.data.open.as_ref().is_none_or(|t| t.name != name) {
            if let Some(open) = self.0.data.open.as_ref().filter(|t| t.doc.is_dirty()) {
                return Err(busy(&format!("table {}", open.name)));
            }
            if !self.project().table_file(name).is_file() {
                return Err(format!("the project has no table \"{name}\""));
            }
            self.0.load_table(name);
        }
        self.0.show_panel(Panel::Data);
        if self.0.data.open.as_ref().is_none_or(|t| t.name != name) {
            return Err(self.failed(format!("can't open table {name}")));
        }
        Ok(&mut self.0.data.open.as_mut().expect("checked above").doc)
    }

    fn settings(&mut self) -> Result<&mut SettingsDoc, String> {
        (self.0.settings.as_mut())
            .map(|editor| &mut editor.doc)
            .ok_or_else(|| "the project's settings aren't loaded".to_owned())
    }

    fn dialogue(&mut self, name: &str) -> Result<&mut DialogueDoc, String> {
        if self
            .0
            .dialogues
            .open
            .as_ref()
            .is_none_or(|d| d.name != name)
        {
            if let Some(open) = self.0.dialogues.open.as_ref().filter(|d| d.is_dirty()) {
                return Err(busy(&format!("dialogue {}", open.name)));
            }
            if !self.project().dialogue_file(name).is_file() {
                return Err(format!("the project has no dialogue \"{name}\""));
            }
            self.0.load_dialogue(name);
        }
        self.0.show_panel(Panel::Dialogue);
        let open = self
            .0
            .dialogues
            .open
            .as_ref()
            .is_some_and(|d| d.name == name);
        match self.0.dialogues.recorded_doc().filter(|_| open) {
            Some(doc) => Ok(doc),
            None => Err(format!("can't open dialogue {name}")),
        }
    }

    fn script(&mut self, name: &str) -> Result<&mut ScriptDoc, String> {
        if self.0.scripts.open.as_ref().is_none_or(|s| s.name != name) {
            if let Some(open) = self.0.scripts.open.as_ref().filter(|s| s.is_dirty()) {
                return Err(busy(&format!("script {}", open.name)));
            }
            let file =
                (self.project().scripts_dir()).join(format!("{name}{}", c3_script::SCRIPT_SUFFIX));
            if !file.is_file() {
                return Err(format!("the project has no script \"{name}\""));
            }
            self.0.load_script(name);
        }
        self.0.show_panel(Panel::Script);
        let open = self.0.scripts.open.as_ref().is_some_and(|s| s.name == name);
        match self.0.scripts.recorded_doc().filter(|_| open) {
            Some(doc) => Ok(doc),
            None => Err(format!("can't open script {name}")),
        }
    }

    fn blueprint(&mut self, name: &str) -> Result<&mut ValueDoc<Blueprint>, String> {
        let names = c3_script::blueprint::blueprint_names(&self.project().scripts_dir());
        if !names.iter().any(|n| n == name) {
            return Err(format!("the project has no blueprint \"{name}\""));
        }
        // Blueprints save as they change: there is never unsaved work to keep.
        if self
            .0
            .blueprint_editor
            .as_ref()
            .is_none_or(|e| e.name != name)
        {
            self.0.open_blueprint(name);
        }
        self.0.show_panel(Panel::Blueprints);
        self.0
            .blueprint_doc()
            .ok_or_else(|| format!("can't open blueprint {name}"))
    }

    fn assets(&mut self) -> Result<&mut AssetDoc, String> {
        (self.0.assets.as_mut()).ok_or_else(|| "the project's assets aren't loaded".to_owned())
    }

    fn save(&mut self) -> Result<Vec<String>, String> {
        let unsaved = self.unsaved();
        if self.0.save_all() {
            Ok(unsaved)
        } else {
            Err(self.failed("saving failed".into()))
        }
    }

    fn unsaved(&self) -> Vec<String> {
        let s = &*self.0;
        let map = s
            .doc
            .as_ref()
            .filter(|d| d.is_dirty())
            .map(|d| match &d.map {
                MapRef::Client(path) => DocRef::Map(path.clone()),
                MapRef::Project(name) => DocRef::Map(name.clone()),
            });
        let table = (s.data.open.as_ref())
            .filter(|t| t.doc.is_dirty())
            .map(|t| DocRef::Table(t.name.clone()));
        let dialogue = (s.dialogues.open.as_ref())
            .filter(|d| d.is_dirty())
            .map(|d| DocRef::Dialogue(d.name.clone()));
        let script = (s.scripts.open.as_ref())
            .filter(|o| o.is_dirty())
            .map(|o| DocRef::Script(o.name.clone()));
        [map, table, dialogue, script]
            .into_iter()
            .flatten()
            .map(|d| d.name())
            .collect()
    }

    fn changed(&mut self, doc: &DocRef) {
        let s = &mut *self.0;
        s.focus = match doc {
            DocRef::Map(_) => {
                if let Some(doc) = &s.doc {
                    s.editor.forget_missing(doc.scene());
                }
                Focus::Scene
            }
            DocRef::Table(_) => {
                s.data.changed();
                Focus::Data
            }
            DocRef::Settings => {
                s.settings_changed();
                Focus::Settings
            }
            DocRef::Dialogue(_) => {
                s.dialogues.doc_changed();
                Focus::Dialogue
            }
            DocRef::Script(_) => {
                s.scripts.doc_changed();
                Focus::Script
            }
            DocRef::Blueprint(_) => {
                s.blueprint_doc_changed();
                Focus::Blueprint
            }
            DocRef::Assets => {
                s.refresh_assets();
                Focus::Assets
            }
        };
    }

    fn refresh(&mut self) {
        let s = &mut *self.0;
        s.refresh_maps();
        s.refresh_tables();
        s.refresh_scripts();
        s.refresh_dialogues();
        s.refresh_assets();
    }
}
