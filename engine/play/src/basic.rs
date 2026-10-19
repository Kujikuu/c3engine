//! The engine's own game: plays a project made from scratch, with no game content of another
//! engine. Its maps are the project's scenes; its art is the engine's built-in placeholders
//! (the project's own images come through [`crate::project_game::ProjectGame`]); roles are
//! drawn as plain figures until the project gives them models; the rules are the project's
//! tables and formulas ([`crate::data_rules`]).

use crate::character::{Character, CharacterMotions, Look, LookKind, PartModel};
use crate::data_rules::DataGameRules;
use crate::module::{Act, GameModule, GameRules, Looks, load_project_scene};
use c3_assets::Image;
use c3_assets::effect::Effect;
use c3_assets::sprite::{Prefab, SpriteSet};
use c3_game::{Player, Rules};
use c3_project::Project;
use c3_render::SceneAssets;
use c3_scene::{Grid, IsoProjection, Npc, Scene, TileLayer};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::builtin::{self, GROUND_IMAGE, GROUND_TILE};
/// Cell size of new maps, in world pixels (the classic 2:1 diamond).
/// Action ids of the engine's own game.
const STAND: u32 = 100;
const WALK: u32 = 110;
const RUN: u32 = 120;
const RUN_ALT: u32 = 121;
const ATTACK: u32 = 400;
const DIE: u32 = 330;
const JUMP: u32 = 130;

/// The engine's own game.
pub struct BasicGame {
    assets: BasicAssets,
    rules: Arc<DataGameRules>,
}

impl Default for BasicGame {
    fn default() -> Self {
        Self {
            assets: BasicAssets,
            rules: Arc::new(DataGameRules),
        }
    }
}

impl GameModule for BasicGame {
    fn name(&self) -> &str {
        "C3 game"
    }

    fn assets(&self) -> &dyn SceneAssets {
        &self.assets
    }

    fn looks(&self) -> &dyn Looks {
        self
    }

    fn rules(&self) -> Arc<dyn GameRules> {
        self.rules.clone()
    }

    fn maps(&self, project: &Project) -> Vec<String> {
        project.map_names()
    }

    fn load_map(&self, project: &Project, key: &str) -> Result<Scene, String> {
        load_project_scene(project, key)
            .unwrap_or_else(|| Err(format!("the project has no map \"{key}\"")))
    }

    fn tilt_deg(&self) -> f32 {
        -30.0
    }

    fn read_file(&self, _path: &str) -> Option<Vec<u8>> {
        None
    }

    fn slot_names(&self) -> Vec<(u8, String)> {
        [
            (1, "Head"),
            (3, "Body"),
            (4, "Right hand"),
            (5, "Left hand"),
        ]
        .map(|(s, n)| (s, n.to_owned()))
        .to_vec()
    }

    fn table_templates(&self) -> Vec<c3_data::Table> {
        crate::data_rules::table_templates()
    }

    fn formulas_template(&self) -> Option<&'static str> {
        Some(crate::data_rules::FORMULAS_TEMPLATE)
    }
}

impl Looks for BasicGame {
    /// The body and hair the player chose, else the default's.
    fn player(&self, p: &Player) -> Look {
        let default = self.default_player();
        let chosen = |v: u32, d: u32| if v == 0 { d } else { v };
        Look {
            body: chosen(p.look.body, default.body),
            hair: chosen(p.look.hair, default.hair),
            ..default
        }
    }

    fn monster(&self, kind: u32, _rules: &dyn Rules) -> Look {
        Look {
            kind: LookKind::Monster,
            body: kind,
            ..Look::default()
        }
    }

    fn npc(&self, npc: &Npc) -> Look {
        Look {
            kind: LookKind::Npc,
            body: npc.look,
            ..Look::default()
        }
    }

    /// Players are look 1 (a project's animation set for look 1 dresses them).
    fn default_player(&self) -> Look {
        Look {
            body: 1,
            ..Look::default()
        }
    }

    fn character(&self, look: &Look, _act: u32) -> Result<Character, String> {
        Ok(Character::single(PartModel::plain(
            builtin::figure(),
            Some(builtin::figure_image(look.kind).to_owned()),
        )))
    }

    fn motions(&self, _character: &Character, _look: &Look, _act: u32) -> CharacterMotions {
        CharacterMotions::default()
    }

    fn frame_ms(&self, _look: &Look, act: u32) -> u32 {
        // A figure has one frame: an action lasts one frame, so steps take this long.
        match act {
            WALK => 500,
            RUN | RUN_ALT => 250,
            _ => 400,
        }
    }

    fn action(&self, act: Act) -> u32 {
        match act {
            Act::Stand => STAND,
            Act::Walk => WALK,
            Act::Run => RUN,
            Act::RunAlt => RUN_ALT,
            Act::Attack => ATTACK,
            Act::Die => DIE,
            Act::Jump => JUMP,
        }
    }

    fn action_names(&self) -> Vec<(u32, String)> {
        [
            (STAND, "Stand"),
            (WALK, "Walk"),
            (RUN, "Run"),
            (ATTACK, "Attack"),
            (DIE, "Die"),
        ]
        .map(|(a, n)| (a, n.to_owned()))
        .to_vec()
    }
}

/// The images the engine draws itself.
struct BasicAssets;

impl SceneAssets for BasicAssets {
    fn sprite_set(&self, _path: &str) -> Option<SpriteSet> {
        None
    }

    fn prefab(&self, _path: &str) -> Option<Prefab> {
        None
    }

    fn tile_layer(&self, _path: &str) -> Option<TileLayer> {
        None
    }

    fn image(&self, path: &str) -> Option<Image> {
        builtin::image(path)
    }

    fn effect(&self, name: &str) -> Result<Effect, String> {
        Err(format!("the project has no effect \"{name}\""))
    }
}

/// The ground image of a new map and the size of its squares.
pub struct GroundImage<'a> {
    pub image: &'a str,
    pub tile_size: u32,
}

impl GroundImage<'_> {
    /// The engine's own ground.
    pub const BUILTIN: GroundImage<'static> = GroundImage {
        image: GROUND_IMAGE,
        tile_size: GROUND_TILE,
    };
}

/// A blank map `width` × `height` cells: all walkable, covered with `ground`.
pub fn blank_scene(width: u32, height: u32, cell: [f32; 2], ground: GroundImage) -> Scene {
    let [cell_w, cell_h] = cell;
    let grid = Grid::new(width.max(1), height.max(1));
    let projection = IsoProjection::for_grid(cell_w, cell_h, grid.width);
    let ground = TileLayer::covering(ground.image, ground.tile_size, &projection, &grid);
    Scene {
        grid,
        projection,
        ground: Some(ground),
        portals: Vec::new(),
        entities: Vec::new(),
        layers: Vec::new(),
        npcs: Vec::new(),
        spawns: Vec::new(),
        regions: Vec::new(),
        traps: Vec::new(),
        meta: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_map_is_walkable_and_has_ground() {
        let scene = blank_scene(40, 30, [64.0, 32.0], GroundImage::BUILTIN);
        assert_eq!((scene.grid.width, scene.grid.height), (40, 30));
        assert!(scene.grid.cells.iter().all(|c| !c.is_blocked()));
        let ground = scene.ground.as_ref().expect("ground");
        // The ground reaches every corner cell of the map.
        for cell in [(0, 0), (39, 0), (0, 29), (39, 29)] {
            let (x, y) = scene.cell_to_world(cell);
            let (gx, gy) = (x as i32 - ground.origin.0, y as i32 - ground.origin.1);
            let (w, h) = ground.world_size();
            assert!(
                gx >= 0 && gy >= 0 && gx < w as i32 && gy < h as i32,
                "{cell:?}"
            );
        }
    }
}
