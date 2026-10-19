//! Playing a game: the runtime every host shares (the client, the server, the Studio's Scene,
//! Play and Preview). A game plugs in through [`GameModule`]: its content, how its roles look
//! ([`Looks`]), its rules ([`GameRules`]) and sounds. [`basic::BasicGame`] is the engine's own
//! game, for projects made from scratch.
//!
//! - [`project_game::ProjectGame`]: a game with the project's own assets ([`native`]) over
//!   its content.
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
pub mod controls;
pub mod cursor;
pub mod data_rules;
pub mod formula_rules;
pub mod game;
pub mod input;
pub mod map_list;
pub mod module;
pub mod native;
pub mod native_character;
pub mod native_effect;
pub mod preview;
pub mod project_game;
pub mod screen;
pub mod serve;
pub mod session;
pub mod settings;
pub mod sim_view;
pub mod touch;
pub mod ui_textures;
pub mod weather;
pub mod world;

pub use character::{Attachment, Character, CharacterMotions, Look, LookKind, PartModel};
pub use module::{Act, DataTables, GameModule, GameRules, Looks, TableRef, load_project_scene};
pub use project_game::ProjectGame;
