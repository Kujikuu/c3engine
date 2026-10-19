//! File > Import Server Data: the game's own server data (Conquer: a TQ SQL dump) into the
//! project in one step: the map list (Project Settings > Maps, one undo step), every map's
//! NPCs, spawn areas and portal destinations (population files, merged with the project's),
//! the game-data tables, and a report (`import_report.txt`, also in the Console).

use crate::app::Studio;
use c3_play::server_data::{self, REPORT_FILE};
use std::path::Path;

impl Studio {
    /// Whether the game has server data to import.
    pub(crate) fn imports_server_data(&self) -> bool {
        (self.game.as_ref()).is_some_and(|g| g.server_data_kind().is_some())
    }

    /// Asks for a dump and imports it.
    pub(crate) fn pick_server_data(&mut self) {
        let kind = (self.game.as_ref())
            .and_then(|g| g.server_data_kind())
            .unwrap_or("SQL dump");
        let Some(file) = rfd::FileDialog::new()
            .set_title(format!("Import the game's server data ({kind})"))
            .add_filter("SQL dump", &["sql"])
            .pick_file()
        else {
            return;
        };
        self.import_server_data(&file);
    }

    /// Imports the dump `file` into the open project.
    pub(crate) fn import_server_data(&mut self, file: &Path) {
        if self.map_dirty() {
            self.error = Some(
                "Save or revert the open map first: the import rewrites maps' populations".into(),
            );
            return;
        }
        let (Some(project), Some(game)) = (self.project.clone(), self.game.clone()) else {
            return;
        };
        let dump = match std::fs::read(file) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.error = Some(format!("Can't read {}: {e}", file.display()));
                return;
            }
        };
        let Some(data) = game.import_server_data(&dump) else {
            self.error = Some(format!("{} has no server data to import", game.name()));
            return;
        };
        let listed = (self.settings.as_ref()).map_or_else(Vec::new, |e| e.settings().maps.clone());
        let written = server_data::write(&project, &data, &listed, &file.display().to_string());
        if let Some(editor) = &mut self.settings {
            let command = c3_edit::SettingsCommand::Set {
                path: "maps".into(),
                value: serde_json::to_value(&written.maps).unwrap_or_default(),
            };
            editor.doc.begin_group("Import server data");
            let listed = editor.doc.execute(&command);
            editor.doc.end_group();
            match listed {
                Ok(_) => self.settings_changed(),
                Err(e) => self.log_error(format!("Map list: {e}")),
            }
        }
        for line in &written.report {
            self.log_info(line.clone());
        }
        self.refresh_tables();
        self.world.refresh();
        // The open map shows its new population.
        if let Some(map) = self.doc.as_ref().map(|d| d.map.clone()) {
            self.load_map(&map);
        }
        match written.problems.len() {
            0 => self.notify(format!(
                "Imported {} maps' populations, {} maps and {} tables (report: {REPORT_FILE})",
                data.populations.len(),
                data.maps.len(),
                data.tables.len()
            )),
            n => self.error = Some(format!("Imported with {n} problems: see {REPORT_FILE}")),
        }
    }
}
