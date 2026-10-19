//! A game's server data brought into a project in one step: a game reads its own server's
//! data (Conquer: TQ's SQL dump) into [`ServerData`] (`GameModule::import_server_data`), and
//! [`write`] lays it into the project: tables, every map's population (merged with what the
//! project already has) and the map list for the settings, with a report.

use c3_core::settings::MapEntry;
use c3_data::Table;
use c3_project::Project;
use c3_scene::Population;
use std::collections::BTreeMap;

/// Where the report of the last import is written, in the project folder.
pub const REPORT_FILE: &str = "import_report.txt";

/// A server's data as a project keeps it.
#[derive(Debug, Default)]
pub struct ServerData {
    /// The map list: one entry per map.
    pub maps: Vec<MapEntry>,
    /// Each map's population, by map key.
    pub populations: BTreeMap<String, Population>,
    pub tables: Vec<Table>,
    /// NPC behaviour as blueprints, by name (NPCs name them as `graphs/<name>`).
    pub blueprints: Vec<(String, c3_script::blueprint::Blueprint)>,
    /// What was read and what was left out, a line each.
    pub report: Vec<String>,
}

/// What [`write`] did: the settings' new map list (for the host to set through its
/// settings document, so it undoes) and the report's lines.
pub struct Written {
    pub maps: Vec<MapEntry>,
    pub report: Vec<String>,
    /// Files that couldn't be written.
    pub problems: Vec<String>,
}

/// Writes `data` into `project`: its tables (replacing those of the same name) and maps'
/// populations (over the project's own, [`merge_population`]); the map list merged into
/// `listed` comes back, and the report is written to [`REPORT_FILE`].
pub fn write(project: &Project, data: &ServerData, listed: &[MapEntry], source: &str) -> Written {
    let mut problems = Vec::new();
    let mut report = vec![format!("Imported {source}")];
    report.extend(data.report.iter().cloned());
    for table in &data.tables {
        let path = project.table_file(&table.name);
        let written = (path.parent())
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| c3_data::save_table(table, &path));
        if let Err(e) = written {
            problems.push(format!("table {}: {e}", table.name));
        }
    }
    let scripts = project.scripts_dir();
    for (name, blueprint) in &data.blueprints {
        let file = (scripts.join(c3_script::blueprint::BLUEPRINT_DIR))
            .join(format!("{name}{}", c3_script::blueprint::BLUEPRINT_SUFFIX));
        let written = c3_script::blueprint::save_blueprint(blueprint, &file)
            .and_then(|()| c3_script::blueprint::build_blueprint(&scripts, name));
        match written {
            Ok(problems) => problems.iter().for_each(|p| {
                report.push(format!("Blueprint {name}: {}", p.message));
            }),
            Err(e) => problems.push(format!("blueprint {name}: {e}")),
        }
    }
    for (key, population) in &data.populations {
        let path = project.population_file(&crate::map_list::short_name(key));
        let own = c3_scene::native::load_population(&path).unwrap_or_default();
        let (population, notes) = merge_population(&own, population);
        report.extend(
            notes
                .into_iter()
                .map(|n| format!("Kept the project's {key}: {n}")),
        );
        let written = (path.parent())
            .map_or(Ok(()), std::fs::create_dir_all)
            .map_err(|e| e.to_string())
            .and_then(|()| {
                c3_scene::native::save_population(&population, &path).map_err(|e| e.to_string())
            });
        if let Err(e) = written {
            problems.push(format!("population of {key}: {e}"));
        }
    }
    report.extend(problems.iter().map(|p| format!("Not written: {p}")));
    let _ = std::fs::write(project.root.join(REPORT_FILE), report.join("\n") + "\n");
    Written {
        maps: merge_maps(listed, &data.maps),
        report,
        problems,
    }
}

/// The settings' map list with `imported` in it: entries for the same map are replaced
/// (keeping what the project set that the import doesn't say: music, light), others kept.
pub fn merge_maps(listed: &[MapEntry], imported: &[MapEntry]) -> Vec<MapEntry> {
    let same = |a: &str, b: &str| {
        a.replace('\\', "/")
            .eq_ignore_ascii_case(&b.replace('\\', "/"))
    };
    let mut maps: Vec<MapEntry> = listed.to_vec();
    for entry in imported {
        match maps.iter_mut().find(|m| same(&m.key, &entry.key)) {
            Some(old) => {
                old.id = entry.id;
                old.name = entry.name.clone();
                old.pk = entry.pk;
            }
            None => maps.push(entry.clone()),
        }
    }
    maps
}

/// A map's population `imported` over the project's `own`: rows with the same id are the
/// import's, keeping a script the project gave the same NPC (same id and name) over the
/// import's; what only the project has stays, an NPC whose id the import gives another NPC moving to a free id.
/// Each move is said in the notes.
pub fn merge_population(own: &Population, imported: &Population) -> (Population, Vec<String>) {
    let mut out = imported.clone();
    let mut notes = Vec::new();
    for npc in &mut out.npcs {
        if let Some(mine) = own
            .npcs
            .iter()
            .find(|n| n.id == npc.id && n.name == npc.name)
            && !mine.script.is_empty()
        {
            npc.script = mine.script.clone();
        }
    }
    let mut next = (out.npcs.iter().chain(&own.npcs))
        .map(|n| n.id)
        .max()
        .unwrap_or(0)
        + 1;
    for mine in &own.npcs {
        match imported.npcs.iter().find(|i| i.id == mine.id) {
            Some(theirs) if theirs.name == mine.name => {}
            Some(theirs) => {
                let mut moved = mine.clone();
                moved.id = next;
                next += 1;
                notes.push(format!(
                    "NPC {} {} became {} (the import's {} is {})",
                    mine.id, mine.name, moved.id, mine.id, theirs.name
                ));
                out.npcs.push(moved);
            }
            None => out.npcs.push(mine.clone()),
        }
    }
    let spawns = own
        .spawns
        .iter()
        .filter(|s| !imported.spawns.iter().any(|i| i.id == s.id));
    out.spawns.extend(spawns.cloned());
    let links = own
        .links
        .iter()
        .filter(|l| !imported.links.iter().any(|i| i.portal == l.portal));
    out.links.extend(links.copied());
    out.regions.extend(own.regions.iter().cloned());
    (out, notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_scene::Npc;

    fn npc(id: u32, name: &str, script: &str) -> Npc {
        Npc {
            id,
            name: name.into(),
            cell: (1, 1),
            direction: 0,
            look: 1,
            kind: 0,
            script: script.into(),
            extra: Default::default(),
        }
    }

    #[test]
    fn merging_keeps_the_projects_scripts_and_npcs() {
        let own = Population {
            npcs: vec![
                npc(1, "Shopkeeper", "npc/shop"),
                npc(3, "Guide", "npc/guide"),
                npc(9, "Mine", ""),
            ],
            ..Population::default()
        };
        let imported = Population {
            npcs: vec![npc(1, "Shopkeeper", ""), npc(3, "Pharmacist", "")],
            ..Population::default()
        };
        let (out, notes) = merge_population(&own, &imported);
        let find = |name: &str| out.npcs.iter().find(|n| n.name == name).unwrap();
        assert_eq!(find("Shopkeeper").script, "npc/shop");
        assert_eq!(find("Pharmacist").script, "");
        assert_eq!(
            (find("Guide").id, find("Guide").script.as_str()),
            (10, "npc/guide")
        );
        assert_eq!(find("Mine").id, 9);
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn map_lists_merge_by_key() {
        let listed = vec![MapEntry {
            key: "map/map/newplain.DMap".into(),
            id: 9,
            music: "a.mp3".into(),
            ..MapEntry::default()
        }];
        let imported = vec![MapEntry {
            key: "map\\map\\NEWPLAIN.DMap".into(),
            id: 1002,
            name: "Twin City".into(),
            pk: true,
            ..MapEntry::default()
        }];
        let merged = merge_maps(&listed, &imported);
        assert_eq!(merged.len(), 1);
        assert_eq!(
            (merged[0].id, merged[0].music.as_str(), merged[0].pk),
            (1002, "a.mp3", true)
        );
    }
}
