//! A project's settings as the runtime uses them: read from the project (or the game's
//! defaults), turned into new characters, start cells and the world's tilt.

use crate::module::GameModule;
use c3_assets::Image;
use c3_game::{GameplaySettings, PlayerSave};
use c3_project::{Project, ProjectSettings};
use c3_scene::{CellPos, Scene};
use c3_ui::fonts::GameFont;

/// The settings `project` plays by: its settings file, else `game`'s defaults. A file that
/// can't be read gives the defaults and says why.
pub fn load(project: &Project, game: &dyn GameModule) -> (ProjectSettings, Option<String>) {
    match project.load_settings() {
        Ok(Some(settings)) => match settings.problems().first() {
            None => (settings, None),
            Some(problem) => (
                game.default_settings(),
                Some(format!("Project Settings: {problem}; using the defaults")),
            ),
        },
        Ok(None) => (game.default_settings(), None),
        Err(e) => (
            game.default_settings(),
            Some(format!("Project Settings: {e}; using the defaults")),
        ),
    }
}

/// Whether the map answering to `map_ids` lets players attack each other (the map list's
/// `pk`).
pub fn pk_map(settings: &ProjectSettings, map_ids: &[u32]) -> bool {
    settings
        .maps
        .iter()
        .any(|m| m.pk && map_ids.contains(&m.id))
}

/// A new character called `name` at `cell`, as the settings make them.
pub fn new_character(settings: &GameplaySettings, name: &str, cell: CellPos) -> PlayerSave {
    let start = &settings.new_character;
    PlayerSave {
        level: start.level.max(1),
        money: start.money,
        bag: start.bag.iter().map(|&k| k.into()).collect(),
        equipment: start.equipment.iter().map(|&k| k.into()).collect(),
        skills: start.skills.clone(),
        ..PlayerSave::new(name, cell)
    }
}

/// The equipment slots players wear items in: the settings', else the game's own.
pub fn equipment_slots(settings: &ProjectSettings, game: &dyn GameModule) -> Vec<(u8, String)> {
    let slots = &settings.gameplay.equipment_slots;
    if slots.is_empty() {
        game.slot_names()
    } else {
        slots.iter().map(|s| (s.id, s.name.clone())).collect()
    }
}

/// Where new characters start on `scene`: the settings' cell, else the map's centre.
pub fn start_cell(settings: &GameplaySettings, scene: &Scene) -> CellPos {
    match settings.start_cell {
        Some([x, y]) => (x, y),
        None => (scene.grid.width as i32 / 2, scene.grid.height as i32 / 2),
    }
}

/// The map new characters start on: the settings', else the game's first.
pub fn start_map(
    settings: &GameplaySettings,
    project: &Project,
    game: &dyn GameModule,
) -> Option<String> {
    Some(settings.start_map.clone())
        .filter(|m| !m.is_empty())
        .or_else(|| game.default_map(project))
}

/// The tilt of 3D roles and effects: the settings', else the game's.
pub fn tilt_deg(settings: &ProjectSettings, game: &dyn GameModule) -> f32 {
    settings
        .camera
        .tilt_deg
        .map_or_else(|| game.tilt_deg(), |t| t as f32)
}

/// The fonts of the game's text (Localization), read through `game`; and the ones that
/// couldn't be read.
pub fn game_fonts(
    settings: &ProjectSettings,
    game: &dyn GameModule,
) -> (Vec<GameFont>, Vec<String>) {
    let mut problems = Vec::new();
    let fonts = settings
        .localization
        .fonts
        .iter()
        .filter_map(|path| match game.read_file(&path.replace('\\', "/")) {
            Some(bytes) => Some(GameFont {
                name: path.clone(),
                bytes,
            }),
            None => {
                problems.push(format!("Font {path} can't be found"));
                None
            }
        })
        .collect();
    (fonts, problems)
}

/// The window icon (Player > icon): an icon file or any image the game reads.
pub fn window_icon(settings: &ProjectSettings, game: &dyn GameModule) -> Option<Image> {
    let path = settings.player.icon.replace('\\', "/");
    if path.is_empty() {
        return None;
    }
    if path.to_ascii_lowercase().ends_with(".ico") {
        let bytes = game.read_file(&path)?;
        let cursor = c3_assets::cursor::decode(&bytes, &crate::native::decode_image).ok()?;
        return cursor.frames.into_iter().next().map(|f| f.image);
    }
    game.assets().image(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_characters_follow_the_settings() {
        let mut g = GameplaySettings::default();
        g.new_character.money = 500;
        g.new_character.equipment = vec![410003];
        g.new_character.level = 0;
        let save = new_character(&g, "Ann", (3, 4));
        assert_eq!((save.level, save.money), (1, 500));
        assert_eq!(save.equipment, [410003.into()]);
        assert_eq!(save.cell, (3, 4));
        let scene = crate::basic::blank_scene(40, 20, [64.0, 32.0]);
        assert_eq!(start_cell(&g, &scene), (20, 10));
        g.start_cell = Some([5, 6]);
        assert_eq!(start_cell(&g, &scene), (5, 6));
    }
}
