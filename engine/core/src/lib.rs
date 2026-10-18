//! c3-core: what every part of the engine shares. Today that is the project's settings
//! ([`settings::ProjectSettings`]), read by the simulation, the runtime, the client, the
//! server and the Studio.

pub mod input;
pub mod settings;

pub use settings::ProjectSettings;
