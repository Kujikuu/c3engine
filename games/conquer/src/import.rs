//! A Conquer project's server data in one step: TQ's SQL dump (`cq_finalized_mariadb.sql`
//! and the like) read into what the engine runs on. Every map of `cq_map` becomes an entry of
//! the settings' map list (its id, name and whether players may fight there); every map's
//! NPCs, dynamic NPCs, monster spawns and portal destinations its population file; and the
//! game-data tables (items, monsters, magic, shops, levels, points...) project tables. What
//! couldn't be placed is written down in the report.

use crate::Content;
use crate::server_data::{DATA_TABLES, ServerPopulation};
use c3_core::settings::MapEntry;
use c3_play::server_data::ServerData;
use c3_scene::Population;
use std::collections::BTreeMap;

/// `cq_map.type` bits meaning players may not fight (no PvP, no attack at all).
const NO_PVP: u32 = 0x8 | 0x10;
/// `cq_map.type` bits of maps where fights bring no crime or PK points (PK field, guild map,
/// prison).
const PK_FREE: u32 = 0x1 | 0x80 | 0x100;

/// One `cq_map` row's fields the engine uses.
struct MapRow {
    id: u32,
    name: String,
    mapdoc: u32,
    kind: u32,
    /// Where its first portal is (players arrive there).
    portal0: [i32; 2],
    /// The map players who die here come back on (at its first portal).
    reborn_map: u32,
}

/// Where players who die on `row`'s map come back: its reborn map, at that map's first
/// portal (battle spec S8–S9).
fn revive_point(rows: &[MapRow], row: &MapRow) -> Option<c3_core::settings::RevivePoint> {
    let reborn = rows
        .iter()
        .find(|r| r.id == row.reborn_map && row.reborn_map != 0)?;
    Some(c3_core::settings::RevivePoint {
        map: reborn.id,
        cell: reborn.portal0,
    })
}

fn map_rows(sql: &str) -> (Vec<MapRow>, usize) {
    let mut rows = Vec::new();
    let mut skipped = 0;
    for line in sql.lines() {
        let Some(values) = crate::server_data::insert_values(line, "cq_map") else {
            continue;
        };
        let get = |i: usize| values.get(i).cloned().flatten().unwrap_or_default();
        let number = |i: usize| get(i).trim().parse::<u32>().ok();
        match (values.len(), number(0), number(3), number(4)) {
            (22, Some(id), Some(mapdoc), Some(kind)) => rows.push(MapRow {
                id,
                name: get(1),
                mapdoc,
                kind,
                portal0: [
                    number(11).unwrap_or(0) as i32,
                    number(12).unwrap_or(0) as i32,
                ],
                reborn_map: number(13).unwrap_or(0),
            }),
            _ => skipped += 1,
        }
    }
    (rows, skipped)
}

/// Reads `sql` against the client `content` (whose `GameMap.dat` says which file each map
/// id is).
pub fn import_server_data(content: &Content, sql: &str) -> ServerData {
    let mut out = ServerData::default();
    let files = content.list_maps();
    // The key a map file goes by: the file as the client folder spells it.
    let key_of = |id: u32| -> Option<String> {
        let path = content.map_path(id)?.replace('\\', "/");
        let lower = path.to_ascii_lowercase();
        Some(
            files
                .iter()
                .find(|f| f.to_ascii_lowercase() == lower)
                .cloned()
                .unwrap_or(path),
        )
    };

    let (mut rows, skipped) = map_rows(sql);
    rows.sort_by_key(|r| r.id);
    let mut by_key: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    let mut unknown = Vec::new();
    for row in &rows {
        let Some(key) = key_of(row.mapdoc).or_else(|| key_of(row.id)) else {
            unknown.push(format!("{} {}", row.id, row.name));
            continue;
        };
        let ids = by_key.entry(key.clone()).or_default();
        // The first map of a file is the file; the others are copies of it, maps of their
        // own (mines, arenas, prisons): `file@id`.
        let key = match ids.is_empty() {
            true => key,
            false => format!("{key}{}{}", c3_play::map_list::INSTANCE, row.id),
        };
        out.maps.push(MapEntry {
            key,
            id: row.id,
            name: row.name.clone(),
            pk: row.kind & NO_PVP == 0,
            pk_free: row.kind & PK_FREE != 0,
            revive: revive_point(&rows, row),
            ..MapEntry::default()
        });
        ids.push(row.id);
    }
    out.report.push(format!(
        "Maps: {} cq_map rows, {} maps of {} map files ({} copies of a file)",
        rows.len(),
        out.maps.len(),
        by_key.len(),
        out.maps.len() - by_key.len(),
    ));
    if !unknown.is_empty() {
        out.report.push(format!(
            "  {} maps have no file in GameMap.dat and were left out: {}",
            unknown.len(),
            unknown.join(", ")
        ));
    }
    if skipped > 0 {
        out.report
            .push(format!("  {skipped} cq_map rows could not be read"));
    }

    // Each file's population: its rows under every id naming it.
    let mut server = ServerPopulation::from_sql(sql);
    // What NPCs do when clicked: TQ's tasks and actions as blueprints.
    let logic = crate::npc_logic::NpcLogic::from_sql(sql);
    let tables = c3_data::sql::import_tables(sql, DATA_TABLES);
    let converted = crate::npc_logic::convert(&logic, &server.npc_tasks(), &item_names(&tables));
    server.set_scripts(&converted.scripts);
    let mut placed = std::collections::HashSet::new();
    for (file, ids) in &by_key {
        // The file's own map: its ids in GameMap.dat but those of its copies.
        let mut own: Vec<u32> = (content.map_ids_for_path(file).into_iter())
            .filter(|id| !ids[1..].contains(id))
            .chain([ids[0]])
            .collect();
        own.sort_unstable();
        own.dedup();
        let maps = std::iter::once((file.clone(), own)).chain((ids[1..].iter()).map(|&id| {
            (
                format!("{file}{}{id}", c3_play::map_list::INSTANCE),
                vec![id],
            )
        }));
        for (key, ids) in maps {
            placed.extend(ids.iter().copied());
            let population = server.for_maps(&ids);
            if !population.npcs.is_empty()
                || !population.spawns.is_empty()
                || !population.links.is_empty()
            {
                out.populations.insert(key, population);
            }
        }
    }
    let count = |f: fn(&Population) -> usize| out.populations.values().map(f).sum::<usize>();
    let dynamic = (out.populations.values())
        .flat_map(|p| &p.npcs)
        .filter(|n| n.extra.contains_key(crate::server_data::DYNAMIC))
        .count();
    out.report.push(format!(
        "Population: {} maps, {} NPCs ({dynamic} dynamic), {} spawn areas, {} portal links",
        out.populations.len(),
        count(|p| p.npcs.len()),
        count(|p| p.spawns.len()),
        count(|p| p.links.len()),
    ));
    let mut stray: Vec<(u32, usize)> = (server.maps.iter())
        .filter(|(id, p)| !placed.contains(id) && !(p.npcs.is_empty() && p.spawns.is_empty()))
        .map(|(id, p)| (*id, p.npcs.len() + p.spawns.len()))
        .collect();
    stray.sort_unstable();
    if !stray.is_empty() {
        let list: Vec<String> = stray.iter().map(|(id, n)| format!("{id} ({n})")).collect();
        out.report.push(format!(
            "  rows on maps with no map file, left out (over 1000000: player homes, made while playing; map id (rows)): {}",
            list.join(", ")
        ));
    }
    if server.skipped > 0 {
        out.report.push(format!(
            "  {} population rows could not be read",
            server.skipped
        ));
    }

    out.report.extend(converted.report);
    out.blueprints = converted.blueprints;
    // The engine's rule tables made of TQ's, and TQ's formulas as the rules blueprint.
    let (rule_tables, problems) = crate::rules::engine_tables(&tables);
    out.report.extend(problems);
    out.report.push(format!(
        "Rules: {} engine tables ({}); formulas in blueprint {}",
        rule_tables.len(),
        (rule_tables.iter())
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        crate::rule_blueprint::RULES_BLUEPRINT
    ));
    out.starters.push((
        crate::rule_blueprint::RULES_BLUEPRINT.into(),
        crate::rule_blueprint::rules_blueprint(),
    ));
    out.tables = tables;
    out.tables.extend(rule_tables);
    for table in &out.tables {
        out.report
            .push(format!("Table {}: {} rows", table.name, table.rows.len()));
    }
    for missing in DATA_TABLES
        .iter()
        .filter(|t| !out.tables.iter().any(|x| x.name == **t))
    {
        out.report.push(format!("Table {missing}: not in the dump"));
    }
    out
}

/// Item types by name, from `cq_itemtype` (several qualities share a name).
fn item_names(tables: &[c3_data::Table]) -> std::collections::HashMap<String, Vec<u32>> {
    let mut names: std::collections::HashMap<String, Vec<u32>> = Default::default();
    let Some(items) = tables
        .iter()
        .find(|t| t.name == crate::server_data::ITEM_TABLE)
    else {
        return names;
    };
    let Some(name) = items.column("name") else {
        return names;
    };
    for row in &items.rows {
        let id = row.first().and_then(c3_data::Value::as_int);
        let text = row.get(name).and_then(c3_data::Value::as_text);
        if let (Some(id), Some(text)) = (id.and_then(|i| u32::try_from(i).ok()), text) {
            names.entry(text.to_owned()).or_default().push(id);
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_rows_say_id_name_and_pvp() {
        let sql = "INSERT INTO `cq_map` VALUES ('1002', 'Twin City', 'x', '1002', '24576', '0', '0000', '-1', '0000', '0000', '0000', '0438', '0398', '1002', '0000', '000', '000', '0000', '0000', '0000', '000', '4294967295');
INSERT INTO `cq_map` VALUES ('1005', 'Arena', 'x', '1005', '0009', '0', '0000', '-1', '0000', '0000', '0000', '0438', '0398', '1002', '0000', '000', '000', '0000', '0000', '0000', '000', '4294967295');";
        let (rows, skipped) = map_rows(sql);
        assert_eq!(skipped, 0);
        assert_eq!(
            (rows[0].id, rows[0].name.as_str(), rows[0].kind & NO_PVP),
            (1002, "Twin City", 0)
        );
        assert_ne!(rows[1].kind & NO_PVP, 0);
    }

    #[test]
    fn imports_the_tq_dump() {
        let dump =
            std::path::Path::new(r"D:\Conquer2\Tools\eosource\tq_re\db\cq_finalized_mariadb.sql");
        let client = std::path::Path::new(r"D:\Conquer2\Tools\eosource\client5065");
        let (Ok(bytes), Ok(content)) = (std::fs::read(dump), Content::open(client)) else {
            return;
        };
        let import = import_server_data(&content, &String::from_utf8_lossy(&bytes));
        let twin = import
            .maps
            .iter()
            .find(|m| m.id == 1002)
            .expect("Twin City");
        assert!(
            twin.key.to_ascii_lowercase().ends_with("newplain.dmap"),
            "{}",
            twin.key
        );
        let population = &import.populations[&twin.key];
        assert!(population.npcs.len() > 50, "{}", population.npcs.len());
        assert!(!population.spawns.is_empty() && !population.links.is_empty());
        // TQ's tables and the engine's rule tables made of them.
        assert_eq!(import.tables.len(), DATA_TABLES.len() + 7);
        assert!(
            import
                .starters
                .iter()
                .any(|(n, _)| n == crate::rule_blueprint::RULES_BLUEPRINT)
        );
        assert!(import.report.iter().any(|l| l.starts_with("Population:")));
        eprintln!(
            "{}",
            import.report.join(
                "
"
            )
        );
    }
}
