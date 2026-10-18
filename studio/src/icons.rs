//! The Studio's icon set: Phosphor Icons, the icon font `c3_ui::icons` bundles for the game UI
//! too. Icons are text, so they sit inline in labels and buttons:
//! `format!("{FLOPPY_DISK}  Save")`. Never use emoji for icons.

pub use c3_ui::icons::install;

// Codepoints from the Phosphor regular font. Keep this list to icons the Studio uses; find
// more with `python scripts/phosphor_icon.py engine/ui/assets/fonts/Phosphor.ttf <icon-name>...`.
pub const ARROW_CLOCKWISE: &str = "\u{E036}";
pub const ARROW_COUNTER_CLOCKWISE: &str = "\u{E038}";
pub const ARROW_DOWN: &str = "\u{E03E}";
pub const ARROW_UP: &str = "\u{E08E}";
pub const BROOM: &str = "\u{EC54}";
pub const CARET_RIGHT: &str = "\u{E13A}";
pub const CHATS: &str = "\u{E17C}";
pub const CHAT_TEARDROP_TEXT: &str = "\u{E178}";
pub const CHECK_CIRCLE: &str = "\u{E184}";
pub const COPY: &str = "\u{E1CA}";
pub const FILE_AUDIO: &str = "\u{EA20}";
pub const FILE_TEXT: &str = "\u{E23A}";
pub const FILE_CODE: &str = "\u{E914}";
pub const BOOK_OPEN: &str = "\u{E0E6}";
pub const FILM_STRIP: &str = "\u{E792}";
pub const FLAG: &str = "\u{E244}";
pub const FOLDER: &str = "\u{E24A}";
pub const GIT_FORK: &str = "\u{E27E}";
pub const IMAGE: &str = "\u{E2CA}";
pub const LINK_BREAK: &str = "\u{E2E4}";
pub const LIST_BULLETS: &str = "\u{E2F2}";
pub const PACKAGE: &str = "\u{E390}";
pub const PUZZLE_PIECE: &str = "\u{E596}";
pub const SCROLL: &str = "\u{EB7A}";
pub const SQUARES_FOUR: &str = "\u{E464}";
pub const CIRCLE: &str = "\u{E18A}";
pub const CLOCK_COUNTER_CLOCKWISE: &str = "\u{E1A0}";
pub const CORNERS_OUT: &str = "\u{E1D0}";
pub const CUBE: &str = "\u{E1DA}";
pub const CURSOR: &str = "\u{E1DC}";
pub const DATABASE: &str = "\u{E1DE}";
pub const DOOR_OPEN: &str = "\u{E7E6}";
pub const DOWNLOAD_SIMPLE: &str = "\u{E20C}";
pub const ERASER: &str = "\u{E21E}";
pub const FILE: &str = "\u{E230}";
pub const FLOPPY_DISK: &str = "\u{E248}";
pub const FOLDER_OPEN: &str = "\u{E256}";
pub const GAME_CONTROLLER: &str = "\u{E26E}";
pub const HAND: &str = "\u{E298}";
pub const INFO: &str = "\u{E2CE}";
pub const KEY: &str = "\u{E2D6}";
pub const LAYOUT: &str = "\u{E6D6}";
pub const MAGNIFYING_GLASS: &str = "\u{E30C}";
pub const MAGNIFYING_GLASS_MINUS: &str = "\u{E30E}";
pub const MAGNIFYING_GLASS_PLUS: &str = "\u{E310}";
pub const MAP_TRIFOLD: &str = "\u{E31A}";
pub const MOUNTAINS: &str = "\u{E7AE}";
pub const PAUSE: &str = "\u{E39E}";
pub const PERSON_SIMPLE: &str = "\u{E72E}";
pub const PLAY: &str = "\u{E3D0}";
pub const PLUS: &str = "\u{E3D4}";
pub const PROHIBIT: &str = "\u{E3DE}";
pub const SKIP_BACK: &str = "\u{E5A4}";
pub const SKIP_FORWARD: &str = "\u{E5A6}";
pub const SLIDERS: &str = "\u{E432}";
pub const SPARKLE: &str = "\u{E6A2}";
pub const SPEAKER_HIGH: &str = "\u{E44A}";
pub const STACK: &str = "\u{E466}";
pub const STOP: &str = "\u{E46C}";
pub const TERMINAL: &str = "\u{E47E}";
pub const TRASH: &str = "\u{E4A6}";
pub const TREE: &str = "\u{E6DA}";
pub const TREE_STRUCTURE: &str = "\u{E67C}";
pub const UPLOAD_SIMPLE: &str = "\u{E4C0}";
pub const USER: &str = "\u{E4C2}";
pub const USERS_THREE: &str = "\u{E68E}";
pub const WARNING: &str = "\u{E4E0}";
pub const X: &str = "\u{E4F6}";
pub const X_CIRCLE: &str = "\u{E4F8}";
