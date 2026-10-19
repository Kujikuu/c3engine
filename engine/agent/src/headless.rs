//! A project open without the Studio: documents open from the files on first use and are
//! written back by [`Workspace::save`], as the Studio writes them.

use crate::workspace::{DocRef, Workspace};
use c3_edit::{AssetDoc, DialogueDoc, SceneDoc, ScriptDoc, SettingsDoc, TableDoc, ValueDoc};
use c3_play::GameModule;
use c3_project::{AssetDb, Project};
use c3_scene::TileLayer;
use c3_script::blueprint::{self, Blueprint};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

/// An open map and its ground as last saved (a client map's ground is its own file, written
/// when it changed).
struct OpenMap {
    doc: SceneDoc,
    saved_ground: Option<TileLayer>,
}

/// A project and the documents an agent has open in it.
pub struct Headless {
    project: Project,
    game: Rc<dyn GameModule>,
    maps: BTreeMap<String, OpenMap>,
    tables: BTreeMap<String, TableDoc>,
    settings: Option<SettingsDoc>,
    dialogues: BTreeMap<String, DialogueDoc>,
    scripts: BTreeMap<String, ScriptDoc>,
    blueprints: BTreeMap<String, ValueDoc<Blueprint>>,
    assets: Option<AssetDoc>,
}

impl Headless {
    /// `project`, played by `game`.
    pub fn new(project: Project, game: Rc<dyn GameModule>) -> Self {
        Self {
            project,
            game,
            maps: BTreeMap::new(),
            tables: BTreeMap::new(),
            settings: None,
            dialogues: BTreeMap::new(),
            scripts: BTreeMap::new(),
            blueprints: BTreeMap::new(),
            assets: None,
        }
    }

    fn script_file(&self, name: &str) -> PathBuf {
        (self.project.scripts_dir()).join(format!("{name}{}", c3_script::SCRIPT_SUFFIX))
    }

    fn blueprint_file(&self, name: &str) -> PathBuf {
        (self.project.scripts_dir().join(blueprint::BLUEPRINT_DIR))
            .join(format!("{name}{}", blueprint::BLUEPRINT_SUFFIX))
    }

    /// Writes the open document `doc`.
    fn save_one(&mut self, doc: &DocRef) -> Result<(), String> {
        let project = &self.project;
        let text = |e: std::io::Error| e.to_string();
        match doc {
            DocRef::Map(key) => {
                let map = self.maps.get_mut(key).ok_or("not open")?;
                let scene = map.doc.scene();
                let ground = scene
                    .ground
                    .as_ref()
                    .filter(|g| Some(*g) != map.saved_ground.as_ref());
                self.game.save_map(project, key, scene, ground)?;
                map.saved_ground = scene.ground.clone();
                map.doc.mark_saved();
            }
            DocRef::Table(name) => {
                let table = self.tables.get_mut(name).ok_or("not open")?;
                c3_data::save_table(table.table(), &project.table_file(name)).map_err(text)?;
                table.mark_saved();
            }
            DocRef::Settings => {
                let settings = self.settings.as_mut().ok_or("not open")?;
                project
                    .save_settings(settings.settings())
                    .map_err(|e| e.to_string())?;
                settings.mark_saved();
            }
            DocRef::Dialogue(name) => {
                let dialogue = self.dialogues.get_mut(name).ok_or("not open")?;
                let graph = dialogue.graph();
                c3_script::save_dialogue(graph, &project.dialogue_file(name)).map_err(text)?;
                let script = (project.scripts_dir()).join(format!(
                    "{}{}",
                    c3_script::generated_script(name),
                    c3_script::SCRIPT_SUFFIX
                ));
                if let Some(dir) = script.parent() {
                    std::fs::create_dir_all(dir).map_err(text)?;
                }
                std::fs::write(&script, graph.to_script(name)).map_err(text)?;
                dialogue.mark_saved();
            }
            DocRef::Script(name) => {
                let file = self.script_file(name);
                let script = self.scripts.get_mut(name).ok_or("not open")?;
                std::fs::write(file, script.text()).map_err(text)?;
                script.mark_saved();
            }
            DocRef::Blueprint(name) => {
                let file = self.blueprint_file(name);
                let doc = self.blueprints.get_mut(name).ok_or("not open")?;
                blueprint::save_blueprint(doc.value(), &file).map_err(text)?;
                blueprint::build_blueprint(&project.scripts_dir(), name).map_err(text)?;
                doc.mark_saved();
            }
            // Asset commands change the files as they run.
            DocRef::Assets => {}
        }
        Ok(())
    }
}

/// The open document `name` of `docs`, opened with `open` when it isn't yet.
fn open<'a, T>(
    docs: &'a mut BTreeMap<String, T>,
    name: &str,
    open: impl FnOnce() -> Result<T, String>,
) -> Result<&'a mut T, String> {
    if !docs.contains_key(name) {
        let doc = open()?;
        docs.insert(name.to_owned(), doc);
    }
    Ok(docs.get_mut(name).expect("inserted above"))
}

/// `file`, or why it can't be opened as a `kind`.
fn existing(file: PathBuf, kind: &str, name: &str) -> Result<PathBuf, String> {
    if file.is_file() {
        Ok(file)
    } else {
        Err(format!(
            "the project has no {kind} \"{name}\" (create makes one)"
        ))
    }
}

impl Workspace for Headless {
    fn project(&self) -> &Project {
        &self.project
    }

    fn game(&self) -> &dyn GameModule {
        &*self.game
    }

    fn scene(&mut self, map: &str) -> Result<&mut SceneDoc, String> {
        let (game, project) = (&self.game, &self.project);
        let open = open(&mut self.maps, map, || {
            let scene = game.load_map(project, map)?;
            Ok(OpenMap {
                saved_ground: scene.ground.clone(),
                doc: SceneDoc::new(scene),
            })
        })?;
        Ok(&mut open.doc)
    }

    fn table(&mut self, name: &str) -> Result<&mut TableDoc, String> {
        let file = existing(self.project.table_file(name), "table", name)?;
        open(&mut self.tables, name, || {
            c3_data::load_table(&file)
                .map(TableDoc::new)
                .map_err(|e| e.to_string())
        })
    }

    fn settings(&mut self) -> Result<&mut SettingsDoc, String> {
        if self.settings.is_none() {
            let defaults = self.game.default_settings();
            let own = (self.project.load_settings_over(&defaults)).map_err(|e| e.to_string())?;
            self.settings = Some(SettingsDoc::new(
                own.unwrap_or_else(|| defaults.clone()),
                defaults,
            ));
        }
        Ok(self.settings.as_mut().expect("opened above"))
    }

    fn dialogue(&mut self, name: &str) -> Result<&mut DialogueDoc, String> {
        let file = existing(self.project.dialogue_file(name), "dialogue", name)?;
        open(&mut self.dialogues, name, || {
            c3_script::load_dialogue(&file)
                .map(DialogueDoc::new)
                .map_err(|e| e.to_string())
        })
    }

    fn script(&mut self, name: &str) -> Result<&mut ScriptDoc, String> {
        let file = existing(self.script_file(name), "script", name)?;
        open(&mut self.scripts, name, || {
            std::fs::read_to_string(&file)
                .map(ScriptDoc::new)
                .map_err(|e| e.to_string())
        })
    }

    fn blueprint(&mut self, name: &str) -> Result<&mut ValueDoc<Blueprint>, String> {
        let file = existing(self.blueprint_file(name), "blueprint", name)?;
        open(&mut self.blueprints, name, || {
            blueprint::load_blueprint(&file)
                .map(ValueDoc::new)
                .map_err(|e| e.to_string())
        })
    }

    fn assets(&mut self) -> Result<&mut AssetDoc, String> {
        if self.assets.is_none() {
            let (db, _) = AssetDb::of(&self.project);
            self.assets = Some(AssetDoc::new(self.project.clone(), db));
        }
        Ok(self.assets.as_mut().expect("opened above"))
    }

    fn save(&mut self) -> Result<Vec<String>, String> {
        let mut saved = Vec::new();
        for name in self.unsaved() {
            let doc = DocRef::parse(&name)?;
            self.save_one(&doc).map_err(|e| format!("{name}: {e}"))?;
            saved.push(name);
        }
        Ok(saved)
    }

    fn unsaved(&self) -> Vec<String> {
        let named = |kind: fn(String) -> DocRef, name: &String| kind(name.clone()).name();
        let maps = self.maps.iter().filter(|(_, m)| m.doc.is_dirty());
        let mut unsaved: Vec<String> = maps.map(|(n, _)| named(DocRef::Map, n)).collect();
        let tables = self.tables.iter().filter(|(_, d)| d.is_dirty());
        unsaved.extend(tables.map(|(n, _)| named(DocRef::Table, n)));
        if self.settings.as_ref().is_some_and(SettingsDoc::is_dirty) {
            unsaved.push(DocRef::Settings.name());
        }
        let dialogues = self.dialogues.iter().filter(|(_, d)| d.is_dirty());
        unsaved.extend(dialogues.map(|(n, _)| named(DocRef::Dialogue, n)));
        let scripts = self.scripts.iter().filter(|(_, d)| d.is_dirty());
        unsaved.extend(scripts.map(|(n, _)| named(DocRef::Script, n)));
        let blueprints = self.blueprints.iter().filter(|(_, d)| d.is_dirty());
        unsaved.extend(blueprints.map(|(n, _)| named(DocRef::Blueprint, n)));
        unsaved
    }
}
