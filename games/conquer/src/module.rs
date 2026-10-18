//! Conquer as a game the engine plays ([`GameModule`]): a 5065 client's content, Conquer's
//! character assembly and actions, its rules, item icons, sounds and music.

use crate::Content;
use crate::character::{DEFAULT_HERO, RoleLook, action, player_look, weapon_family};
use crate::game;
use crate::rules::{ConquerRules, SLOT_NAMES};
use c3_assets::Image;
use c3_audio::MusicRegion;
use c3_game::{EntityId, Player, PlayerSave, Rules, Sim};
use c3_play::{
    Act, Character, CharacterMotions, GameModule, GameRules, Look, LookKind, Looks, PartModel,
};
use c3_project::Project;
use c3_render::SceneAssets;
use c3_scene::{CellPos, Npc, Scene};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

/// Twin City, where Conquer starts.
pub const DEFAULT_MAP_ID: u32 = 1002;
/// Effect marking where the player clicked to walk.
const CLICK_EFFECT: &str = "mouse";
/// Effect played on a player rising a level.
const LEVEL_UP: &str = "LevelUp";
/// Action of a dying role (`_ACTION_DIE`).
const DIE: u32 = 330;
/// Frame length of NPCs, which keep their stand-by motion (`GetActionTimeInterval`'s default).
const NPC_FRAME_MS: u32 = 33;
/// Weapon family meaning "any weapon" in `ActionSound.ini`.
const ANY_WEAPON: u32 = 999;

/// Conquer Online 5065, from a client's content.
pub struct ConquerModule {
    content: Rc<Content>,
    rules: Arc<ConquerGameRules>,
}

impl ConquerModule {
    pub fn new(content: Rc<Content>) -> Self {
        Self {
            content,
            rules: Arc::new(ConquerGameRules),
        }
    }

    /// Opens the client content at `root`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, String> {
        Ok(Self::new(Rc::new(Content::open(root)?)))
    }

    pub fn content(&self) -> &Content {
        &self.content
    }
}

/// Conquer's rules from a project's tables (built-in ones without), and a newbie's start.
pub struct ConquerGameRules;

impl GameRules for ConquerGameRules {
    fn rules(&self, tables: Option<&Path>) -> (Box<dyn Rules>, Vec<String>) {
        match tables {
            Some(dir) => {
                let (rules, problems) = ConquerRules::from_tables(dir);
                (Box::new(rules), problems)
            }
            None => (Box::new(ConquerRules::default()), Vec::new()),
        }
    }

    fn new_character(&self, name: &str, cell: CellPos) -> PlayerSave {
        game::new_character(name, cell)
    }
}

impl GameModule for ConquerModule {
    fn name(&self) -> &str {
        "Conquer Online 5065"
    }

    fn assets(&self) -> &dyn SceneAssets {
        &*self.content
    }

    fn looks(&self) -> &dyn Looks {
        self
    }

    fn rules(&self) -> Arc<dyn GameRules> {
        self.rules.clone()
    }

    fn maps(&self, project: &Project) -> Vec<String> {
        let mut maps = project.map_names();
        maps.extend(self.content.list_maps());
        maps
    }

    fn default_map(&self, _project: &Project) -> Option<String> {
        self.content.map_path(DEFAULT_MAP_ID)
    }

    fn load_map(&self, project: &Project, key: &str) -> Result<Scene, String> {
        game::load_map(project, &self.content, key)
    }

    fn map_ids(&self, key: &str) -> Vec<u32> {
        self.content.map_ids_for_path(key)
    }

    fn tilt_deg(&self) -> f32 {
        self.content.oblique_angle()
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        self.content.files().read(path).map(|b| b.into_owned())
    }

    fn item_icon(&self, kind: u32) -> Option<Image> {
        // Icons are drawn for one quality digit (5 for weapons, 0 for most).
        [kind, kind / 10 * 10 + 5, kind / 10 * 10]
            .iter()
            .find_map(|k| {
                self.content
                    .load_image(&format!("data/ItemMinIcon/{k}.dds"))
            })
    }

    fn slot_names(&self) -> Vec<(u8, String)> {
        SLOT_NAMES
            .iter()
            .map(|(s, n)| (*s, (*n).to_owned()))
            .collect()
    }

    fn click_effect(&self) -> Option<String> {
        Some(CLICK_EFFECT.to_owned())
    }

    fn level_up_effect(&self) -> Option<String> {
        Some(LEVEL_UP.to_owned())
    }

    fn item_use(&self, kind: u32) -> (Option<String>, Option<String>) {
        let (effect, sound) = self.content.item_use_media(kind);
        (effect.map(str::to_owned), sound.map(str::to_owned))
    }

    fn action_sound(&self, sim: &Sim, who: EntityId, act: Act) -> Option<String> {
        let (look, weapon) = if let Some(p) = sim.player(who) {
            let look = player_look(p);
            (look.look, weapon_family(look.l_weapon, look.r_weapon))
        } else {
            let m = sim.monster(who)?;
            (sim.rules().monster(m.kind)?.look % 1000, ANY_WEAPON)
        };
        let act = match act {
            Act::Run | Act::RunAlt => action::RUN_L,
            other => self.action(other),
        };
        self.content
            .action_sound(look, weapon, act)
            .map(str::to_owned)
    }

    fn music_regions(&self) -> Vec<MusicRegion> {
        self.content.music_regions().to_vec()
    }
}

impl Looks for ConquerModule {
    fn player(&self, p: &Player) -> Look {
        player_look(p).to_look(LookKind::Player)
    }

    fn monster(&self, kind: u32, rules: &dyn Rules) -> Look {
        let body = rules.monster(kind).map_or(0, |t| t.look % 1000);
        Look {
            kind: LookKind::Monster,
            body,
            zoom_percent: self.content.monster_zoom(kind),
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
        DEFAULT_HERO.to_look(LookKind::Player)
    }

    fn character(&self, look: &Look, act: u32) -> Result<Character, String> {
        match look.kind {
            LookKind::Npc => self.content.npc_character(look.body),
            LookKind::Model => {
                let (model, texture) = self.content.load_model(look.body.into())?;
                Ok(Character::single(PartModel::plain(model, texture)))
            }
            _ => self
                .content
                .assemble_character(&RoleLook::from_look(look), act),
        }
    }

    fn motions(&self, character: &Character, look: &Look, act: u32) -> CharacterMotions {
        match look.kind {
            // NPCs and plain models keep their own motion.
            LookKind::Npc | LookKind::Model => CharacterMotions::default(),
            _ => self
                .content
                .character_motions(character, &RoleLook::from_look(look), act),
        }
    }

    fn frame_ms(&self, look: &Look, act: u32) -> u32 {
        match look.kind {
            LookKind::Npc | LookKind::Model => NPC_FRAME_MS,
            _ => self.content.frame_interval(&RoleLook::from_look(look), act),
        }
    }

    fn part_effects(&self, key: u64, act: u32) -> Vec<String> {
        self.content.part_action_effects(key, act)
    }

    fn action(&self, act: Act) -> u32 {
        match act {
            Act::Stand => action::STANDBY,
            Act::Walk => action::WALK_L,
            Act::Run => action::RUN_L,
            Act::RunAlt => action::RUN_R,
            Act::Attack => action::ATTACK_0,
            Act::Die => DIE,
            Act::Jump => action::JUMP,
        }
    }

    fn action_names(&self) -> Vec<(u32, String)> {
        action::NAMED
            .iter()
            .map(|(id, name)| (*id, (*name).to_owned()))
            .collect()
    }
}
