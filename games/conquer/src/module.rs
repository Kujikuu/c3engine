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
        attribute(
            "lock_key",
            "The number that unlocks the player's items (0: none)",
            0,
            None,
        ),
        attribute(
            "blessing_until",
            "When heaven's blessing ends (seconds of the clock)",
            0,
            None,
        ),
        attribute("cursed", "Cursed: no battle experience", 0, Some(1)),
        attribute(
            "exp_percent",
            "Battle experience percent while it lasts",
            100,
            None,
        ),
        attribute(
            "exp_until",
            "When the experience percent ends (seconds of the clock)",
            0,
            None,
        ),
        attribute(
            "lucky_time",
            "Lucky time left, in seconds: double hits, dodged blows",
            0,
            Some(7200),
        ),
    ];
    // TQ's item columns beside type, amount and durability (`cq_item`).
    s.gameplay.item_attributes = [
        "plus", "gem1", "gem2", "bless", "enchant", "color", "ident", "data", "max_dur",
    ]
    .map(String::from)
    .to_vec();
    // Gems: socket value n is item type 700000 + n (items §6).
    s.gameplay.sockets = ["gem1", "gem2"].map(String::from).to_vec();
    s.gameplay.socket_item_base = 700_000;
    // TQ's rank of a plain guild member.
    s.gameplay.guild_member_rank = 50;
    // Attribute points spent on the four attributes (MsgAllot).
    s.gameplay.points_attribute = "attribute_points".into();
    // Helmets, armour and shields show their colour's icon.
    s.gameplay.item_look_attribute = "color".into();
    s.gameplay.point_attributes = ["strength", "agility", "vitality", "spirit"]
        .map(String::from)
        .to_vec();
    // Deputies (90) invite too; a guild needs 100,000 in its fund to take members, and has
    // 800 at most (syndicate §2).
    s.gameplay.guild_invite_rank = 90;
    s.gameplay.guild_join_min_fund = 100_000;
    // A guild under 100,000 can't talk on its channel either (packets 1004, 2004).
    s.gameplay.guild_chat_min_fund = 100_000;
    // Face, forge and gem NPCs open their windows (MyShellDlg.cpp 2100-2108, Role.h).
    s.gameplay.npc_windows = vec![[5, 212], [6, 211], [7, 229]];
    // Booth flags (_ROLE_BOOTH_FLAG_NPC 16, Role.h) are where players open booths.
    s.gameplay.booth_spots = vec![16];
    // Mentors (adapters spec §5, cq_tutor_type and cq_student_type): level 70, a gap of 30 /
    // 20 / 10 levels by the mentor's rebirths, students by its level, a share of a student's
    // time below 130 (to the mentor's mentor from 110), 50,000 silver and 72 hours to leave,
    // blessing (status 9876) and +1 stones (730001).
    s.gameplay.mentor = c3_core::settings::MentorSettings {
        min_level: 70,
        gaps: vec![30, 20, 10],
        students: vec![
            [70, 109, 2],
            [110, 119, 3],
            [120, 129, 5],
            [130, 133, 7],
            [134, 140, 10],
        ],
        shares: vec![
            [0, 1, 69, 2],
            [0, 70, 99, 3],
            [0, 100, 109, 5],
            [0, 110, 119, 7],
            [0, 120, 140, 10],
            [1, 110, 119, 1],
            [1, 120, 140, 2],
        ],
        max_student_level: 130,
        leave_cost: 50_000,
        leave_hours: 72,
        bless_status: 9876,
        item: 730_001,
    };
    // Nobility (adapters spec §2): from level 70, at least 3,000,000 silver (or a CP for each
    // 50,000), a board of 60: the first 3 Kings, to 15 Princes, to 50 Dukes, the rest Earls
    // from 200,000,000, Barons from 100,000,000, Knights from 30,000,000 (En_Res notices).
    s.gameplay.nobility = c3_core::settings::NobilitySettings {
        min_level: 70,
        min_donation: 3_000_000,
        max_donation: 1_000_000_000,
        currency: CONQUER_POINTS.into(),
        rate: 50_000,
        board_size: 60,
        by_place: vec![[3, 12], [15, 9], [50, 7]],
        by_value: vec![[200_000_000, 5], [100_000_000, 3], [30_000_000, 1]],
        notices: vec![
            (
                12,
                "Hear ye, hear ye! All hail his majesty, %s! King of Conquer!".into(),
            ),
            (
                9,
                "Hear ye, hear ye! A new Prince has arrived! Hail, %s! Prince!".into(),
            ),
            (7, "Hear ye, hear ye! Rejoice for the new Duke, %s!".into()),
            (5, "Hear ye, hear ye! Welcome to our new Earl, %s!".into()),
            (
                3,
                "Hear ye, hear ye! Greetings to our new Baron, %s!".into(),
            ),
            (
                1,
                "Hear ye, hear ye! Knight %s has just been appointed!".into(),
            ),
        ],
        // StrRes 11100-11107.
        titles: [
            (12, "King"),
            (9, "Prince"),
            (7, "Duke"),
            (5, "Earl"),
            (3, "Baron"),
            (1, "Knight"),
            (0, "Commoner"),
        ]
        .map(|(rank, title)| (rank, title.to_owned()))
        .to_vec(),
    };
    // Broadcasts (adapters spec §4): 5 CPs a line from level 50, 80 letters, 5 waiting each,
    // a minute apart; 15 CPs to the front, 5 CPs up 5 places.
    s.gameplay.broadcast = c3_core::settings::BroadcastSettings {
        cost: 5,
        currency: CONQUER_POINTS.into(),
        min_level: 50,
        max_len: 80,
        max_queued: 5,
        gap_secs: 60,
        first_cost: 15,
        bump_cost: 5,
        bump_places: 5,
    };
    // A locked item takes 5 days to unlock (StrRes 11194).
    s.gameplay.item_lock_days = 5;
    // Sashes hold 3, 6 and 9 items (the chest window's rows: type % 10 / 3).
    s.gameplay.chests = vec![[1_100_003, 3], [1_100_006, 6], [1_100_009, 9]];
    // A broken item takes 5 Meteors to repair (MyShellDlg.cpp OpenBrokenRepairPrompt5065).
    s.gameplay.broken_repair = Some([1_088_001, 5]);
    // Faces (Common.ini Playerface: 102 male from 1, 90 female from 201; bodies 3-4 male,
    // 1-2 female), 500 silver a change (packets spec MsgAction 142).
    s.gameplay.faces = vec![[3, 4, 1, 102], [1, 2, 201, 290]];
    s.gameplay.face_cost = 500;
    // Trade partners are on trial for 3 days (social spec §3, PROBATION 259200).
    s.gameplay.partner_probation_secs = 259_200;
    // The message boards (CDlgLevWord's tabs, channels 2201-2205); the guild's own is the
    // fourth; posting from level 16 (packets spec B10).
    s.gameplay.boards = ["Trade", "Friend", "Team", "Guild", "Other"]
        .map(String::from)
        .to_vec();
    s.gameplay.guild_board = Some(3);
    s.gameplay.board_min_level = 16;
    // Players enter in arrest mode, as the client asks at login (DlgMain.cpp 2176).
    s.gameplay.pk.start_mode = c3_core::settings::PkMode::Arrest;
    s.gameplay.guild_max_members = 800;
    // Teammates within 32 cells of a kill share it (battle §4); a spouse is `mate`.
    s.gameplay.team_exp_range = 32;
    // A killer's loot is its own for 30 s (monsters §6.9); teams never share DragonBalls or
    // Meteors; a teammate under 70 picking up the money of one of 70 or more, 20 levels above
    // it, gives it a fifth (social §6.5).
    s.gameplay.loot_protection_seconds = 30.0;
    s.gameplay.team_never_shared = vec![1_088_000, 1_088_001];
    s.gameplay.team_money_cut = c3_core::settings::TeamMoneyCut {
        percent: 20,
        picker_below: 70,
        owner_from: 70,
        gap: 20,
    };
    // Dynamic NPCs with life (training goals, the guild-war pole and gates) can be hit; a
    // goal's level is its `data3`; at 0 life the `linkid` chain runs (sort bit 0x10).
    s.gameplay.npc_combat = c3_core::settings::NpcCombatSettings {
        max_life: "maxlife".into(),
        life: "life".into(),
        level: "data3".into(),
        defence: "defence".into(),
        destroyed_event: "linkid".into(),
    };
    s.gameplay.spouse_text = crate::npc_logic::MATE.to_owned();
    s.gameplay.new_character = game::new_character();
    s.network.fronts.push(c3_core::settings::FrontSettings {
        kind: FRONT_KIND.into(),
        public_ip: "127.0.0.1".into(),
    });
    s.ui.layout = "ini/GUI.ini".into();
    s.ui.skin = "ani/Control.Ani".into();
    s.ui.skins = vec![
        "ani/Server.ani".into(),
        "ani/NpcFace.Ani".into(),
        "ani/Magic.Ani".into(),
        // Players' faces (PlayerFace.Ani names them Face<n> too, as NpcFace.Ani does).
        "ani/PlayerFace.Ani#Player".into(),
        // The chat's emotions (ICON<n>, as EmotionICON<n>).
        "ani/EmotionIco.ani#Emotion".into(),
    ];
    // The client's dialog sounds (DXPlaySound in its dialogs).
    s.ui.sound_open = "sound/DlgOpen.wav".into();
    s.ui.sound_close = "sound/DlgClose.wav".into();
    s.ui.sound_click = "sound/BtnClick.wav".into();
    s.ui.sound_sell = "sound/Sell.wav".into();
    s.ui.sound_equip = "sound/equip.wav".into();
    // The radar: MiniMap.ani's picture in a 170x128 box top right (CGameMap::Create).
    s.ui.radar_box = Some([170, 128]);
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

    fn map_key(&self, id: u32) -> Option<String> {
        Some(self.content.map_path(id)?.replace('\\', "/"))
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

    fn login_list(
        &self,
        address: &str,
        read: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) -> Option<c3_play::module::LoginList> {
        crate::Content::login_list(address, read)
    }

    fn chat_look(&self) -> Option<c3_ui::ChatLook> {
        self.content.chat_look()
    }

    fn destinations(&self) -> Vec<c3_ui::Destination> {
        self.content.destinations()
    }

    fn battle_power(&self, sim: &c3_game::Sim, p: &c3_game::Player) -> Option<c3_ui::Power> {
        Some(crate::battle_power::battle_power(sim, p))
    }

    fn item_glow(&self, kind: u32) -> Vec<Image> {
        self.content.item_glow(kind)
    }

    fn item_icon(&self, kind: u32, look: u32) -> Option<Image> {
        self.content.item_icon(kind, look)
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

    fn radar_mark(&self, kind: &str) -> Option<Image> {
        self.content.radar_mark(kind)
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

    /// Sitting, kneeling and fainting hold their last frame; the dance loops (TQ's emotes).
    fn emote_play(&self, emote: u32) -> c3_play::module::EmotePlay {
        use c3_play::module::EmotePlay;
        match emote {
            1 => EmotePlay::Loop,
            170 | 250 | 270 => EmotePlay::Hold,
            _ => EmotePlay::Once,
        }
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
