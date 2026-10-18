//! c3-core: what every part of the engine shares: the project's settings
//! ([`settings::ProjectSettings`]), the Input System's actions ([`input`]) and what the engine
//! knows about each asset ([`asset`]), read by the simulation, the runtime, the client, the
//! server and the Studio.

pub mod asset;
pub mod input;
pub mod settings;

pub use settings::ProjectSettings;
