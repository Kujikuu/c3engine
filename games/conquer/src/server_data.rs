//! The server's map population in TQ's database schema (`cq_npc`, the dynamic NPCs of
//! `cq_dynanpc` (training stakes, gates, guild flags), `cq_generator`, and the portals'
//! destinations in `cq_passway` + `cq_portal`), which Redux also runs on: read from an
//! SQL dump into engine [`Npc`]s, [`Spawn`]s and [`PortalLink`]s, and NPCs and spawns written
//! back as SQL to load into the server's database.
//!
//! Engine fields carry what the editor shows and moves; every other column rides along in
//! `extra` under its column name, so a row exports as it was imported.

use c3_scene::{Npc, Population, PortalLink, PortalTarget, Spawn};
use std::collections::{BTreeMap, HashMap};

/// The table of item types (names, prices, stats).
pub const ITEM_TABLE: &str = "cq_itemtype";

/// The tables of TQ's server database that hold game data (not map population or accounts).
pub const DATA_TABLES: &[&str] = &[
    "cq_itemtype",
    "cq_itemaddition",
    "cq_monstertype",
    "cq_magictype",
    "cq_levexp",
    "cq_goods",
    "cq_rebirth",
    "cq_point_allot",
];

/// `extra` key holding the game map id a row belongs to.
pub const MAP_ID: &str = "mapid";

const NPC_COLUMNS: [&str; 29] = [
    "id",
    "ownerid",
    "playerid",
    "name",
    "type",
    "lookface",
    "idxserver",
    "mapid",
    "cellx",
    "celly",
    "task0",
    "task1",
    "task2",
    "task3",
    "task4",
    "task5",
    "task6",
    "task7",
    "data0",
    "data1",
    "data2",
    "data3",
    "datastr",
    "linkid",
    "life",
    "maxlife",
    "base",
    "sort",
    "itemid",
];

/// `cq_dynanpc`: `cq_npc`'s columns with an owner type for the player id, and defences.
const DYNANPC_COLUMNS: [&str; 31] = [
    "id",
    "ownerid",
    "ownertype",
    "name",
    "type",
    "lookface",
    "idxserver",
    "mapid",
    "cellx",
    "celly",
    "task0",
    "task1",
    "task2",
    "task3",
    "task4",
    "task5",
    "task6",
    "task7",
    "data0",
    "data1",
    "data2",
    "data3",
    "datastr",
    "linkid",
    "life",
    "maxlife",
    "base",
    "sort",
    "itemid",
    "defence",
    "magic_def",
];

/// `extra` key marking an NPC of `cq_dynanpc` (its value is `1`).
pub const DYNAMIC: &str = "dynamic";
/// `extra` key holding a dynamic NPC's own id when it had to take another.
const TQ_ID: &str = "tq_id";
/// Added to a dynamic NPC's id that a static NPC already has.
const DYNAMIC_ID_OFFSET: u32 = 10_000_000;

const PASSWAY_COLUMNS: [&str; 5] = [
    "id",
    "mapid",
    "passway_idx",
    "passway_mapid",
    "passway_mapportal",
];

const PORTAL_COLUMNS: [&str; 5] = ["id", "mapid", "portal_idx", "portal_x", "portal_y"];

const GENERATOR_COLUMNS: [&str; 14] = [
    "id",
    "mapid",
    "bound_x",
    "bound_y",
    "bound_cx",
    "bound_cy",
    "maxnpc",
    "rest_secs",
    "max_per_gen",
    "npctype",
    "timer_begin",
    "timer_end",
    "born_x",
    "born_y",
];

/// Columns the engine fields stand for (not kept in `extra`).
const NPC_OWN: [&str; 6] = ["id", "name", "type", "lookface", "cellx", "celly"];
const GENERATOR_OWN: [&str; 9] = [
    "id",
    "bound_x",
    "bound_y",
    "bound_cx",
    "bound_cy",
    "maxnpc",
    "rest_secs",
    "max_per_gen",
    "npctype",
];

use c3_data::sql::SqlValue as Value;

/// The population of every map in an SQL dump, by game map id. Rows whose column count
/// doesn't match the table are skipped and counted in `skipped`.
#[derive(Debug, Default)]
pub struct ServerPopulation {
    pub maps: HashMap<u32, Population>,
    pub skipped: usize,
}

impl ServerPopulation {
    /// Reads the `cq_npc` and `cq_generator` rows of an SQL dump. Later rows with the same id
    /// replace earlier ones, as reloading the dump into a database would.
    pub fn from_sql(sql: &str) -> Self {
        let mut npcs: BTreeMap<u32, Npc> = BTreeMap::new();
        let mut dynamic: BTreeMap<u32, Npc> = BTreeMap::new();
        let mut spawns: BTreeMap<u32, Spawn> = BTreeMap::new();
        // (map, passway) -> (target map, target portal); (map, portal) -> cell.
        let mut passways: BTreeMap<(u32, i32), (u32, i32)> = BTreeMap::new();
        let mut portals: HashMap<(u32, i32), (i32, i32)> = HashMap::new();
        let mut skipped = 0;
        for line in sql.lines() {
            if let Some(values) = insert_values(line, "cq_npc") {
                match npc_from_row(&NPC_COLUMNS, &values) {
                    Some(npc) => {
                        npcs.insert(npc.id, npc);
                    }
                    None => skipped += 1,
                }
            } else if let Some(values) = insert_values(line, "cq_dynanpc") {
                match npc_from_row(&DYNANPC_COLUMNS, &values) {
                    Some(mut npc) => {
                        npc.extra.insert(DYNAMIC.into(), "1".into());
                        dynamic.insert(npc.id, npc);
                    }
                    None => skipped += 1,
                }
            } else if let Some(values) = insert_values(line, "cq_generator") {
                match spawn_from_row(&values) {
                    Some(spawn) => {
                        spawns.insert(spawn.id, spawn);
                    }
                    None => skipped += 1,
                }
            } else if let Some(values) = insert_values(line, "cq_passway") {
                match Row::new(&PASSWAY_COLUMNS, &values) {
                    Some(row) => {
                        passways.insert(
                            (row.number("mapid"), row.number("passway_idx")),
                            (row.number("passway_mapid"), row.number("passway_mapportal")),
                        );
                    }
                    None => skipped += 1,
                }
            } else if let Some(values) = insert_values(line, "cq_portal") {
                match Row::new(&PORTAL_COLUMNS, &values) {
                    Some(row) => {
                        portals.insert(
                            (row.number("mapid"), row.number("portal_idx")),
                            (row.number("portal_x"), row.number("portal_y")),
                        );
                    }
                    None => skipped += 1,
                }
            }
        }
        // NPC ids are one space in the engine: a dynamic NPC sharing a static one's id takes
        // a free id, its own kept for export.
        for (id, mut npc) in dynamic {
            if npcs.contains_key(&id) {
                npc.extra.insert(TQ_ID.into(), id.to_string());
                npc.id = id + DYNAMIC_ID_OFFSET;
            }
            npcs.insert(npc.id, npc);
        }
        let mut maps: HashMap<u32, Population> = HashMap::new();
        for npc in npcs.into_values() {
            maps.entry(map_id(&npc.extra)).or_default().npcs.push(npc);
        }
        for spawn in spawns.into_values() {
            maps.entry(map_id(&spawn.extra))
                .or_default()
                .spawns
                .push(spawn);
        }
        // A passway leads to a portal of another map: its cell is in `cq_portal`.
        for ((map, passway), (to_map, to_portal)) in passways {
            if let Some(&cell) = portals.get(&(to_map, to_portal)) {
                maps.entry(map).or_default().links.push(PortalLink {
                    portal: passway,
                    to: PortalTarget { map: to_map, cell },
                });
            }
        }
        Self { maps, skipped }
    }

    /// NPCs and spawns of the maps `ids` together (maps that share one map file).
    pub fn for_maps(&self, ids: &[u32]) -> Population {
        let mut out = Population::default();
        for id in ids {
            if let Some(p) = self.maps.get(id) {
                out.npcs.extend(p.npcs.iter().cloned());
                out.spawns.extend(p.spawns.iter().cloned());
                out.links.extend(p.links.iter().copied());
            }
        }
        out
    }
}

/// The game map id of a row (0 if it has none).
pub fn map_id(extra: &BTreeMap<String, String>) -> u32 {
    extra.get(MAP_ID).and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// SQL that writes `population` into a TQ/Redux database: one `REPLACE` per row, so running
/// it again updates the same rows. Rows without a map id get `default_map`.
pub fn population_sql(population: &Population, default_map: u32) -> String {
    let mut out = String::new();
    for npc in &population.npcs {
        let (table, columns) = match npc.extra.contains_key(DYNAMIC) {
            true => ("cq_dynanpc", &DYNANPC_COLUMNS[..]),
            false => ("cq_npc", &NPC_COLUMNS[..]),
        };
        let values = npc_row(npc, columns, default_map);
        out.push_str(&replace_statement(table, columns, &values));
    }
    for spawn in &population.spawns {
        let values = spawn_row(spawn, default_map);
        out.push_str(&replace_statement(
            "cq_generator",
            &GENERATOR_COLUMNS,
            &values,
        ));
    }
    out
}

fn npc_from_row(columns: &[&str], values: &[Value]) -> Option<Npc> {
    let row = Row::new(columns, values)?;
    let lookface: u32 = row.number("lookface");
    Some(Npc {
        id: row.number("id"),
        name: row.text("name"),
        cell: (row.number("cellx"), row.number("celly")),
        direction: (lookface % 10) as u8,
        look: lookface / 10,
        kind: row.number("type"),
        script: String::new(),
        extra: row.extra(&NPC_OWN),
    })
}

fn spawn_from_row(values: &[Value]) -> Option<Spawn> {
    let row = Row::new(&GENERATOR_COLUMNS, values)?;
    let min: (i32, i32) = (row.number("bound_x"), row.number("bound_y"));
    let size: (i32, i32) = (row.number("bound_cx"), row.number("bound_cy"));
    Some(Spawn {
        id: row.number("id"),
        monster: row.number("npctype"),
        min,
        max: (min.0 + (size.0 - 1).max(0), min.1 + (size.1 - 1).max(0)),
        max_alive: row.number("maxnpc"),
        respawn_secs: row.number("rest_secs"),
        per_wave: row.number("max_per_gen"),
        extra: row.extra(&GENERATOR_OWN),
    })
}

fn npc_row(npc: &Npc, columns: &[&str], default_map: u32) -> Vec<Value> {
    columns
        .iter()
        .map(|&column| match column {
            "id" => Some(
                npc.extra
                    .get(TQ_ID)
                    .cloned()
                    .unwrap_or_else(|| npc.id.to_string()),
            ),
            "name" => Some(npc.name.clone()),
            "type" => Some(npc.kind.to_string()),
            "lookface" => Some((npc.look * 10 + u32::from(npc.direction % 10)).to_string()),
            "cellx" => Some(npc.cell.0.to_string()),
            "celly" => Some(npc.cell.1.to_string()),
            _ => extra_value(&npc.extra, column, default_map),
        })
        .collect()
}

fn spawn_row(spawn: &Spawn, default_map: u32) -> Vec<Value> {
    GENERATOR_COLUMNS
        .iter()
        .map(|&column| match column {
            "id" => Some(spawn.id.to_string()),
            "bound_x" => Some(spawn.min.0.to_string()),
            "bound_y" => Some(spawn.min.1.to_string()),
            "bound_cx" => Some((spawn.max.0 - spawn.min.0 + 1).to_string()),
            "bound_cy" => Some((spawn.max.1 - spawn.min.1 + 1).to_string()),
            "maxnpc" => Some(spawn.max_alive.to_string()),
            "rest_secs" => Some(spawn.respawn_secs.to_string()),
            "max_per_gen" => Some(spawn.per_wave.to_string()),
            "npctype" => Some(spawn.monster.to_string()),
            _ => extra_value(&spawn.extra, column, default_map),
        })
        .collect()
}

/// A pass-through column: its kept value, the map id, or the column's neutral default.
fn extra_value(extra: &BTreeMap<String, String>, column: &str, default_map: u32) -> Value {
    if let Some(v) = extra.get(column) {
        return (v != NULL_MARK).then(|| v.clone());
    }
    Some(match column {
        MAP_ID => default_map.to_string(),
        "idxserver" => "-1".to_owned(),
        "datastr" => String::new(),
        "itemid" => return None,
        _ => "0".to_owned(),
    })
}

/// How a kept `NULL` is written in `extra`.
const NULL_MARK: &str = "NULL";

struct Row<'a> {
    columns: &'a [&'a str],
    values: &'a [Value],
}

impl<'a> Row<'a> {
    fn new(columns: &'a [&'a str], values: &'a [Value]) -> Option<Self> {
        (columns.len() == values.len()).then_some(Self { columns, values })
    }

    fn get(&self, column: &str) -> Option<&str> {
        let i = self.columns.iter().position(|c| *c == column)?;
        self.values[i].as_deref()
    }

    fn text(&self, column: &str) -> String {
        self.get(column).unwrap_or_default().to_owned()
    }

    /// A numeric column (zero-filled in TQ's dumps); 0 if missing or not a number.
    fn number<T: std::str::FromStr + Default>(&self, column: &str) -> T {
        self.get(column)
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or_default()
    }

    /// Every column but `own`, by name: numbers written plainly (TQ dumps zero-fill them,
    /// `'0042'`), text as is, `NULL` as [`NULL_MARK`].
    fn extra(&self, own: &[&str]) -> BTreeMap<String, String> {
        self.columns
            .iter()
            .zip(self.values)
            .filter(|(c, _)| !own.contains(c))
            .map(|(c, v)| {
                let value = match v.as_deref() {
                    None => NULL_MARK.to_owned(),
                    Some(raw) => raw
                        .parse::<i64>()
                        .map_or_else(|_| raw.to_owned(), |n| n.to_string()),
                };
                (c.to_string(), value)
            })
            .collect()
    }
}

/// The values of `INSERT INTO `table` VALUES (...);` if `line` is one.
pub(crate) fn insert_values(line: &str, table: &str) -> Option<Vec<Value>> {
    let rest = line
        .trim_start()
        .strip_prefix("INSERT INTO `")?
        .strip_prefix(table)?
        .strip_prefix("` VALUES (")?;
    c3_data::sql::parse_values(rest).map(|(values, _)| values)
}

fn replace_statement(table: &str, columns: &[&str], values: &[Value]) -> String {
    let columns: Vec<String> = columns.iter().map(|c| format!("`{c}`")).collect();
    let values: Vec<String> = values
        .iter()
        .map(|v| match v {
            None => "NULL".to_owned(),
            Some(v) => format!("'{}'", v.replace('\\', "\\\\").replace('\'', "\\'")),
        })
        .collect();
    format!(
        "REPLACE INTO `{table}` ({}) VALUES ({});\n",
        columns.join(", "),
        values.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passways_lead_to_portal_cells() {
        let dump = "INSERT INTO `cq_passway` VALUES ('0001', '1002', '0003', '1011', '0000');
                    INSERT INTO `cq_portal` VALUES ('0007', '1011', '0000', '0193', '0266');
                    INSERT INTO `cq_passway` VALUES ('0002', '1002', '0004', '1999', '0000');";
        let read = ServerPopulation::from_sql(dump);
        let links = &read.maps[&1002].links;
        // The second passway's portal is missing: no link.
        assert_eq!(
            links,
            &[PortalLink {
                portal: 3,
                to: PortalTarget {
                    map: 1011,
                    cell: (193, 266)
                }
            }]
        );
        assert_eq!(read.for_maps(&[1002]).links.len(), 1);
    }

    const DUMP: &str = r#"
INSERT INTO `cq_npc` VALUES ('0008', '0', '0', 'Warehouseman', '0003', '0080', '-1', '1002', '0409', '0351', '8003000', '0000', '0000', '0000', '0000', '0000', '0000', '0000', '0', '0', '0', '0', 'None', '0000', '00', '00', '0000', '01', null);
INSERT INTO `cq_npc` VALUES ('0009', '0', '0', 'Mr.O\'Neil', '0001', '0036', '-1', '1000', '0511', '0630', '0000', '0000', '0000', '0000', '0000', '0000', '0000', '0000', '0', '0', '0', '0', 'None', '0000', '00', '00', '0000', '00', null);
INSERT INTO `cq_generator` VALUES ('0001', '1002', '0416', '0460', '0130', '0030', '0013', '0001', '0020', '0001', '0000', '0000', '0000', '0000');
INSERT INTO `cq_other` VALUES ('1');
INSERT INTO `cq_generator` VALUES ('0002', '1002');
"#;

    #[test]
    fn reads_npcs_and_generators_by_map() {
        let server = ServerPopulation::from_sql(DUMP);
        assert_eq!(server.skipped, 1);
        let tc = &server.maps[&1002];
        let npc = &tc.npcs[0];
        assert_eq!(
            (
                npc.id,
                npc.name.as_str(),
                npc.cell,
                npc.look,
                npc.direction,
                npc.kind
            ),
            (8, "Warehouseman", (409, 351), 8, 0, 3)
        );
        assert_eq!(npc.extra["task0"], "8003000");
        assert_eq!(npc.extra["itemid"], NULL_MARK);
        let spawn = &tc.spawns[0];
        assert_eq!(
            (spawn.min, spawn.max, spawn.max_alive, spawn.monster),
            ((416, 460), (545, 489), 13, 1)
        );
        let market = &server.maps[&1000].npcs[0];
        assert_eq!((market.name.as_str(), market.direction), ("Mr.O'Neil", 6));
        assert_eq!(server.for_maps(&[1000, 1002]).npcs.len(), 2);
    }

    #[test]
    fn exported_rows_read_back_the_same() {
        let server = ServerPopulation::from_sql(DUMP);
        let tc = &server.maps[&1002];
        let sql = population_sql(tc, 9999);
        assert!(
            sql.starts_with("REPLACE INTO `cq_npc` (`id`, `ownerid`"),
            "{sql}"
        );
        assert!(
            sql.contains("'Warehouseman'") && sql.contains("NULL);"),
            "{sql}"
        );
        let reread: Vec<String> = sql
            .lines()
            .map(|l| {
                // Back to the dump's form, then through the reader.
                let (_, values) = l.split_once(" VALUES ").unwrap();
                let table = if l.contains("cq_npc") {
                    "cq_npc"
                } else {
                    "cq_generator"
                };
                format!("INSERT INTO `{table}` VALUES {values}")
            })
            .collect();
        let again = ServerPopulation::from_sql(&reread.join("\n"));
        assert_eq!(&again.maps[&1002], tc);
        let mut fresh = tc.npcs[0].clone();
        fresh.extra.clear();
        let sql = population_sql(
            &Population {
                npcs: vec![fresh],
                ..Population::default()
            },
            1015,
        );
        assert!(sql.contains("'-1', '1015'"), "{sql}");
    }

    #[test]
    fn reads_the_tq_database_dump() {
        // TQ's own data; skipped where it isn't available.
        let path = r"D:\Conquer2\Tools\eosource\tq_re\db\cq_finalized_mariadb.sql";
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let server = ServerPopulation::from_sql(&String::from_utf8_lossy(&bytes));
        assert_eq!(server.skipped, 0);
        let tc = &server.maps[&1002];
        println!(
            "Twin City: {} NPCs, {} spawns",
            tc.npcs.len(),
            tc.spawns.len()
        );
        assert!(tc.npcs.len() > 20 && tc.spawns.len() > 5);
    }
}
