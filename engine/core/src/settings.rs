//! Project Settings: every value a game is tuned by, kept in the project (`settings.toml`)
//! and read by the Studio's Play mode, the client and the server alike. Sections follow
//! Unity's Project Settings window. A file may leave out anything; missing values take their
//! defaults, so a project only stores what it changed.

use crate::input::InputSettings;
use serde::{Deserialize, Serialize};

/// All of a project's settings.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectSettings {
    pub player: PlayerSettings,
    pub cursors: CursorSettings,
    pub camera: CameraSettings,
    pub input: InputSettings,
    pub gameplay: GameplaySettings,
    /// The game's maps: their ids (what portals and scripts name them by), names, music. Maps
    /// of the project that aren't listed still play, with ids the engine gives them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub maps: Vec<MapEntry>,
    pub graphics: GraphicsSettings,
    pub audio: AudioSettings,
    pub network: NetworkSettings,
    pub localization: LocalizationSettings,
    pub ui: UiSettings,
    pub characters: CharacterSettings,
}

impl ProjectSettings {
    /// Why these settings can't be used, if they can't (each problem in words).
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut names = std::collections::BTreeSet::new();
        for a in &self.gameplay.attributes {
            if a.name.trim().is_empty() {
                problems.push("A player attribute has no name".to_owned());
            } else if !names.insert(a.name.as_str()) {
                problems.push(format!("Two player attributes are called {}", a.name));
            } else if a.min.zip(a.max).is_some_and(|(min, max)| min > max) {
                problems.push(format!(
                    "Player attribute {}: its lowest is above its highest",
                    a.name
                ));
            }
        }
        let mut check = |ok: bool, problem: &str| {
            if !ok {
                problems.push(problem.to_owned());
            }
        };
        let (p, c, g, a, n) = (
            &self.player,
            &self.camera,
            &self.gameplay,
            &self.audio,
            &self.network,
        );
        check(
            (320..=7680).contains(&p.window_width) && (240..=4320).contains(&p.window_height),
            "The window must be 320×240 to 7680×4320",
        );
        check(
            c.min_zoom > 0.0 && c.min_zoom <= c.zoom && c.zoom <= c.max_zoom && c.max_zoom <= 16.0,
            "Zoom limits must hold the start zoom: 0 < min ≤ zoom ≤ max ≤ 16",
        );
        check(c.wheel_speed >= 0.0, "Wheel speed can't be negative");
        check(c.pan_speed >= 0.0, "Pan speed can't be negative");
        check(
            c.follow_smoothing >= 0.0,
            "Follow smoothing can't be negative",
        );
        check(c.dead_zone >= 0.0, "The dead zone can't be negative");
        check(c.shake >= 0.0, "Shake can't be negative");
        check(
            c.new_map_cell[0] >= 2.0 && c.new_map_cell[1] >= 1.0,
            "Cells must be at least 2×1 pixels",
        );
        check(
            (10..=1000).contains(&g.tick_ms),
            "A tick must last 10 to 1000 ms",
        );
        check(
            (1..=100).contains(&g.view_range),
            "The view range must be 1 to 100 cells",
        );
        check(
            [
                g.talk_range,
                g.trade_range,
                g.jump_range,
                g.attack_search,
                g.monster_leash,
                g.monster_wander,
            ]
            .iter()
            .all(|r| (0..=1000).contains(r)),
            "Ranges must be 0 to 1000 cells",
        );
        check(
            [g.corpse_seconds, g.revive_seconds, g.item_life_seconds]
                .iter()
                .all(|s| (0.0..=86_400.0).contains(s)),
            "Times must be 0 to 86 400 seconds",
        );
        check(
            (1..=500).contains(&g.bag_size),
            "The bag must hold 1 to 500 items",
        );
        check(
            g.new_character.level >= 1,
            "New characters start at level 1 or more",
        );
        check(
            [a.master_volume, a.music_volume, a.sound_volume]
                .iter()
                .all(|v| (0.0..=1.0).contains(v)),
            "Volumes must be 0 to 1",
        );
        check(a.hearing_range > 0.0, "The hearing range must be above 0");
        check(n.port != 0, "The port can't be 0");
        check(
            n.save_interval_seconds >= 1.0 && n.join_timeout_seconds >= 1.0,
            "The save interval and join timeout must be 1 second or more",
        );
        problems.extend(self.input.problems());
        problems.extend(map_problems(&self.maps));
        problems
    }
}

/// A map of the game, as the map list names it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapEntry {
    /// The map: a project scene's name (`Meadow`), or a map file of the game's content.
    pub key: String,
    /// What portals, scripts and saves name the map by; 1 or more.
    pub id: u32,
    /// The name players see (the key's file name when empty).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Music played on the map (a sound asset); none when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub music: String,
    /// Players may attack each other here.
    #[serde(default, skip_serializing_if = "is_false")]
    pub pk: bool,
    /// Fights between players here bring no crime and no PK points (arenas).
    #[serde(default, skip_serializing_if = "is_false")]
    pub pk_free: bool,
    /// The light over this map (night, a cave); none: the project's (Graphics > Light).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<[u8; 3]>,
    /// Where players who die here come back: a map's id and cell; none: where they fell
    /// (or their own return point).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revive: Option<RevivePoint>,
}

/// A map's id and a cell of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevivePoint {
    pub map: u32,
    pub cell: [i32; 2],
}

fn is_false(b: &bool) -> bool {
    !b
}

fn map_problems(maps: &[MapEntry]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, m) in maps.iter().enumerate() {
        if m.key.trim().is_empty() {
            out.push(format!("Map {} of the list names no map", i + 1));
        }
        if m.id == 0 {
            out.push(format!("Map {} needs an id of 1 or more", m.key));
        }
        if maps[..i].iter().any(|o| o.id == m.id) {
            out.push(format!("Two maps have id {}", m.id));
        }
        if maps[..i].iter().any(|o| o.key == m.key) {
            out.push(format!("Map {} is listed twice", m.key));
        }
    }
    out
}

/// The game as a program: its name, icon and window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerSettings {
    /// The game's name, shown as the window title; empty: the project's name.
    pub game_name: String,
    /// The window icon: an image in the project; empty: none.
    pub icon: String,
    pub window_width: u32,
    pub window_height: u32,
    pub fullscreen: bool,
    pub resizable: bool,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            game_name: String::new(),
            icon: String::new(),
            window_width: 1280,
            window_height: 800,
            fullscreen: false,
            resizable: true,
        }
    }
}

/// A mouse cursor: an image of the project (PNG, or a Windows `.cur`/`.ani`) and the pixel
/// that points.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Cursor {
    /// Empty: the system's arrow.
    pub image: String,
    /// The pointing pixel; `None`: the one the file names (cursor files), else the top left.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hotspot: Option<[u32; 2]>,
}

/// The cursors the game shows for what is under the mouse.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CursorSettings {
    pub normal: Cursor,
    /// Over a monster.
    pub attack: Cursor,
    /// Over an NPC.
    pub talk: Cursor,
    /// Over something on the ground.
    pub pick_up: Cursor,
    /// Over a portal.
    pub portal: Cursor,
}

/// How the game's camera sees the map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraSettings {
    /// Zoom when the game starts (1 = the art's own size).
    pub zoom: f64,
    pub min_zoom: f64,
    pub max_zoom: f64,
    /// How fast the mouse wheel zooms (1 = normal, 0 = the wheel doesn't zoom).
    pub wheel_speed: f64,
    /// Keyboard panning, in world pixels per second at zoom 1.
    pub pan_speed: f64,
    /// Players may move the camera away from the hero (drag, keys).
    pub free_pan: bool,
    /// Seconds the camera takes to catch up with the hero (0: it stays locked on).
    pub follow_smoothing: f64,
    /// How far (screen pixels) the hero may move from the centre before the camera follows.
    pub dead_zone: f64,
    /// Keeps the view inside the map.
    pub clamp_to_map: bool,
    /// How hard (screen pixels) the view shakes when the hero is hit; 0: no shake.
    pub shake: f64,
    /// Tilt of 3D models and effects, in degrees; `None`: the game's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tilt_deg: Option<f64>,
    /// Size of a cell (width, height in pixels) on new maps.
    pub new_map_cell: [f64; 2],
}

impl Default for CameraSettings {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            min_zoom: 0.25,
            max_zoom: 4.0,
            wheel_speed: 1.0,
            pan_speed: 900.0,
            free_pan: true,
            follow_smoothing: 0.0,
            dead_zone: 0.0,
            clamp_to_map: false,
            shake: 0.0,
            tilt_deg: None,
            new_map_cell: [64.0, 32.0],
        }
    }
}

/// What a new character starts with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NewCharacter {
    pub level: u32,
    pub money: u64,
    /// Item types carried.
    pub bag: Vec<u32>,
    /// Item types worn.
    pub equipment: Vec<u32>,
    /// Skills they know.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<u32>,
}

impl Default for NewCharacter {
    fn default() -> Self {
        Self {
            level: 1,
            money: 0,
            bag: Vec::new(),
            equipment: Vec::new(),
            skills: Vec::new(),
        }
    }
}

/// The money cut: a teammate below level `picker_below` picking up the money of a teammate of
/// level `owner_from` or more, at least `gap` levels above it, gives the owner `percent` of it
/// (0: none).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TeamMoneyCut {
    pub percent: u32,
    pub picker_below: u32,
    pub owner_from: u32,
    pub gap: u32,
}

/// The names of the NPC extra values that make an NPC one to hit: an NPC with a `max_life`
/// value above 0 can be hit; empty names: no NPC can.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NpcCombatSettings {
    /// Its most life.
    pub max_life: String,
    /// Its life at the start.
    pub life: String,
    /// Its level.
    pub level: String,
    /// Its defence.
    pub defence: String,
    /// The extra value naming its destruction's event `npc_destroyed_<value>` (shared by NPCs
    /// with one chain); empty or 0: `npc_destroyed_<NPC id>`.
    pub destroyed_event: String,
}

impl NpcCombatSettings {
    fn is_off(&self) -> bool {
        self.max_life.is_empty()
    }
}

/// The simulation's numbers: timing, ranges, the bag, the start, game masters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameplaySettings {
    /// Game time per simulation tick (ms).
    pub tick_ms: u64,
    /// Cells around a player within which it sees others.
    pub view_range: i32,
    /// Cells from an NPC within which a player can start talking to it.
    pub talk_range: i32,
    /// Cells from an NPC within which a player can trade with it.
    pub trade_range: i32,
    /// Cells a player can jump at once.
    pub jump_range: i32,
    /// Cells within which "attack nearest" looks for a monster.
    pub attack_search: i32,
    /// Cells a monster chases away from its spawn area before giving up.
    pub monster_leash: i32,
    /// Cells a monster wanders from its spawn area.
    pub monster_wander: i32,
    /// How long a dead monster lies before it vanishes.
    pub corpse_seconds: f64,
    /// How long a dead player waits to come back.
    pub revive_seconds: f64,
    /// A dead player comes back only when it asks (after the wait), not by itself.
    pub revive_by_request: bool,
    /// Dead players get up as ghosts and may walk until they come back.
    pub ghosts_walk: bool,
    /// How long a drop stays on the ground.
    pub item_life_seconds: f64,
    /// How long a killer's loot is its own (and its spouse's and team's, as the team shares);
    /// 0: anyone's at once.
    #[serde(default)]
    pub loot_protection_seconds: f64,
    /// Item types a team never shares while protected (Conquer: DragonBalls and Meteors).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub team_never_shared: Vec<u32>,
    /// A share of the money a low-level teammate picks up from a high-level one's loot.
    #[serde(default)]
    pub team_money_cut: TeamMoneyCut,
    /// Items a bag holds.
    pub bag_size: u32,
    /// The money's name, as players see it.
    pub currency: String,
    /// Other currencies beside the money (gems, honour), by name; items may be priced in them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub currencies: Vec<String>,
    /// Crime and PK points: who gets a bad name for fighting other players.
    pub pk: PkSettings,
    /// The numbers every player keeps (strength, rebirths...), for scripts and formulas.
    #[serde(rename = "attribute", skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<AttributeDef>,
    /// Names of the numbers items keep (up to 8: plus level, gems, blessing...), in order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub item_attributes: Vec<String>,
    /// Item attributes that hold another item (a gem in a socket): the item type is
    /// `socket_item_base` + the value. What it gives counts while the item is worn (fighters'
    /// `socket_<name>` facts).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sockets: Vec<String>,
    #[serde(skip_serializing_if = "is_zero")]
    pub socket_item_base: u32,
    /// The equipment slots items are worn in; empty: the game's own.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub equipment_slots: Vec<EquipmentSlot>,
    /// Item qualities from the lowest; each step up adds `quality_bonus_percent` to what an
    /// item gives.
    pub qualities: Vec<String>,
    pub quality_bonus_percent: u32,
    /// Items one storage NPC keeps for a player.
    #[serde(default = "storage_size")]
    pub storage_size: u32,
    /// NPCs that can be hit (training goals, gates): their extra values' names.
    #[serde(default, skip_serializing_if = "NpcCombatSettings::is_off")]
    pub npc_combat: NpcCombatSettings,
    /// Cells from a killed monster within which teammates share its kill experience; 0: the
    /// view range.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub team_exp_range: u32,
    /// The player text variable naming its spouse (a spouse shares more of a kill); empty:
    /// none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub spouse_text: String,
    /// The rank from which a guild's member invites and takes in applicants (0: its leader
    /// alone).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub guild_invite_rank: u32,
    /// The fund a guild needs to take a member in (0: none).
    #[serde(default)]
    pub guild_join_min_fund: i64,
    /// The most members a guild has (0: no limit).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub guild_max_members: u32,
    /// The item attribute that picks an item's look among its type's icons (Conquer: the
    /// colour of helmets, armour and shields); empty: none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub item_look_attribute: String,
    /// The attribute holding points a player spends on `point_attributes` (empty: none).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub points_attribute: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub point_attributes: Vec<String>,
    /// The rank a player joining a guild gets (the game's rank numbers).
    #[serde(default = "guild_member_rank")]
    pub guild_member_rank: u32,
    /// Chance (percent) that a worn item with durability loses a point when its wearer hits
    /// (weapons) or is hit (armour).
    pub wear_percent: u32,
    /// The map new characters start on (a project map's name or a content map path); empty:
    /// the game's first map.
    pub start_map: String,
    /// The cell they start on; `None`: the map's centre.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_cell: Option<[i32; 2]>,
    pub new_character: NewCharacter,
    /// Everyone may use game-master commands (handy while making the game).
    pub gm_everyone: bool,
    /// Accounts that may use game-master commands.
    pub gm_accounts: Vec<String>,
}

fn storage_size() -> u32 {
    20
}

fn guild_member_rank() -> u32 {
    1
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl Default for GameplaySettings {
    fn default() -> Self {
        Self {
            tick_ms: 50,
            view_range: 18,
            talk_range: 3,
            trade_range: 18,
            jump_range: 16,
            attack_search: 10,
            monster_leash: 24,
            monster_wander: 4,
            corpse_seconds: 3.0,
            revive_seconds: 10.0,
            revive_by_request: false,
            ghosts_walk: false,
            item_life_seconds: 60.0,
            bag_size: 40,
            currency: "Gold".into(),
            currencies: Vec::new(),
            pk: PkSettings::default(),
            attributes: Vec::new(),
            item_attributes: Vec::new(),
            sockets: Vec::new(),
            storage_size: storage_size(),
            guild_member_rank: guild_member_rank(),
            guild_invite_rank: 0,
            item_look_attribute: String::new(),
            points_attribute: String::new(),
            point_attributes: Vec::new(),
            guild_join_min_fund: 0,
            guild_max_members: 0,
            team_exp_range: 0,
            loot_protection_seconds: 0.0,
            team_never_shared: Vec::new(),
            team_money_cut: TeamMoneyCut::default(),
            npc_combat: NpcCombatSettings::default(),
            spouse_text: String::new(),
            socket_item_base: 0,
            equipment_slots: Vec::new(),
            qualities: ["Common", "Fine", "Rare", "Epic", "Legendary"]
                .map(String::from)
                .to_vec(),
            quality_bonus_percent: 10,
            wear_percent: 10,
            start_map: String::new(),
            start_cell: None,
            new_character: NewCharacter::default(),
            gm_everyone: true,
            gm_accounts: Vec::new(),
        }
    }
}

/// A number a game keeps on every player (strength, rebirths, stamina...): its name as
/// scripts and formulas name it, what it is, a new player's value and the range it stays in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AttributeDef {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub doc: String,
    pub default: i64,
    /// Lowest value; going below it fails (a cost that can't be paid). `None`: no limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    /// Highest value; more is cut off. `None`: no limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
}

/// Crime and PK points (TQ's blue, red and black names): attacking a player of good name
/// makes the attacker a criminal for a while; killing one earns PK points, which wear off;
/// enough of them make a red, then a black name. Maps marked free-PK bring neither.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PkSettings {
    /// Seconds an attacker of a player of good name stays a criminal (0: never).
    pub crime_seconds: f64,
    /// Seconds a criminal who kills stays one.
    pub crime_kill_seconds: f64,
    /// PK points for killing a player of good name.
    pub kill_points: i32,
    /// PK points from which a name is red, and black.
    pub red_at: i32,
    pub black_at: i32,
    /// Every this many seconds a player loses a PK point (0: never).
    pub decay_seconds: f64,
}

impl Default for PkSettings {
    /// TQ's: 15 s of crime (120 after a kill), 10 points a kill, red at 30, black at 100, a
    /// point off every 6 minutes.
    fn default() -> Self {
        Self {
            crime_seconds: 15.0,
            crime_kill_seconds: 120.0,
            kill_points: 10,
            red_at: 30,
            black_at: 100,
            decay_seconds: 360.0,
        }
    }
}

/// Most numbers an item keeps beside its type, count, durability and quality.
pub const ITEM_ATTRIBUTES: usize = 8;

/// An equipment slot: the number item types name it by, and what players see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquipmentSlot {
    pub id: u8,
    pub name: String,
}

impl GameplaySettings {
    /// The name of quality `step` (the highest named for steps past the list).
    pub fn quality_name(&self, step: u8) -> &str {
        self.qualities
            .get(usize::from(step))
            .or(self.qualities.last())
            .map_or("", String::as_str)
    }

    /// Whether `account` may use game-master commands.
    pub fn is_gm(&self, account: &str) -> bool {
        self.gm_everyone
            || self
                .gm_accounts
                .iter()
                .any(|a| a.eq_ignore_ascii_case(account))
    }
}

/// How the client draws.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphicsSettings {
    /// Waits for the display between frames (no tearing).
    pub vsync: bool,
    /// Most frames per second; 0: no limit.
    pub frame_cap: u32,
    /// Colour around and behind the map.
    pub background: [u8; 3],
    /// Colour of the light over the scene (white: as drawn; darker or tinted for night,
    /// dusk or caves). Effects keep their glow.
    pub light: [u8; 3],
    /// Shades models as if lit by the sun from one side.
    pub shade_models: bool,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            vsync: true,
            frame_cap: 0,
            background: [1, 1, 1],
            light: [255, 255, 255],
            shade_models: false,
        }
    }
}

/// Volumes and how far sounds carry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    pub master_volume: f64,
    pub music_volume: f64,
    pub sound_volume: f64,
    /// Cells from the hero at which a sound fades out completely.
    pub hearing_range: f64,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master_volume: 1.0,
            music_volume: 0.5,
            sound_volume: 0.8,
            hearing_range: 18.0,
        }
    }
}

/// A server front for another game client, beside the engine's own protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrontSettings {
    /// Which client it serves, e.g. `conquer-5065`.
    pub kind: String,
    /// The address its clients are sent to after logging in.
    pub public_ip: String,
}

/// The game server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkSettings {
    /// The port of the engine's protocol.
    pub port: u16,
    /// How often logged-in characters are saved.
    pub save_interval_seconds: f64,
    /// How long a client waits for the server when joining.
    pub join_timeout_seconds: f64,
    /// Fronts for other clients.
    #[serde(rename = "front", skip_serializing_if = "Vec::is_empty")]
    pub fronts: Vec<FrontSettings>,
    /// The servers the client's login offers (none: this machine's, on `port`).
    #[serde(rename = "server", skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<ServerEntry>,
    /// The groups the login sorts servers into (none: one plain list).
    #[serde(rename = "group", skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<ServerGroup>,
    /// The picture of this machine's server when no servers are listed (a skin section).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub local_icon: String,
}

/// A server the login offers: the name players see and its `host:port`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerEntry {
    pub name: String,
    pub address: String,
    /// The group it is listed in (a group's name; empty: the first group).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub group: String,
    /// Its picture on the login (a skin section).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub icon: String,
}

/// A group of servers on the login (TQ's `Server.dat` groups): its name and picture.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerGroup {
    pub name: String,
    /// A skin section.
    pub icon: String,
}

impl NetworkSettings {
    /// The servers to log in to: the listed ones, else this machine's.
    pub fn login_servers(&self) -> Vec<ServerEntry> {
        if !self.servers.is_empty() {
            return self.servers.clone();
        }
        vec![ServerEntry {
            name: "This computer".into(),
            address: format!("127.0.0.1:{}", self.port),
            icon: self.local_icon.clone(),
            ..Default::default()
        }]
    }
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            port: 7101,
            save_interval_seconds: 60.0,
            join_timeout_seconds: 8.0,
            fronts: Vec::new(),
            servers: Vec::new(),
            groups: Vec::new(),
            local_icon: String::new(),
        }
    }
}

/// What a player chose in the game's Settings menu: kept on their machine, over the
/// project's settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerPrefs {
    /// Loudness of everything, music and sounds, 0 to 1 (times the project's volumes).
    pub master: f32,
    pub music: f32,
    pub sound: f32,
    pub fullscreen: bool,
}

impl Default for PlayerPrefs {
    fn default() -> Self {
        Self {
            master: 1.0,
            music: 1.0,
            sound: 1.0,
            fullscreen: false,
        }
    }
}

/// Text in the game's languages.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalizationSettings {
    /// Font files of the project (TTF/OTF) the game's text is drawn with, in order: the first
    /// draws what it can, the next ones the characters it lacks (e.g. Chinese).
    pub fonts: Vec<String>,
}

/// Players' characters: how many an account has and the looks a new one picks from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CharacterSettings {
    /// Characters one account may have.
    pub per_account: u32,
    /// The looks character creation offers, in order (none: the game's default look).
    pub looks: Vec<CreationLook>,
    /// The classes creation offers first (TQ's RoleChoose), each with some of the looks
    /// (none: creation starts at the looks).
    #[serde(rename = "class", skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<CreationClass>,
}

/// A class character creation offers: its name, what it is, the profession it gives and
/// the looks it may wear.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CreationClass {
    pub name: String,
    /// Text, or `@file:key`: the text of line `key=…` of a game file (`\n` breaks lines).
    pub description: String,
    /// The profession it gives, as the game numbers them.
    pub profession: u32,
    /// Which creation looks it offers, by index (none: all of them).
    pub looks: Vec<usize>,
    /// The figure shown while it is picked: one of the game's showcase figures (none: its
    /// first look).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub showcase: Option<u32>,
}

impl Default for CharacterSettings {
    fn default() -> Self {
        Self {
            per_account: 3,
            looks: Vec::new(),
            classes: Vec::new(),
        }
    }
}

/// A look character creation offers: its name and the game's body and hair.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CreationLook {
    pub name: String,
    pub body: u32,
    pub hair: u32,
}

/// The game UI drawn from data (TQ's model: a layout, a skin and the controls file); with no
/// layout the engine's own UI shows.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    /// Where dialogs and controls sit (`GUI.ini` form), an asset path.
    pub layout: String,
    /// The images controls show (`Control.Ani` form).
    pub skin: String,
    /// More images (the same form) for sections the skin hasn't: server pictures.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skins: Vec<String>,
    /// Each control's kind, image, binding and action.
    pub controls: String,
    /// Sounds (asset paths) of a dialog opening and closing and a button clicked; none when
    /// empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sound_open: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sound_close: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sound_click: String,
    /// The sounds of the hero selling an item and changing what it wears; none when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sound_sell: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sound_equip: String,
}

impl UiSettings {
    /// Whether the game UI comes from data.
    pub fn from_data(&self) -> bool {
        !self.layout.is_empty() && !self.controls.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_usable() {
        assert!(ProjectSettings::default().problems().is_empty());
    }

    #[test]
    fn bad_values_are_named() {
        let mut s = ProjectSettings::default();
        s.camera.min_zoom = 3.0;
        s.gameplay.tick_ms = 0;
        assert_eq!(s.problems().len(), 2);
    }

    #[test]
    fn game_masters() {
        let mut g = GameplaySettings {
            gm_everyone: false,
            gm_accounts: vec!["Admin".into()],
            ..GameplaySettings::default()
        };
        assert!(g.is_gm("admin") && !g.is_gm("alice"));
        g.gm_everyone = true;
        assert!(g.is_gm("alice"));
    }
}
