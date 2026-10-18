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
    /// The game's ids of map `key` (script teleports, music regions, protocols).
    fn map_ids(&self, key: &str) -> Vec<u32> {
        let _ = key;
        Vec::new()
    }
    /// Camera tilt of 3D roles and effects on maps, in degrees.
    fn tilt_deg(&self) -> f32;
    /// A file of the game's content (sounds), loose or packed.
    fn read_file(&self, path: &str) -> Option<Vec<u8>>;
    /// The icon of item type `kind`.
    fn item_icon(&self, kind: u32) -> Option<Image> {
        let _ = kind;
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
    /// The map drawn small for the radar, covering the map's ground (or its cells when it
    /// has none): map `key`, answering to `map_ids`.
    fn minimap(&self, key: &str, map_ids: &[u32]) -> Option<Image> {
        let _ = (key, map_ids);
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

/// Map `key` of `project` when it is one of the project's own scenes.
pub fn load_project_scene(project: &Project, key: &str) -> Option<Result<Scene, String>> {
    project.map_names().iter().any(|n| n == key).then(|| {
        c3_scene::native::load_scene(&project.map_file(key)).map_err(|e| format!("map {key}: {e}"))
    })
}
