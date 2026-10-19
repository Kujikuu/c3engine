//! Conquer as a game the engine plays ([`GameModule`]): a 5065 client's content, Conquer's
//! character assembly and actions, its rules, item icons, sounds and music.

use crate::Content;
use crate::character::{DEFAULT_HERO, RoleLook, action, player_look, weapon_family};
use crate::game;
use crate::protocol::FRONT_KIND;
use crate::rules::SLOT_NAMES;
use c3_assets::Image;
use c3_audio::MusicRegion;
use c3_core::settings::AttributeDef;
use c3_game::{EntityId, Player, Rules, Sim, SkillType};
use c3_play::{
    Act, Character, CharacterMotions, GameModule, GameRules, Look, LookKind, Looks, PartModel,
};
use c3_project::{Project, ProjectSettings};
use c3_render::SceneAssets;
use c3_scene::{Npc, Scene};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

/// Twin City, where Conquer starts.
pub const DEFAULT_MAP_ID: u32 = 1002;
/// Effect marking where the player clicked to walk.
const CLICK_EFFECT: &str = "mouse";
/// The effect the client draws on every portal.
const PORTAL_EFFECT: &str = "Exit";
/// Effect played on a player rising a level.
const LEVEL_UP: &str = "LevelUp";
/// Action of a dying role (`_ACTION_DIE`).
const DIE: u32 = 330;
/// Frame length of NPCs, which keep their stand-by motion (`GetActionTimeInterval`'s default).
const NPC_FRAME_MS: u32 = 33;
/// Weapon family meaning "any weapon" in `ActionSound.ini`.
const ANY_WEAPON: u32 = 999;

/// The currency TQ calls e-money: Conquer Points.
pub const CONQUER_POINTS: &str = "CPs";

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

/// Conquer's rules: the engine's data rules over the project's tables and formulas (the
/// import writes them from TQ's server data: [`crate::rules`], [`crate::rule_blueprint`]).
pub struct ConquerGameRules;

impl GameRules for ConquerGameRules {
    fn rules(&self, tables: Option<&Path>) -> (Box<dyn Rules>, Vec<String>) {
        c3_play::data_rules::DataGameRules.rules(tables)
    }
}

/// The client's cursor files, by what the mouse is over.
const CURSORS: [(&str, &str); 4] = [
    ("normal", "data/Cursor/Normal.ani"),
    ("attack", "data/Cursor/pk.ani"),
    ("talk", "data/Cursor/Npc.ani"),
    ("pick_up", "data/Cursor/pickup.ani"),
];

/// Conquer's settings: its cursors, money and newbie, served to the original client too.
/// The client's dialogs as its code builds them (see the file), served at [`UI_CONTROLS`]
/// unless the project has its own.
const CONTROLS: &[u8] = include_bytes!("../ui/controls.ini");
const UI_CONTROLS: &str = "ui/controls.ini";

pub fn conquer_settings() -> ProjectSettings {
    let mut s = ProjectSettings::default();
    s.player.game_name = "Conquer Online".into();
    s.player.icon = "conquer.ico".into();
    for (kind, file) in CURSORS {
        let cursor = match kind {
            "normal" => &mut s.cursors.normal,
            "attack" => &mut s.cursors.attack,
            "talk" => &mut s.cursors.talk,
            _ => &mut s.cursors.pick_up,
        };
        cursor.image = file.into();
    }
    s.gameplay.currency = "Silver".into();
    s.gameplay.currencies = vec![CONQUER_POINTS.into()];
    // The dead get up as ghosts and ask to come back, after 20 seconds (battle spec S9).
    s.gameplay.revive_seconds = 20.0;
    s.gameplay.revive_by_request = true;
    s.gameplay.ghosts_walk = true;
    let attribute = |name: &str, doc: &str, default: i64, max: Option<i64>| AttributeDef {
        name: name.into(),
        doc: doc.into(),
        default,
        min: Some(0),
        max,
    };
    s.gameplay.attributes = vec![
        attribute("strength", "Raises attack", 0, None),
        attribute("agility", "Raises accuracy and dodging", 0, None),
        attribute("vitality", "Raises life", 0, None),
        attribute("spirit", "Raises mana", 0, None),
        attribute(
            "attribute_points",
            "Points to spend on the four above",
            0,
            None,
        ),
        attribute("rebirths", "Times reborn (metempsychosis)", 0, Some(2)),
        attribute("virtue", "Earned by mentoring new players", 0, None),
        attribute("stamina", "Spent by XP skills and jumping", 100, Some(100)),
        attribute("xp", "Fills toward an XP skill", 0, Some(100)),
    ];
    // TQ's item columns beside type, amount and durability (`cq_item`).
    s.gameplay.item_attributes = [
        "plus", "gem1", "gem2", "bless", "enchant", "color", "ident", "data",
    ]
    .map(String::from)
    .to_vec();
    // Gems: socket value n is item type 700000 + n (items §6).
    s.gameplay.sockets = ["gem1", "gem2"].map(String::from).to_vec();
    s.gameplay.socket_item_base = 700_000;
    s.gameplay.new_character = game::new_character();
    s.network.fronts.push(c3_core::settings::FrontSettings {
        kind: FRONT_KIND.into(),
        public_ip: "127.0.0.1".into(),
    });
    s.ui.layout = "ini/GUI.ini".into();
    s.ui.skin = "ani/Control.Ani".into();
    s.ui.skins = vec!["ani/Server.ani".into()];
    // The login's server groups and pictures, from the client's Server.ani.
    s.network.groups = vec![c3_core::settings::ServerGroup {
        name: "Conquer".into(),
        icon: "GroupPic1".into(),
    }];
    s.network.local_icon = "servericon01".into();
    s.ui.controls = UI_CONTROLS.into();
    // One character per account; the four bodies of character creation (lookface % 1000).
    s.characters.per_account = 1;
    s.characters.looks = [
        ("Small male", 3, 310),
        ("Large male", 4, 310),
        ("Small female", 1, 310),
        ("Large female", 2, 310),
    ]
    .map(|(name, body, hair)| c3_core::settings::CreationLook {
        name: name.into(),
        body,
        hair,
    })
    .to_vec();
    let classes = [
        ("Trojan", 10, 10025),
        ("Warrior", 20, 10022),
        ("Archer", 40, 10028),
        ("Taoist", 100, 10031),
    ];
    s.characters.classes = (0..8)
        .map(|i| {
            let (name, profession, text) = classes[i % 4];
            c3_core::settings::CreationClass {
                name: name.into(),
                description: format!("@ini/StrRes.ini:{text}"),
                profession,
                looks: if i < 4 { vec![0, 1] } else { vec![2, 3] },
                showcase: Some(i as u32),
            }
        })
        .collect();
    s
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

    fn default_settings(&self) -> ProjectSettings {
        conquer_settings()
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

    fn server_data_kind(&self) -> Option<&str> {
        Some("TQ SQL dump")
    }

    fn import_server_data(&self, dump: &[u8]) -> Option<c3_play::server_data::ServerData> {
        // TQ's dumps keep their text (names, dialogue) in GBK.
        let (sql, _, _) = encoding_rs::GBK.decode(dump);
        Some(crate::import::import_server_data(&self.content, &sql))
    }

    fn save_map(
        &self,
        project: &Project,
        key: &str,
        scene: &Scene,
        ground: Option<&c3_scene::TileLayer>,
    ) -> Result<(), String> {
        game::save_map(project, key, scene, ground)
    }

    fn map_ids(&self, key: &str) -> Vec<u32> {
        self.content.map_ids_for_path(key)
    }

    fn tilt_deg(&self) -> f32 {
        self.content.oblique_angle()
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        if path == UI_CONTROLS {
            return Some(CONTROLS.to_vec());
        }
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

    /// The client draws `Exit` on every passage cell as it loads a map
    /// (`CTerrainLayer::LoadDataPassage`).
    fn portal_effect(&self) -> Option<String> {
        Some(PORTAL_EFFECT.to_owned())
    }

    /// TQ's tables; magic rows are keyed by row, their skill by `type`.
    fn data_tables(&self) -> c3_play::DataTables {
        use c3_play::TableRef;
        c3_play::DataTables {
            monsters: TableRef::keyed(crate::rules::MONSTER_TABLE),
            items: TableRef::keyed(crate::server_data::ITEM_TABLE),
            skills: TableRef {
                table: "cq_magictype".into(),
                id_column: Some("type".into()),
            },
            levels: TableRef::keyed("cq_levexp"),
        }
    }

    /// The client's NPC types (`Npc.ini`).
    fn npc_looks(&self) -> Vec<(u32, String)> {
        self.content.npc_looks()
    }

    /// The client's own minimap (`ani/MiniMap.Ani`, by map id): one image, or four
    /// quarters laid out left to right, top to bottom.
    fn minimap(&self, _key: &str, map_ids: &[u32]) -> Option<Image> {
        self.content.minimap(map_ids)
    }

    fn click_effect(&self) -> Option<String> {
        Some(CLICK_EFFECT.to_owned())
    }

    fn level_up_effect(&self) -> Option<String> {
        Some(LEVEL_UP.to_owned())
    }

    fn skill_media(&self, skill: &SkillType) -> (Option<String>, Option<String>) {
        let (effect, sound) = self.content.magic_media(skill.id);
        (effect.map(str::to_owned), sound.map(str::to_owned))
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
            LookKind::Showcase => self.content.showcase_character(look.body),
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
            LookKind::Npc | LookKind::Model | LookKind::Showcase => CharacterMotions::default(),
            _ => self
                .content
                .character_motions(character, &RoleLook::from_look(look), act),
        }
    }

    fn frame_ms(&self, look: &Look, act: u32) -> u32 {
        match look.kind {
            LookKind::Npc | LookKind::Model | LookKind::Showcase => NPC_FRAME_MS,
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
