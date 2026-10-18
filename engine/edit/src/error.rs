use c3_scene::CellPos;
use std::fmt;

/// Why a command was refused. Refused commands change nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    OutOfBounds(CellPos),
    PortalExists(CellPos),
    NoPortal(i32),
    PortalIdTaken(i32),
    NoEntity(usize),
    NoNpc(u32),
    NpcIdTaken(u32),
    NoSpawn(u32),
    SpawnIdTaken(u32),
    NoRow(i64),
    NoColumn(String),
    /// A value of the wrong kind for its column.
    WrongKind(String),
    /// A row with the wrong number of values.
    WrongWidth(usize),
    KeyTaken(i64),
    /// An area covering more cells than a single command may touch.
    AreaTooLarge(usize),
    InvalidArea(String),
    /// A settings path that names no setting.
    NoSetting(String),
    /// A settings value of the wrong type, or settings that break a rule.
    InvalidSetting(String),
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds((x, y)) => write!(f, "cell {x}, {y} is outside the map"),
            Self::PortalExists((x, y)) => write!(f, "cell {x}, {y} already has a portal"),
            Self::NoPortal(id) => write!(f, "there is no portal {id}"),
            Self::PortalIdTaken(id) => write!(f, "portal id {id} is already used"),
            Self::NoEntity(i) => write!(f, "there is no entity {i}"),
            Self::NoNpc(id) => write!(f, "there is no NPC {id}"),
            Self::NpcIdTaken(id) => write!(f, "NPC id {id} is already used"),
            Self::NoSpawn(id) => write!(f, "there is no spawn area {id}"),
            Self::SpawnIdTaken(id) => write!(f, "spawn area id {id} is already used"),
            Self::NoRow(key) => write!(f, "there is no row {key}"),
            Self::NoColumn(name) => write!(f, "there is no column {name}"),
            Self::WrongKind(name) => write!(f, "that value doesn't fit the {name} column"),
            Self::WrongWidth(n) => write!(f, "the row has {n} values, not one per column"),
            Self::KeyTaken(key) => write!(f, "key {key} is already used"),
            Self::AreaTooLarge(n) => write!(
                f,
                "the area covers {n} cells, more than one command may change ({})",
                crate::MAX_AREA_CELLS
            ),
            Self::InvalidArea(why) => write!(f, "invalid area: {why}"),
            Self::NoSetting(path) => write!(f, "there is no setting {path}"),
            Self::InvalidSetting(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for EditError {}
