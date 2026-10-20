//! The seam between the engine and a game: what the engine asks a game when it plays one.
//! The client, the server and the Studio's Scene, Play and Preview all go through a
//! [`GameModule`], so any project plays, whichever game it is: Conquer's module reads a TQ
//! client, the engine's own ([`crate::basic`]) plays a project made from scratch.

use crate::character::{Character, CharacterMotions, Look};
use c3_assets::Image;
use c3_audio::MusicRegion;
use c3_game::{EntityId, Player, Rules, Sim, SkillType, StatusType};
use c3_project::{Project, ProjectSettings};
use c3_render::SceneAssets;
use c3_scene::{Npc, Scene};
use std::path::Path;
use std::sync::Arc;

/// What every game animates. A game maps them to its own action ids ([`Looks::action`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Stand,
    Walk,
    /// Running steps alternate between `Run` and `RunAlt` (left and right foot).
    Run,
    RunAlt,
    Attack,
    Die,
    Jump,
}

/// How an emote plays (see [`Looks::emote_play`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmotePlay {
    Once,
    Hold,
    Loop,
}

/// How a game dresses and animates its roles.
pub trait Looks {
    /// What player `p` looks like in what it wears.
    fn player(&self, p: &Player) -> Look;
    /// What a monster of type `kind` looks like.
    fn monster(&self, kind: u32, rules: &dyn Rules) -> Look;
    fn npc(&self, npc: &Npc) -> Look;
    /// A player to preview when none is chosen.
    fn default_player(&self) -> Look;
    /// The models of `look`, posed for action `act`.
    fn character(&self, look: &Look, act: u32) -> Result<Character, String>;
    /// `character`'s motions for action `act` (empty when its models keep their own).
    fn motions(&self, character: &Character, look: &Look, act: u32) -> CharacterMotions;
    /// How emote `emote` plays: once, held on its last frame (sitting), or looping (a
    /// dance) until the role moves.
    fn emote_play(&self, emote: u32) -> EmotePlay {
        let _ = emote;
        EmotePlay::Once
    }
    /// Milliseconds per animation frame of action `act`.
    fn frame_ms(&self, look: &Look, act: u32) -> u32;
    /// Effects a part starts when its role begins action `act` (weapon trails); `"none"`
    /// stops the part's effects. `key` is the part's [`crate::character::PartModel::effect_key`].
    fn part_effects(&self, key: u64, act: u32) -> Vec<String> {
        let _ = (key, act);
        Vec::new()
    }
    /// The game's id of action `act`.
    fn action(&self, act: Act) -> u32;
    /// Actions with readable names, for pickers.
    fn action_names(&self) -> Vec<(u32, String)>;
}

/// A game's rules. `Send`, so a game server can run them on its own thread.
pub trait GameRules: Send + Sync {
    /// The rules, from the project's `tables/` (when it has them), and what was wrong there.
    fn rules(&self, tables: Option<&Path>) -> (Box<dyn Rules>, Vec<String>);
}

/// A table of the project holding one kind of the game's things.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRef {
    pub table: String,
    /// The column holding the rules' id of each row; `None`: the row's key.
    pub id_column: Option<String>,
}

impl TableRef {
    /// Table `table`, its rows' keys being the rules' ids.
    pub fn keyed(table: &str) -> Self {
        Self {
            table: table.to_owned(),
            id_column: None,
        }
    }

    /// The rules' id of row `key` of `table` (this table, open).
    pub fn id_of(&self, table: &c3_data::Table, key: i64) -> Option<u32> {
        let value = match &self.id_column {
            Some(column) => table.get(key, column)?.as_int()?,
            None => key,
        };
        u32::try_from(value).ok()
    }
}

/// The tables the Studio's forms know (see [`GameModule::data_tables`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataTables {
    pub monsters: TableRef,
    pub items: TableRef,
    pub skills: TableRef,
    pub levels: TableRef,
}

/// A login list: server groups and the servers in them.
pub type LoginList = (
    Vec<c3_core::settings::ServerGroup>,
    Vec<c3_core::settings::ServerEntry>,
);

/// A game the engine plays.
pub trait GameModule {
    /// The game's name, for titles and the Studio.
    fn name(&self) -> &str;
    /// Art for maps: sprites, ground, images and effects.
    fn assets(&self) -> &dyn SceneAssets;
    fn looks(&self) -> &dyn Looks;
    fn rules(&self) -> Arc<dyn GameRules>;
    /// The settings of a project of this game that has no settings file yet.
    fn default_settings(&self) -> ProjectSettings {
        ProjectSettings::default()
    }
    /// Maps of `project` that can be played, by key.
    fn maps(&self, project: &Project) -> Vec<String>;
    /// The map played when none is named.
    fn default_map(&self, project: &Project) -> Option<String> {
        self.maps(project).into_iter().next()
    }
    /// Loads map `key` of `project`, with the population the project keeps for it.
    fn load_map(&self, project: &Project, key: &str) -> Result<Scene, String>;
    /// Writes `scene` as map `key` of `project`; `ground` is its ground layer when that
    /// changed (a game's own map files keep their ground apart). The project's own scenes
    /// save as such; a game saves its own map files.
    fn save_map(
        &self,
        project: &Project,
        key: &str,
        scene: &Scene,
        ground: Option<&c3_scene::TileLayer>,
    ) -> Result<(), String> {
        let _ = ground;
        save_project_scene(project, key, scene)
            .unwrap_or_else(|| Err(format!("the project has no map \"{key}\"")))
    }
    /// The key of the game's map `id` when the project's map list has none (a map file the
    /// game knows by number: TQ's map docs).
    fn map_key(&self, id: u32) -> Option<String> {
        let _ = id;
        None
    }
    /// The game's ids of map `key` (script teleports, music regions, protocols).
    fn map_ids(&self, key: &str) -> Vec<u32> {
        let _ = key;
        Vec::new()
    }
    /// Camera tilt of 3D roles and effects on maps, in degrees.
    fn tilt_deg(&self) -> f32;
    /// A file of the game's content (sounds), loose or packed.
    fn read_file(&self, path: &str) -> Option<Vec<u8>>;
    /// The game's own login list (its groups and servers) for a project that lists none,
    /// each server at `address` (this project's server); `read` reads the game's files, a
    /// project's own copies first.
    fn login_list(
        &self,
        address: &str,
        read: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) -> Option<LoginList> {
        let _ = (address, read);
        None
    }
    /// Player `p`'s battle power as the game counts it; none: the game counts none.
    fn battle_power(&self, sim: &c3_game::Sim, p: &c3_game::Player) -> Option<c3_ui::Power> {
        let _ = (sim, p);
        None
    }
    /// The game's auto-path destinations, every map's; none: no auto-path menu.
    fn destinations(&self) -> Vec<c3_ui::Destination> {
        Vec::new()
    }
    /// How the game shows its chat log; none: the engine's.
    fn chat_look(&self) -> Option<c3_ui::ChatLook> {
        None
    }
    /// The pictures of the glow played under item type `kind`'s icon, in turn a tenth of a
    /// second each (TQ's quality lights); none: no glow.
    fn item_glow(&self, kind: u32) -> Vec<Image> {
        let _ = kind;
        Vec::new()
    }
    /// The icon of item type `kind` in look `look` (an item's look attribute; 0: its usual).
    fn item_icon(&self, kind: u32, look: u32) -> Option<Image> {
        let _ = (kind, look);
        None
    }
    /// The equipment slots and their names, in display order.
    fn slot_names(&self) -> Vec<(u8, String)>;
    /// The tables the game's rules read, each with an example row, for a project to start
    /// from (none: the game brings its own data).
    fn table_templates(&self) -> Vec<c3_data::Table> {
        Vec::new()
    }
    /// A project's first `scripts/rules.rhai`, when the game's formulas are a script.
    fn formulas_template(&self) -> Option<&'static str> {
        None
    }
    /// The looks NPCs can have, `(look, name)`, for pickers (empty: any number).
    fn npc_looks(&self) -> Vec<(u32, String)> {
        Vec::new()
    }
    /// What the game's server data is, for menus and file dialogs (Conquer: "TQ SQL dump");
    /// `None` when it has none to import.
    fn server_data_kind(&self) -> Option<&str> {
        None
    }
    /// The project data in `dump`, the game's own server data as its file holds it (Conquer:
    /// TQ's SQL dump, its text in GBK); `None` when the game has no server data to import.
    fn import_server_data(&self, dump: &[u8]) -> Option<crate::server_data::ServerData> {
        let _ = dump;
        None
    }
    /// Which of the project's tables hold the game's monsters, items, skills and levels, for
    /// the Studio's forms (a monster row dragged onto a map makes a spawn area of it).
    fn data_tables(&self) -> DataTables {
        use crate::data_rules::{ITEMS, LEVELS, MONSTERS, SKILLS};
        DataTables {
            monsters: TableRef::keyed(MONSTERS),
            items: TableRef::keyed(ITEMS),
            skills: TableRef::keyed(SKILLS),
            levels: TableRef::keyed(LEVELS),
        }
    }
    /// The map drawn small for the radar, covering the map's ground (or its cells when it
    /// has none): map `key`, answering to `map_ids`.
    fn minimap(&self, key: &str, map_ids: &[u32]) -> Option<Image> {
        let _ = (key, map_ids);
        None
    }
    /// The picture the radar marks `kind` with (`hero`, `player`, `leader`) in place of its
    /// dot; none: the dot.
    fn radar_mark(&self, kind: &str) -> Option<Image> {
        let _ = kind;
        None
    }
    /// The effect every portal shows, looping (TQ's client draws `Exit` on each).
    fn portal_effect(&self) -> Option<String> {
        None
    }
    /// The effect marking where the player clicked to walk.
    fn click_effect(&self) -> Option<String> {
        None
    }
    /// The effect played on a player rising a level.
    fn level_up_effect(&self) -> Option<String> {
        None
    }
    /// The effect shown on each one `skill` reaches and the sound of its cast (by default the
    /// effect the skills table names).
    fn skill_media(&self, skill: &SkillType) -> (Option<String>, Option<String>) {
        (
            (!skill.effect.is_empty()).then(|| skill.effect.clone()),
            None,
        )
    }
    /// The effect its bearer shows while `status` lasts.
    fn status_effect(&self, status: &StatusType) -> Option<String> {
        (!status.effect.is_empty()).then(|| status.effect.clone())
    }
    /// The effect and sound of using item type `kind`.
    fn item_use(&self, kind: u32) -> (Option<String>, Option<String>) {
        let _ = kind;
        (None, None)
    }
    /// The sound of `who` doing `act`.
    fn action_sound(&self, sim: &Sim, who: EntityId, act: Act) -> Option<String> {
        let _ = (sim, who, act);
        None
    }
    fn music_regions(&self) -> Vec<MusicRegion> {
        Vec::new()
    }
    /// The project's own assets, when the game plays in a project
    /// ([`crate::project_game::ProjectGame`]).
    fn native(&self) -> Option<&crate::native::NativeAssets> {
        None
    }
}

/// Writes `scene` as map `key` of `project` when that is one of the project's own scenes.
pub fn save_project_scene(
    project: &Project,
    key: &str,
    scene: &Scene,
) -> Option<Result<(), String>> {
    project.map_names().iter().any(|n| n == key).then(|| {
        c3_scene::native::save_scene(scene, &project.map_file(key))
            .map_err(|e| format!("map {key}: {e}"))
    })
}

/// Map `key` of `project` when it is one of the project's own scenes.
pub fn load_project_scene(project: &Project, key: &str) -> Option<Result<Scene, String>> {
    project.map_names().iter().any(|n| n == key).then(|| {
        c3_scene::native::load_scene(&project.map_file(key)).map_err(|e| format!("map {key}: {e}"))
    })
}
