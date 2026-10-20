//! A game played in a project: the game's module with the project's own assets layered over its
//! content, so a Conquer project can use plain PNG sprites and WAV sounds beside its TQ files,
//! and a project made from scratch has nothing else. Apps open every game through
//! [`ProjectGame`].

use crate::character::{Character, CharacterMotions, Look};
use crate::module::{Act, GameModule, GameRules, Looks};
use crate::native::NativeAssets;
use c3_assets::Image;
use c3_assets::effect::Effect;
use c3_assets::sprite::{Prefab, SpriteSet};
use c3_audio::MusicRegion;
use c3_game::{EntityId, Sim, SkillType, StatusType};
use c3_project::{Project, ProjectSettings};
use c3_render::SceneAssets;
use c3_scene::{Scene, TileLayer};
use std::rc::Rc;
use std::sync::Arc;

/// `game` playing `project`, with the project's assets first.
pub struct ProjectGame {
    game: Rc<dyn GameModule>,
    assets: Layered,
    /// The project's `maps/` folder, where generated minimaps are.
    maps_dir: std::path::PathBuf,
}

/// The project's assets, then the game's.
struct Layered {
    native: NativeAssets,
    game: Rc<dyn GameModule>,
}

impl ProjectGame {
    /// `game` with `project`'s assets; also returns what was wrong with their sidecars.
    pub fn open(project: &Project, game: Rc<dyn GameModule>) -> (Self, Vec<String>) {
        let (native, problems) = NativeAssets::new(project);
        let assets = Layered {
            native,
            game: Rc::clone(&game),
        };
        let maps_dir = project.maps_dir();
        (
            Self {
                game,
                assets,
                maps_dir,
            },
            problems,
        )
    }
}

impl SceneAssets for Layered {
    fn sprite_set(&self, path: &str) -> Option<SpriteSet> {
        self.native
            .sprite_set(path)
            .or_else(|| self.game.assets().sprite_set(path))
    }

    fn prefab(&self, path: &str) -> Option<Prefab> {
        self.native
            .prefab(path)
            .or_else(|| self.game.assets().prefab(path))
    }

    fn tile_layer(&self, path: &str) -> Option<TileLayer> {
        self.native
            .tile_layer(path)
            .or_else(|| self.game.assets().tile_layer(path))
    }

    fn image(&self, path: &str) -> Option<Image> {
        self.native
            .image(path)
            .or_else(|| self.game.assets().image(path))
    }

    fn effect(&self, name: &str) -> Result<Effect, String> {
        self.native
            .effect(name)
            .or_else(|_| self.game.assets().effect(name))
    }
}

impl GameModule for ProjectGame {
    fn name(&self) -> &str {
        self.game.name()
    }

    fn assets(&self) -> &dyn SceneAssets {
        &self.assets
    }

    fn looks(&self) -> &dyn Looks {
        &self.assets
    }

    fn rules(&self) -> Arc<dyn GameRules> {
        self.game.rules()
    }

    fn default_settings(&self) -> ProjectSettings {
        self.game.default_settings()
    }

    fn maps(&self, project: &Project) -> Vec<String> {
        self.game.maps(project)
    }

    fn default_map(&self, project: &Project) -> Option<String> {
        self.game.default_map(project)
    }

    fn load_map(&self, project: &Project, key: &str) -> Result<Scene, String> {
        self.game.load_map(project, key)
    }

    fn server_data_kind(&self) -> Option<&str> {
        self.game.server_data_kind()
    }

    fn import_server_data(&self, dump: &[u8]) -> Option<crate::server_data::ServerData> {
        self.game.import_server_data(dump)
    }

    fn save_map(
        &self,
        project: &Project,
        key: &str,
        scene: &Scene,
        ground: Option<&c3_scene::TileLayer>,
    ) -> Result<(), String> {
        self.game.save_map(project, key, scene, ground)
    }

    fn map_ids(&self, key: &str) -> Vec<u32> {
        self.game.map_ids(key)
    }

    fn tilt_deg(&self) -> f32 {
        self.game.tilt_deg()
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        self.assets
            .native
            .read(path)
            .or_else(|| self.game.read_file(path))
    }

    fn item_glow(&self, kind: u32) -> Vec<Image> {
        self.game.item_glow(kind)
    }

    fn chat_look(&self) -> Option<c3_ui::ChatLook> {
        self.game.chat_look()
    }

    fn battle_power(&self, sim: &c3_game::Sim, p: &c3_game::Player) -> Option<c3_ui::Power> {
        self.game.battle_power(sim, p)
    }

    fn item_icon(&self, kind: u32, look: u32) -> Option<Image> {
        self.game.item_icon(kind, look)
    }

    fn slot_names(&self) -> Vec<(u8, String)> {
        self.game.slot_names()
    }

    fn table_templates(&self) -> Vec<c3_data::Table> {
        self.game.table_templates()
    }

    fn formulas_template(&self) -> Option<&'static str> {
        self.game.formulas_template()
    }

    fn click_effect(&self) -> Option<String> {
        self.game.click_effect()
    }

    fn data_tables(&self) -> crate::module::DataTables {
        self.game.data_tables()
    }

    fn npc_looks(&self) -> Vec<(u32, String)> {
        self.game.npc_looks()
    }

    /// The project's generated minimap, else the game's own.
    fn minimap(&self, key: &str, map_ids: &[u32]) -> Option<Image> {
        let file = c3_project::minimap_file_in(&self.maps_dir, key);
        let own = image::open(file).ok().map(|img| {
            let rgba = img.into_rgba8();
            Image::new(rgba.width(), rgba.height(), rgba.into_raw())
        });
        own.or_else(|| self.game.minimap(key, map_ids))
    }

    fn portal_effect(&self) -> Option<String> {
        self.game.portal_effect()
    }

    fn login_list(
        &self,
        address: &str,
        read: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) -> Option<crate::module::LoginList> {
        self.game.login_list(address, read)
    }

    fn map_key(&self, id: u32) -> Option<String> {
        self.game.map_key(id)
    }

    fn radar_mark(&self, kind: &str) -> Option<Image> {
        self.game.radar_mark(kind)
    }

    fn skill_media(&self, skill: &SkillType) -> (Option<String>, Option<String>) {
        self.game.skill_media(skill)
    }

    fn status_effect(&self, status: &StatusType) -> Option<String> {
        self.game.status_effect(status)
    }

    fn level_up_effect(&self) -> Option<String> {
        self.game.level_up_effect()
    }

    fn item_use(&self, kind: u32) -> (Option<String>, Option<String>) {
        self.game.item_use(kind)
    }

    /// A project animation set's sound for the action, else the game's.
    fn action_sound(&self, sim: &Sim, who: EntityId, act: Act) -> Option<String> {
        let looks = self.game.looks();
        let look = match (sim.player(who), sim.monster(who)) {
            (Some(p), _) => Some(looks.player(p)),
            (_, Some(m)) => Some(looks.monster(m.kind, sim.rules())),
            _ => None,
        };
        let own = (look.and_then(|l| self.assets.native.character_for(&l)))
            .and_then(|set| set.action(act).map(|a| a.sound.clone()))
            .filter(|s| !s.is_empty());
        own.or_else(|| self.game.action_sound(sim, who, act))
    }

    fn music_regions(&self) -> Vec<MusicRegion> {
        self.game.music_regions()
    }

    fn native(&self) -> Option<&NativeAssets> {
        Some(&self.assets.native)
    }
}

/// Roles dressed as the game dresses them, unless the project has an animation set for
/// their look: then its model, animations and timing.
impl Looks for Layered {
    fn player(&self, p: &c3_game::Player) -> Look {
        self.game.looks().player(p)
    }

    fn monster(&self, kind: u32, rules: &dyn c3_game::Rules) -> Look {
        self.game.looks().monster(kind, rules)
    }

    fn npc(&self, npc: &c3_scene::Npc) -> Look {
        self.game.looks().npc(npc)
    }

    fn default_player(&self) -> Look {
        self.game.looks().default_player()
    }

    fn character(&self, look: &Look, act: u32) -> Result<Character, String> {
        match self.native.character_for(look) {
            Some(set) => set.build(self.act(act), &self.native),
            None => self.game.looks().character(look, act),
        }
    }

    fn motions(&self, character: &Character, look: &Look, act: u32) -> CharacterMotions {
        match self.native.character_for(look) {
            // A set's model carries the action's animation already.
            Some(_) => CharacterMotions::default(),
            None => self.game.looks().motions(character, look, act),
        }
    }

    fn frame_ms(&self, look: &Look, act: u32) -> u32 {
        match self.native.character_for(look) {
            Some(set) => set.frame_ms(self.act(act)),
            None => self.game.looks().frame_ms(look, act),
        }
    }

    fn part_effects(&self, key: u64, act: u32) -> Vec<String> {
        self.game.looks().part_effects(key, act)
    }

    fn action(&self, act: Act) -> u32 {
        self.game.looks().action(act)
    }

    fn action_names(&self) -> Vec<(u32, String)> {
        self.game.looks().action_names()
    }
}

impl Layered {
    /// The engine action the game's action `act` is (standing when it is none of them).
    fn act(&self, act: u32) -> Act {
        let looks = self.game.looks();
        (crate::native_character::ACTS.into_iter())
            .find(|a| looks.action(*a) == act)
            .unwrap_or(Act::Stand)
    }
}
