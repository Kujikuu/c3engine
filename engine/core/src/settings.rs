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
    pub graphics: GraphicsSettings,
    pub audio: AudioSettings,
    pub network: NetworkSettings,
    pub localization: LocalizationSettings,
}

impl ProjectSettings {
    /// Why these settings can't be used, if they can't (each problem in words).
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
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
        problems
    }
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
}

impl Default for NewCharacter {
    fn default() -> Self {
        Self {
            level: 1,
            money: 0,
            bag: Vec::new(),
            equipment: Vec::new(),
        }
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
    /// How long a drop stays on the ground.
    pub item_life_seconds: f64,
    /// Items a bag holds.
    pub bag_size: u32,
    /// The money's name, as players see it.
    pub currency: String,
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
            item_life_seconds: 60.0,
            bag_size: 40,
            currency: "Gold".into(),
            start_map: String::new(),
            start_cell: None,
            new_character: NewCharacter::default(),
            gm_everyone: true,
            gm_accounts: Vec::new(),
        }
    }
}

impl GameplaySettings {
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
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            vsync: true,
            frame_cap: 0,
            background: [1, 1, 1],
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
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            port: 7101,
            save_interval_seconds: 60.0,
            join_timeout_seconds: 8.0,
            fronts: Vec::new(),
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
