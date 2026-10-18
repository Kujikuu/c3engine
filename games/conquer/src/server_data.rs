//! The server's map population in TQ's database schema (`cq_npc`, `cq_generator`), which Redux
//! also runs on: read from an SQL dump into engine [`Npc`]s and [`Spawn`]s, and written back
//! as SQL to load into the server's database.
//!
//! Engine fields carry what the editor shows and moves; every other column rides along in
//! `extra` under its column name, so a row exports as it was imported.

use c3_scene::{Npc, Population, Spawn};
use std::collections::{BTreeMap, HashMap};

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

/// One SQL value: `None` for `NULL`.
type Value = Option<String>;

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
        let mut spawns: BTreeMap<u32, Spawn> = BTreeMap::new();
        let mut skipped = 0;
        for line in sql.lines() {
            if let Some(values) = insert_values(line, "cq_npc") {
                match npc_from_row(&values) {
                    Some(npc) => {
                        npcs.insert(npc.id, npc);
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
            }
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
        Self { maps, skipped }
    }

    /// NPCs and spawns of the maps `ids` together (maps that share one map file).
    pub fn for_maps(&self, ids: &[u32]) -> Population {
        let mut out = Population::default();
        for id in ids {
            if let Some(p) = self.maps.get(id) {
                out.npcs.extend(p.npcs.iter().cloned());
                out.spawns.extend(p.spawns.iter().cloned());
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
        let values = npc_row(npc, default_map);
        out.push_str(&replace_statement("cq_npc", &NPC_COLUMNS, &values));
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

fn npc_from_row(values: &[Value]) -> Option<Npc> {
    let row = Row::new(&NPC_COLUMNS, values)?;
    let lookface: u32 = row.number("lookface");
    Some(Npc {
        id: row.number("id"),
        name: row.text("name"),
        cell: (row.number("cellx"), row.number("celly")),
        direction: (lookface % 10) as u8,
        look: lookface / 10,
        kind: row.number("type"),
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

fn npc_row(npc: &Npc, default_map: u32) -> Vec<Value> {
    NPC_COLUMNS
        .iter()
        .map(|&column| match column {
            "id" => Some(npc.id.to_string()),
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
fn insert_values(line: &str, table: &str) -> Option<Vec<Value>> {
    let rest = line
        .trim_start()
        .strip_prefix("INSERT INTO `")?
        .strip_prefix(table)?
        .strip_prefix("` VALUES (")?;
    parse_values(rest)
}

/// Parses `'a', 12, null, 'it\'s')...` up to the closing parenthesis.
fn parse_values(s: &str) -> Option<Vec<Value>> {
    let mut values = Vec::new();
    let mut chars = s.chars().peekable();
    loop {
        while chars.peek() == Some(&' ') {
            chars.next();
        }
        match chars.peek()? {
            '\'' => {
                chars.next();
                let mut text = String::new();
                loop {
                    match chars.next()? {
                        '\\' => text.push(chars.next()?),
                        '\'' if chars.peek() == Some(&'\'') => {
                            chars.next();
                            text.push('\'');
                        }
                        '\'' => break,
                        c => text.push(c),
                    }
                }
                values.push(Some(text));
            }
            _ => {
                let mut word = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ',' || c == ')' {
                        break;
                    }
                    word.push(c);
                    chars.next();
                }
                let word = word.trim();
                values.push((!word.eq_ignore_ascii_case("null")).then(|| word.to_owned()));
            }
        }
        while chars.peek() == Some(&' ') {
            chars.next();
        }
        match chars.next()? {
            ',' => {}
            ')' => return Some(values),
            _ => return None,
        }
    }
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
                spawns: Vec::new(),
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
