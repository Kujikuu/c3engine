//! The engine's own game: plays a project made from scratch, with no game content of another
//! engine. Its maps are the project's scenes; its art is the engine's built-in placeholders
//! (the project's own images come through [`crate::project_game::ProjectGame`]); roles are
//! drawn as plain figures until the project gives them models; the rules are simple defaults.
//! Later steps of the roadmap replace these defaults with the project's data.

use crate::character::{Character, CharacterMotions, Look, LookKind, PartModel};
use crate::module::{Act, GameModule, GameRules, Looks, load_project_scene};
use c3_assets::Image;
use c3_assets::effect::Effect;
use c3_assets::sprite::{Prefab, SpriteSet};
use c3_game::{Drop, Fighter, ItemType, MonsterType, Player, Rng, Rules, Stats};
use c3_project::Project;
use c3_render::SceneAssets;
use c3_scene::{Grid, IsoProjection, Npc, Scene, TileLayer, Tileset};
use std::collections::BTreeMap;
use std::path::Path;
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
    rules: Arc<BasicRules>,
}

impl Default for BasicGame {
    fn default() -> Self {
        Self {
            assets: BasicAssets,
            rules: Arc::new(BasicRules),
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
}

impl Looks for BasicGame {
    fn player(&self, _p: &Player) -> Look {
        self.default_player()
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

    fn default_player(&self) -> Look {
        Look::default()
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

/// Simple rules until the project defines its own: players grow with their level, hits deal
/// attack minus defence, and there are no monster or item types.
#[derive(Debug, Default)]
struct BasicRules;

impl Rules for BasicRules {
    fn monster(&self, _kind: u32) -> Option<&MonsterType> {
        None
    }

    fn item(&self, _kind: u32) -> Option<&ItemType> {
        None
    }

    fn player_stats(&self, level: u32) -> Stats {
        Stats {
            max_life: 100 + level * 20,
            max_mana: 50 + level * 10,
            attack_min: 8 + level * 2,
            attack_max: 12 + level * 3,
            defence: level,
            dodge: 5,
            dexterity: 5,
            attack_range: 1,
            attack_ms: 1000,
            step_ms: 500,
        }
    }

    fn level_exp(&self, level: u32) -> Option<u64> {
        (level < 100).then(|| u64::from(level) * 100)
    }

    fn hit(&self, attacker: &Fighter, target: &Fighter, rng: &mut Rng) -> u32 {
        let a = &attacker.stats;
        let attack = rng.range(a.attack_min.into(), a.attack_max.into()) as u32;
        attack.saturating_sub(target.stats.defence).max(1)
    }

    fn hit_exp(&self, _level: u32, _target: &Fighter, damage: u32) -> u64 {
        damage.into()
    }

    fn kill_exp(&self, _level: u32, target: &Fighter) -> u64 {
        u64::from(target.level) * 10
    }

    fn drops(&self, _kind: u32, _rng: &mut Rng) -> Vec<Drop> {
        Vec::new()
    }

    fn shop(&self, _npc: u32) -> &[u32] {
        &[]
    }
}

impl GameRules for BasicRules {
    fn rules(&self, _tables: Option<&Path>) -> (Box<dyn Rules>, Vec<String>) {
        (Box::new(BasicRules), Vec::new())
    }
}

/// A blank map `width` × `height` cells: all walkable, on the engine's default ground.
pub fn blank_scene(width: u32, height: u32, cell: [f32; 2]) -> Scene {
    let [cell_w, cell_h] = cell;
    let grid = Grid::new(width.max(1), height.max(1));
    let projection = IsoProjection::for_grid(cell_w, cell_h, grid.width);
    // The ground covers the map's diamond: from its left to its right corner, top to bottom.
    let (w, h) = (grid.width as f32, grid.height as f32);
    let left = projection.origin.0 - cell_w * h / 2.0;
    let right = projection.origin.0 + cell_w * w / 2.0;
    let bottom = cell_h * (w + h) / 2.0 + projection.origin.1;
    let origin = (left.floor() as i32, 0);
    let tiles_x = ((right - left) / GROUND_TILE as f32).ceil() as u32 + 1;
    let tiles_y = (bottom / GROUND_TILE as f32).ceil() as u32 + 1;
    let ground = TileLayer {
        source: GROUND_IMAGE.to_owned(),
        tile_size: GROUND_TILE,
        width: tiles_x,
        height: tiles_y,
        tiles: vec![Some(0); (tiles_x * tiles_y) as usize],
        tileset: Tileset {
            source: GROUND_IMAGE.to_owned(),
            frames: BTreeMap::from([(0, GROUND_IMAGE.to_owned())]),
        },
        origin,
        scroll: (0, 0),
    };
    Scene {
        grid,
        projection,
        ground: Some(ground),
        portals: Vec::new(),
        entities: Vec::new(),
        layers: Vec::new(),
        npcs: Vec::new(),
        spawns: Vec::new(),
        meta: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_map_is_walkable_and_has_ground() {
        let scene = blank_scene(40, 30, [64.0, 32.0]);
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
