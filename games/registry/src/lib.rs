//! The games built into the engine's apps, and which one plays a project: a project with a
//! Conquer 5065 client as its content source plays Conquer; any other project plays the
//! engine's own game ([`c3_play::basic::BasicGame`]). The client, the server and the Studio
//! reach games only through here and [`GameModule`].

use c3_online::Front;
use c3_play::GameModule;
use c3_play::basic::BasicGame;
use c3_project::Project;
use c3_scene::Scene;
use game_conquer::ConquerModule;
use game_conquer::module::DEFAULT_MAP_ID;
use game_conquer::protocol::{ConquerFront, ConquerSetup};
use std::path::Path;
use std::rc::Rc;

/// The game that plays `project`.
pub fn open_project(project: &Project) -> Result<Rc<dyn GameModule>, String> {
    Ok(match project.tq_client() {
        Some((_, dir)) => Rc::new(ConquerModule::open(dir)?),
        None => Rc::new(BasicGame::new(project)),
    })
}

/// A client's map opened for viewing.
pub struct ClientMap {
    pub game: Rc<dyn GameModule>,
    /// The map file, as a content path.
    pub path: String,
    pub scene: Scene,
    /// What the import couldn't read.
    pub warnings: Vec<String>,
}

/// A Conquer client folder opened for viewing (no project): the game, and map `map` (a map
/// file of the client) or map `map_id` of its catalog (Twin City when neither is given).
pub fn open_client_map(
    dir: &Path,
    map: Option<&str>,
    map_id: Option<u32>,
) -> Result<ClientMap, String> {
    let game = ConquerModule::open(dir)?;
    let path = match map {
        Some(p) => p.to_owned(),
        None => {
            let id = map_id.unwrap_or(DEFAULT_MAP_ID);
            game.content()
                .map_path(id)
                .ok_or(format!("map {id} is not in GameMap.dat"))?
        }
    };
    let imported = game.content().load_map(&path)?;
    Ok(ClientMap {
        game: Rc::new(game),
        path,
        scene: imported.scene,
        warnings: imported.warnings,
    })
}

/// Conquer's ports for the original 5065 client.
pub use game_conquer::protocol::{ACCOUNT_PORT, GAME_PORT};

/// A front for the original Conquer 5065 client, on `listen` (an IP) at Conquer's ports, that
/// sends logged-in clients to `public_ip`.
pub fn conquer_front(listen: &str, public_ip: &str) -> Result<Box<dyn Front + Send>, String> {
    let setup = ConquerSetup::on(listen, public_ip);
    let front = ConquerFront::bind(&setup).map_err(|e| {
        format!(
            "can't listen on {} and {}: {e}",
            setup.account_listen, setup.game_listen
        )
    })?;
    Ok(Box::new(front))
}
