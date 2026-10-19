//! Conquer's game data as the engine's: TQ's tables (`cq_monstertype`, `cq_itemtype`,
//! `cq_levexp`, `cq_goods`, `cq_point_allot`, `cq_magictype`) read into the engine's rule
//! tables (`monsters`, `items`, `levels`, `shops`, `skills`, `statuses`, and `points`: the
//! attribute points each class has at each level), so a Conquer project plays on the engine's
//! data rules with its formulas as a blueprint ([`crate::rule_blueprint`]). Nothing here runs
//! while playing; it is what the import writes.

use c3_data::{Column, ColumnKind, Table, Value};
use std::collections::BTreeMap;

/// TQ's monster types table (`cq_monstertype`).
pub const MONSTER_TABLE: &str = "cq_monstertype";
/// The engine table of attribute points by class and level (key: class × 1000 + level).
pub const POINTS_TABLE: &str = "points";
/// The engine table of rebirths (key: profession × 100000 + new profession × 10 + rule).
pub const REBIRTHS_TABLE: &str = "rebirths";

/// `attack_user` bit: attacks players on sight (`ATKUSER_ACTIVE`).
const ACTIVE: i64 = 0x02;
/// `attack_user` bits of monsters that hunt players by their name colour (guard, police,
/// evil killer).
const GUARD_LIKE: i64 = 0x8 | 0x10 | 0x800;
/// `cq_magictype.sort` of healing magic (`MAGICSORT_RECRUIT`).
const SORT_HEAL: i64 = 2;
/// `cq_magictype.sort` of magic that puts a status on (`MAGICSORT_ATTACHSTATUS`).
const SORT_STATUS: i64 = 6;
/// `cq_magictype.status` flags (TQ's `KEEPEFFECT_*`) and the stat each raises.
const STATUS_ACCURACY: i64 = 0x20;
const STATUS_SHIELD: i64 = 0x40;
const STATUS_SUPERMAN: i64 = 0x2000;
const STATUS_CYCLONE: i64 = 0x40000;
/// A `cq_magictype.power` above this is a physical skill: that much over, in percent of a hit.
const PERCENT_POWER: i64 = 30000;
/// Chances (percent) a dead monster drops its life and mana potions.
const LIFE_POTION_CHANCE: i64 = 8;
const MANA_POTION_CHANCE: i64 = 4;

/// Equipment slots (TQ's `ITEMPOSITION_*`).
pub mod slot {
    pub const HELMET: u8 = 1;
    pub const NECKLACE: u8 = 2;
    pub const ARMOR: u8 = 3;
    pub const RIGHT_HAND: u8 = 4;
    pub const LEFT_HAND: u8 = 5;
    pub const RING: u8 = 6;
    pub const BOOTS: u8 = 8;
}

/// The slots shown in the bag, with their names.
pub const SLOT_NAMES: &[(u8, &str)] = &[
    (slot::HELMET, "Helmet"),
    (slot::NECKLACE, "Necklace"),
    (slot::ARMOR, "Armor"),
    (slot::RIGHT_HAND, "Weapon"),
    (slot::LEFT_HAND, "Shield"),
    (slot::RING, "Ring"),
    (slot::BOOTS, "Boots"),
];

/// Where item type `id` is worn, from its type number (TQ item sorts: 11xxxx helmets,
/// 12xxxx necklaces, 13xxxx armor, 15xxxx rings, 16xxxx boots, 4xxxxx/5xxxxx weapons,
/// 9xxxxx shields, 105xxxx arrows).
pub fn slot(id: u32) -> Option<u8> {
    match id / 10_000 {
        11 => Some(slot::HELMET),
        12 => Some(slot::NECKLACE),
        13 => Some(slot::ARMOR),
        15 => Some(slot::RING),
        16 => Some(slot::BOOTS),
        40..=59 => Some(slot::RIGHT_HAND),
        90..=99 => Some(slot::LEFT_HAND),
        105 => Some(slot::LEFT_HAND),
        _ => None,
    }
}

/// A row of a TQ table, read by column name; a missing column reads as nothing.
struct TqRow<'a> {
    table: &'a Table,
    values: &'a [Value],
}

impl TqRow<'_> {
    fn int(&self, column: &str) -> i64 {
        (self.table.column(column))
            .and_then(|i| self.values.get(i))
            .and_then(Value::as_int)
            .unwrap_or(0)
    }

    fn text(&self, column: &str) -> String {
        (self.table.column(column))
            .and_then(|i| self.values.get(i))
            .and_then(Value::as_text)
            .unwrap_or_default()
            .to_owned()
    }
}

/// Every row of `table`.
fn rows(table: &Table) -> impl Iterator<Item = TqRow<'_>> {
    (table.rows.iter()).map(move |values| TqRow { table, values })
}

/// An engine table being filled, its rows given by column name.
struct Out {
    table: Table,
}

impl Out {
    /// The engine's template of table `name` (its columns), with `extra` columns after.
    fn new(name: &str, extra: &[(&str, ColumnKind, &str)]) -> Self {
        let mut table = (c3_play::data_rules::table_templates().into_iter())
            .find(|t| t.name == name)
            .unwrap_or_else(|| Table::new(name, Vec::new()));
        table.rows.clear();
        for (column, kind, doc) in extra {
            table.columns.push(Column {
                doc: (*doc).into(),
                ..Column::new(*column, *kind)
            });
        }
        Self { table }
    }

    /// Adds a row of `values` by column name; columns not named get their empty value.
    fn row(&mut self, values: &[(&str, Value)]) {
        let row = (self.table.columns.iter())
            .map(|c| {
                (values.iter())
                    .find(|(name, _)| *name == c.name)
                    .map_or_else(|| c.kind.default_value(), |(_, v)| v.clone())
            })
            .collect();
        self.table.rows.push(row);
    }
}

fn int(v: i64) -> Value {
    Value::Int(v)
}

fn text(v: impl Into<String>) -> Value {
    Value::Text(v.into())
}

/// Makes an engine table of a TQ one.
type TableMaker<'a> = dyn Fn(&Table) -> Table + 'a;

/// The engine's rule tables made of TQ's `tables`, and what couldn't be made.
pub fn engine_tables(tables: &[Table]) -> (Vec<Table>, Vec<String>) {
    let find = |name: &str| tables.iter().find(|t| t.name == name);
    let mut out = Vec::new();
    let mut problems = Vec::new();
    let mut missing = |name: &str| problems.push(format!("Rules: the dump has no {name}"));
    let (skills, statuses, magic_types) = match find("cq_magictype") {
        Some(t) => skills_and_statuses(t),
        None => {
            missing("cq_magictype");
            (
                Out::new(c3_play::data_rules::SKILLS, &[]).table,
                Out::new(c3_play::data_rules::STATUSES, &[]).table,
                BTreeMap::new(),
            )
        }
    };
    out.extend([skills, statuses]);
    let makers: [(&str, &TableMaker); 6] = [
        (MONSTER_TABLE, &|t| monsters(t, &magic_types)),
        (crate::server_data::ITEM_TABLE, &items),
        ("cq_levexp", &levels),
        ("cq_goods", &shops),
        ("cq_point_allot", &points),
        ("cq_rebirth", &rebirths),
    ];
    for (name, make) in makers {
        match find(name) {
            Some(t) => out.push(make(t)),
            None => missing(name),
        }
    }
    (out, problems)
}

/// Magic types at their first level (`level` 0) as skills, the statuses attach-status magic
/// puts on, and each magic level's id → its type (monsters name magic levels).
fn skills_and_statuses(t: &Table) -> (Table, Table, BTreeMap<i64, i64>) {
    let mut skills = Out::new(c3_play::data_rules::SKILLS, &[]);
    let mut statuses = Out::new(c3_play::data_rules::STATUSES, &[]);
    let magic_types = rows(t).map(|r| (r.int("id"), r.int("type"))).collect();
    for r in rows(t).filter(|r| r.int("level") == 0) {
        let id = r.int("type");
        let name = r.text("name");
        let (kind, status) = match r.int("sort") {
            SORT_HEAL => ("heal", 0),
            SORT_STATUS => match status(&mut statuses, id, &name, &r) {
                Some(status) => ("status", status),
                None => ("damage", 0),
            },
            _ => ("damage", 0),
        };
        let target = match kind {
            "heal" | "status" => "self",
            _ if r.int("ground") != 0 => "ground",
            _ => "enemy",
        };
        // XP skills need the full XP bar (battle M.11); others may cost stamina.
        let cost = match (r.int("use_xp"), r.int("use_ep")) {
            (xp, _) if xp != 0 => ("xp", 100),
            (_, ep) if ep > 0 => ("stamina", ep),
            _ => ("", 0),
        };
        let area = if r.int("multi") != 0 {
            r.int("range")
        } else {
            0
        };
        // A negative power (Restore's -32768) heals fully.
        let power = if r.int("power") < 0 {
            i64::from(u32::MAX)
        } else {
            r.int("power")
        };
        skills.row(&[
            ("id", int(id)),
            ("name", text(name)),
            ("target", text(target)),
            ("kind", text(kind)),
            ("range", int(r.int("distance").max(1))),
            ("area", int(area)),
            ("cooldown_ms", int(r.int("delay_ms").max(500))),
            ("mana", int(r.int("use_mp"))),
            ("power", int(power)),
            ("level", int(r.int("need_level"))),
            ("status", int(status)),
            ("cost_attribute", text(cost.0)),
            ("cost", int(cost.1)),
        ]);
    }
    (skills.table, statuses.table, magic_types)
}

/// The status attach-status magic `id` puts on (its `status` flag says which stat; its power
/// is 30000 + the stat's new percent; `step_secs` how long); `None` for a flag not modelled.
fn status(out: &mut Out, id: i64, name: &str, r: &TqRow) -> Option<i64> {
    let change = (r.int("power") - PERCENT_POWER - 100).clamp(-100, 10_000);
    let (column, change) = match r.int("status") {
        STATUS_ACCURACY => ("dodge_percent", change),
        STATUS_SHIELD => ("defence_percent", change),
        STATUS_SUPERMAN => ("attack_percent", change),
        // Cyclone's power is its speed gain itself.
        STATUS_CYCLONE => ("speed_percent", change + 100),
        _ => return None,
    };
    out.row(&[
        ("id", int(id)),
        ("name", text(name)),
        ("duration_ms", int(r.int("step_secs") * 1000)),
        ("tick_ms", int(1000)),
        (column, int(change)),
    ]);
    Some(id)
}

fn monsters(t: &Table, magic_types: &BTreeMap<i64, i64>) -> Table {
    let extra = [
        (
            "attack_user",
            ColumnKind::Int,
            "TQ's behaviour bits (2 active, 8 guard, 16 police...)",
        ),
        (
            "defence2",
            ColumnKind::Int,
            "Damage it takes, in ten-thousandths (10000: all)",
        ),
    ];
    let mut out = Out::new(c3_play::data_rules::MONSTERS, &extra);
    for r in rows(t) {
        let attack_user = r.int("attack_user");
        let potions = [
            ("drop_hp", LIFE_POTION_CHANCE),
            ("drop_mp", MANA_POTION_CHANCE),
        ];
        let drops: Vec<String> = (potions.iter())
            .filter(|(column, _)| r.int(column) != 0)
            .map(|(column, chance)| format!("{}:{chance}", r.int(column)))
            .collect();
        let life = r.int("life").max(1);
        let skill = magic_types.get(&r.int("magic_type")).copied().unwrap_or(0);
        out.row(&[
            ("id", int(r.int("id"))),
            ("name", text(r.text("name"))),
            ("look", int(r.int("lookface"))),
            ("level", int(r.int("level"))),
            ("life", int(life)),
            ("attack_min", int(r.int("attack_min"))),
            (
                "attack_max",
                int(r.int("attack_max").max(r.int("attack_min"))),
            ),
            ("defence", int(r.int("defence"))),
            ("dodge", int(r.int("dodge"))),
            ("dexterity", int(r.int("dexterity"))),
            ("attack_range", int(r.int("attack_range").max(1))),
            ("attack_ms", int(r.int("attack_speed").max(300))),
            ("step_ms", int(r.int("move_speed").max(200))),
            ("view_range", int(r.int("view_range"))),
            // Guards and police start fights only with players of a bad name.
            (
                "aggressive",
                Value::Bool(attack_user & ACTIVE != 0 && attack_user & GUARD_LIKE == 0),
            ),
            ("drop_money", int(r.int("drop_money"))),
            ("drops", text(drops.join(", "))),
            (
                "flee_percent",
                int((r.int("escape_life") * 100 / life).min(100)),
            ),
            ("skill", int(skill)),
            ("skill_percent", int(r.int("magic_hitrate").min(100))),
            ("hunts", text(hunts(attack_user))),
            ("attack_user", int(attack_user)),
            ("defence2", int(r.int("defence2"))),
        ]);
    }
    out.table
}

/// Whom a monster with TQ's `attack_user` bits hunts by their name (battle §4.3): guards
/// criminals, police black names, evil killers players of good name.
fn hunts(attack_user: i64) -> String {
    let kinds = [(0x8, "criminal"), (0x10, "black"), (0x800, "good")];
    let names: Vec<&str> = (kinds.iter())
        .filter(|(bit, _)| attack_user & bit != 0)
        .map(|(_, name)| *name)
        .collect();
    names.join(", ")
}

/// TQ's item columns kept as the game's facts (requirements and weapon numbers).
const ITEM_FACTS: [(&str, &str); 11] = [
    (
        "req_profession",
        "The profession needed (TQ: class × 10 + its step)",
    ),
    ("req_weaponskill", "The weapon skill level needed"),
    ("req_sex", "The sex needed (1 male, 2 female; 0 any)"),
    ("req_force", "Strength needed"),
    ("req_speed", "Agility needed"),
    ("req_health", "Vitality needed"),
    ("req_soul", "Spirit needed"),
    ("atk_speed", "Milliseconds between a weapon's hits"),
    ("atk_range", "A weapon's reach in cells"),
    ("magic_atk", "Magic attack"),
    ("magic_def", "Magic defence"),
];

fn items(t: &Table) -> Table {
    let extra: Vec<(&str, ColumnKind, &str)> = (ITEM_FACTS.iter())
        .map(|(column, doc)| (*column, ColumnKind::Int, *doc))
        .collect();
    let mut out = Out::new(c3_play::data_rules::ITEMS, &extra);
    for r in rows(t) {
        let id = r.int("id");
        let slot = slot(u32::try_from(id).unwrap_or(0));
        // A price in CPs when there is no money price.
        let (price, currency) = match (r.int("price"), r.int("emoney_price")) {
            (0, cps) if cps > 0 => (cps, crate::module::CONQUER_POINTS),
            (price, _) => (price, ""),
        };
        // Worn items' amount limit is their durability (in hundredths); arrows' how many
        // stack.
        let limit = r.int("amount_limit");
        let (durability, stack) = match slot {
            Some(_) if id / 10_000 == 105 => (0, limit),
            Some(_) => (limit / 100, 0),
            None => (0, 0),
        };
        let mut values = vec![
            ("id", int(id)),
            ("name", text(r.text("name"))),
            ("price", int(price)),
            ("slot", int(slot.map_or(0, i64::from))),
            ("life", int(r.int("life"))),
            ("mana", int(r.int("mana"))),
            ("attack_min", int(r.int("attack_min"))),
            ("attack_max", int(r.int("attack_max"))),
            ("defence", int(r.int("defense"))),
            ("dodge", int(r.int("dodge"))),
            ("dexterity", int(r.int("dexterity"))),
            ("stack", int(stack)),
            ("durability", int(durability)),
            ("currency", text(currency)),
            ("requires_level", int(r.int("req_level"))),
        ];
        values.extend(
            ITEM_FACTS
                .iter()
                .map(|(column, _)| (*column, int(r.int(column)))),
        );
        out.row(&values);
    }
    out.table
}

fn levels(t: &Table) -> Table {
    let mut out = Out::new(c3_play::data_rules::LEVELS, &[]);
    let mut levels: Vec<(i64, i64)> = rows(t).map(|r| (r.int("level"), r.int("exp"))).collect();
    levels.sort_unstable();
    for (level, exp) in levels {
        out.row(&[("level", int(level)), ("exp", int(exp))]);
    }
    out.table
}

fn shops(t: &Table) -> Table {
    let mut out = Out::new(c3_play::data_rules::SHOPS, &[]);
    for (i, r) in rows(t).enumerate() {
        out.row(&[
            ("id", int(i as i64 + 1)),
            ("npc", int(r.int("ownerid"))),
            ("item", int(r.int("itemtype"))),
        ]);
    }
    out.table
}

/// Who may be reborn as what (`cq_rebirth`), keyed profession × 100000 + new profession × 10
/// + rule (the rebirth's number): the level needed, and the level after.
fn rebirths(t: &Table) -> Table {
    let column = |name: &str, doc: &str| Column {
        doc: doc.into(),
        ..Column::new(name, ColumnKind::Int)
    };
    let mut out = Table::new(
        REBIRTHS_TABLE,
        vec![
            column(
                "key",
                "Profession × 100000 + new profession × 10 + rule (1st or 2nd)",
            ),
            column("need_level", "The level needed"),
            column("new_level", "The level after"),
        ],
    );
    out.doc = "Rebirths: who may be reborn as what (TQ's cq_rebirth)".into();
    for r in rows(t) {
        let key = r.int("need_prof") * 100_000 + r.int("new_prof") * 10 + r.int("metepsychosis");
        out.rows.push(vec![
            int(key),
            int(r.int("need_level")),
            int(r.int("new_level")),
        ]);
    }
    out
}

/// The points each class has at each level (`cq_point_allot`), keyed class × 1000 + level.
fn points(t: &Table) -> Table {
    let column = |name: &str, doc: &str| Column {
        doc: doc.into(),
        ..Column::new(name, ColumnKind::Int)
    };
    let mut out = Table::new(
        POINTS_TABLE,
        vec![
            column(
                "key",
                "Class × 1000 + level (class: 1 Trojan, 2 Warrior, 4 Archer, 10 Taoist)",
            ),
            column("strength", "Strength at that level"),
            column("agility", "Agility"),
            column("vitality", "Vitality"),
            column("spirit", "Spirit"),
        ],
    );
    out.doc = "The attribute points each class has at each level (TQ's cq_point_allot)".into();
    for r in rows(t) {
        out.rows.push(vec![
            int(r.int("profession") * 1000 + r.int("level")),
            int(r.int("force")),
            int(r.int("Speed")),
            int(r.int("health")),
            int(r.int("soul")),
        ]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_game::Rules;

    #[test]
    fn tq_tables_become_the_engines() {
        let dump =
            std::path::Path::new(r"D:\Conquer2\Tools\eosource\tq_re\db\cq_finalized_mariadb.sql");
        let Ok(bytes) = std::fs::read(dump) else {
            return;
        };
        let sql = encoding_rs::GBK.decode(&bytes).0;
        let tq = c3_data::sql::import_tables(&sql, crate::server_data::DATA_TABLES);
        let (tables, problems) = engine_tables(&tq);
        assert!(problems.is_empty(), "{problems:?}");
        let rules = c3_play::data_rules::DataRules::from_tables(&tables);
        let blade = rules.item(410003).expect("SteelBlade");
        assert_eq!(
            (blade.slot, blade.requires_level, blade.durability),
            (Some(4), 5, 15)
        );
        assert!(rules.monster(1).is_some() && rules.skill(1000).is_some());
        let points = tables.iter().find(|t| t.name == POINTS_TABLE).unwrap();
        assert!(points.find(1001).is_some(), "Trojan level 1");
    }
}
