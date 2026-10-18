//! Playing a game: the runtime every host shares (the client, the server, the Studio's Scene,
//! Play and Preview). A game plugs in through [`GameModule`]: its content, how its roles look
//! ([`Looks`]), its rules ([`GameRules`]) and sounds. [`basic::BasicGame`] is the engine's own
//! game, for projects made from scratch.
//!
//! - [`world::World`]: a map being played, drawn with its actors.
//! - [`actor::Actor`]: a role's models walking and acting on the map.
//! - [`sim_view::SimView`]: a simulation's events shown on the world's actors.
//! - [`session::Session`]: a game being played, here or on a server, with its UI and sound.
//! - [`screen::GameScreen`]: a session with its camera, mouse and keys.
//! - [`serve::ServerHandle`]: a game server on a thread of its own.

pub mod actor;
pub mod audio;
pub mod basic;
pub mod builtin;
pub mod character;
pub mod cursor;
pub mod game;
pub mod module;
pub mod screen;
pub mod serve;
pub mod session;
pub mod settings;
pub mod sim_view;
pub mod world;

pub use character::{Attachment, Character, CharacterMotions, Look, LookKind, PartModel};
pub use module::{Act, GameModule, GameRules, Looks, load_project_scene};
