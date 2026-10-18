//! A game played in a project: the game's module with the project's own assets layered over its
//! content, so a Conquer project can use plain PNG sprites and WAV sounds beside its TQ files,
//! and a project made from scratch has nothing else. Apps open every game through
//! [`ProjectGame`].

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
        self.game.looks()
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

    fn item_icon(&self, kind: u32) -> Option<Image> {
        self.game.item_icon(kind)
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

    fn monster_table(&self) -> &str {
        self.game.monster_table()
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

    fn action_sound(&self, sim: &Sim, who: EntityId, act: Act) -> Option<String> {
        self.game.action_sound(sim, who, act)
    }

    fn music_regions(&self) -> Vec<MusicRegion> {
        self.game.music_regions()
    }

    fn native(&self) -> Option<&NativeAssets> {
        Some(&self.assets.native)
    }
}
