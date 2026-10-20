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
/// The engine table of the lottery's prizes (key: TQ's row id).
pub const LOTTERY_TABLE: &str = "lottery";

/// The engine table of TQ's level-difference factors (key: level difference, −400..400).
pub const DISDAIN_TABLE: &str = "disdain";
/// `cq_disdain`'s columns (battle §2.6, exp §2) kept in the disdain table.
pub const DISDAIN_COLUMNS: [(&str, &str); 18] = [
    (
        "usr_atk_mst",
        "A player's attack on a monster, percent more",
    ),
    ("mst_atk", "A monster's attack on a player, percent more"),
    ("max_atk", "Most damage to a monster, percent of its life"),
    ("max_xp_atk", "The same under an XP skill"),
    ("exp_factor", "Experience, percent more"),
    ("xp_exp_factor", "The same under an XP skill"),
    ("usr_atk_usr_min", "PvP floor (both under level 110)"),
    ("usr_atk_usr_max", "PvP cap (both under level 110)"),
    ("usr_atk_usr_overadj", "Percent kept above the cap"),
    ("usr_atk_usrx_min", "PvP floor (target 110 or more)"),
    ("usr_atk_usrx_max", "PvP cap (target 110 or more)"),
    ("usr_atk_usrx_overadj", "Percent kept above the cap"),
    ("usrx_atk_usr_min", "PvP floor (attacker 110 or more)"),
    ("usrx_atk_usr_max", "PvP cap (attacker 110 or more)"),
    ("usrx_atk_usr_overadj", "Percent kept above the cap"),
    ("usrx_atk_usrx_min", "PvP floor (both 110 or more)"),
    ("usrx_atk_usrx_max", "PvP cap (both 110 or more)"),
    ("usrx_atk_usrx_overadj", "Percent kept above the cap"),
];

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
/// Bless: lucky time grows, idle players near pray.
const STATUS_CAST_PRAY: i64 = 0x200_0000;
/// The status of a player praying near a Bless caster (TQ's 0x4000000).
pub const STATUS_PRAYING: i64 = TQ_STATUS + 0x400_0000;
/// Bless's magic type (its status's id).
pub const BLESS: i64 = 9876;
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
    let makers: [(&str, &TableMaker); 10] = [
        ("cq_magictype", &magic_levels),
        ("cq_magictype", &skill_ranks),
        (MONSTER_TABLE, &|t| monsters(t, &magic_types)),
        (crate::server_data::ITEM_TABLE, &items),
        ("cq_levexp", &levels),
        ("cq_goods", &shops),
        ("cq_point_allot", &points),
        ("cq_rebirth", &rebirths),
        ("cq_disdain", &disdain),
        ("cq_lottery", &lottery),
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
    let extra = [
        ("superman", ColumnKind::Int, "1: TQ's Superman (XP)"),
        ("cyclone", ColumnKind::Int, "1: TQ's Cyclone (XP)"),
        (
            "cast_pray",
            ColumnKind::Int,
            "1: TQ's Bless (lucky time, others pray)",
        ),
        ("attack_add", ColumnKind::Int, "Attack added"),
        ("defence_add", ColumnKind::Int, "Defence added"),
        (
            "client_bit",
            ColumnKind::Int,
            "The Conquer client's effect bit",
        ),
    ];
    let mut statuses = Out::new(c3_play::data_rules::STATUSES, &extra);
    for (bit, name, attack, defence, client) in TQ_STATUSES {
        statuses.row(&[
            ("id", int(TQ_STATUS + bit)),
            ("name", text(name)),
            ("duration_ms", int(60_000)),
            ("tick_ms", int(1000)),
            ("attack_add", int(attack)),
            ("defence_add", int(defence)),
            ("client_bit", int(client)),
        ]);
    }
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
        let mut row = vec![
            ("id", int(id)),
            ("name", text(name)),
            ("target", text(target)),
            ("kind", text(kind)),
            ("level", int(r.int("need_level"))),
            ("status", int(status)),
            ("cost_attribute", text(cost.0)),
            ("cost", int(cost.1)),
            ("rank_attribute", text(format!("{MAGIC_LEVEL_PREFIX}{id}"))),
        ];
        row.extend(magic_numbers(&r));
        skills.row(&row);
    }
    (skills.table, statuses.table, magic_types)
}

/// Statuses TQ's actions put on by their bit (1082: `self add <bit> <power> <secs>`), each
/// a status `TQ_STATUS + bit`: its name, what it adds (TQ's power 200, added), and the
/// client's effect bit (battle spec S3).
pub const TQ_STATUS: i64 = 1_000_000_000;
const TQ_STATUSES: [(i64, &str, i64, i64, i64); 5] = [
    (0x4, "Poison", 0, 0, 0x2),
    (0x40, "Defence+", 0, 200, 0x100),
    (0x80, "Attack+", 200, 0, 0x200),
    (0x20000, "Invisible", 0, 0, 0x40_0000),
    (0x400_0000, "Praying", 0, 0, 0x8000_0000),
];

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
        STATUS_CAST_PRAY => ("cast_pray", 1),
        _ => return None,
    };
    let flag = r.int("status");
    out.row(&[
        ("id", int(id)),
        ("name", text(name)),
        ("duration_ms", int(r.int("step_secs") * 1000)),
        ("tick_ms", int(1000)),
        (column, int(change)),
        ("superman", int(i64::from(flag == STATUS_SUPERMAN))),
        ("cyclone", int(i64::from(flag == STATUS_CYCLONE))),
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
        (
            "armor_type",
            ColumnKind::Int,
            "The armour it wears (the minimum damage)",
        ),
        ("magic_def", ColumnKind::Int, "Magic defence, percent"),
        (
            "anti_monster",
            ColumnKind::Int,
            "Its kind items may be made against",
        ),
        (
            "extra_battlelev",
            ColumnKind::Int,
            "Battle power a player needs against it",
        ),
        (
            "extra_exp",
            ColumnKind::Int,
            "Experience percent per battle power over that (left 0: the dump's 7000 is wrong)",
        ),
        (
            "extra_damage",
            ColumnKind::Int,
            "Its hits' least damage, in millionths of life",
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
            ("armor_type", int(r.int("armor_type"))),
            ("magic_def", int(r.int("magic_def"))),
            ("anti_monster", int(r.int("anti_monster"))),
            ("extra_battlelev", int(r.int("extra_battlelev"))),
            // Battle exp §2: every row's 7000 would give 70% per battle power point.
            ("extra_exp", int(0)),
            ("extra_damage", int(r.int("extra_damage"))),
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

/// What a gem gives in a socket, by its kind (`type / 10 % 10` of 7000xx) and grade (the last
/// digit: 1 normal, 2 refined, 3 super): its items-table column and the three values
/// (battle §1 gem tables, items §4.1; Moon's from the 5065 client's gem texts).
const GEMS: [(i64, &str, &str, [i64; 3]); 8] = [
    (
        0,
        "gem_magic",
        "Magic damage, percent (Phoenix)",
        [5, 10, 15],
    ),
    (
        1,
        "gem_attack",
        "Physical attack, percent (Dragon)",
        [5, 10, 15],
    ),
    (
        2,
        "gem_dexterity",
        "Added to the dexterity sum (Fury)",
        [5, 10, 15],
    ),
    (3, "gem_exp", "Experience, percent (Rainbow)", [10, 15, 25]),
    (
        4,
        "gem_durability",
        "Most durability, percent (Kylin)",
        [50, 100, 200],
    ),
    (
        5,
        "gem_skill_exp",
        "Weapon skill experience, percent (Violet)",
        [30, 50, 100],
    ),
    (
        6,
        "gem_magic_exp",
        "Magic skill experience, percent (Moon)",
        [15, 30, 50],
    ),
    (
        7,
        "gem_defence",
        "Damage taken, percent of half (Tortoise)",
        [4, 8, 12],
    ),
];

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

/// An item's CP price beside its silver one (`emoney_price`), for the CP shops.
const CP_PRICE: &str = "price_CPs";

fn items(t: &Table) -> Table {
    let extra: Vec<(&str, ColumnKind, &str)> = (ITEM_FACTS.iter())
        .map(|(column, doc)| (*column, ColumnKind::Int, *doc))
        .chain([(CP_PRICE, ColumnKind::Int, "Its price in CPs, in a CP shop")])
        .chain(
            GEMS.iter()
                .map(|(_, column, doc, _)| (*column, ColumnKind::Int, *doc)),
        )
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
            (
                "alt_slot",
                int(if id / 100_000 == 4 {
                    slot::LEFT_HAND.into()
                } else {
                    0
                }),
            ),
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
        // A gem's number in its column.
        let gem = (id / 100 == 7000 && (1..=3).contains(&(id % 10)))
            .then(|| GEMS.iter().find(|(kind, ..)| *kind == id / 10 % 10))
            .flatten();
        if let Some((_, column, _, grades)) = gem {
            values.push((*column, int(grades[(id % 10 - 1) as usize])));
        }
        values.extend(
            ITEM_FACTS
                .iter()
                .map(|(column, _)| (*column, int(r.int(column)))),
        );
        values.push((CP_PRICE, int(r.int("emoney_price"))));
        out.row(&values);
    }
    out.table
}

fn levels(t: &Table) -> Table {
    let mut out = Out::new(c3_play::data_rules::LEVELS, &[]);
    // up_lev_time: 600 of it is one exp ball (progression §9.4).
    let mut levels: Vec<(i64, i64, i64)> = rows(t)
        .map(|r| (r.int("level"), r.int("exp"), r.int("up_lev_time")))
        .collect();
    levels.sort_unstable();
    for (level, exp, time) in levels {
        out.row(&[
            ("level", int(level)),
            ("exp", int(exp)),
            ("time", int(time)),
        ]);
    }
    out.table
}

/// What NPCs sell (`cq_goods`); a ware of `moneytype` 1 is sold for CPs (the VIP shop).
fn shops(t: &Table) -> Table {
    let mut out = Out::new(c3_play::data_rules::SHOPS, &[]);
    for (i, r) in rows(t).enumerate() {
        let currency = match r.int("moneytype") {
            1 => crate::module::CONQUER_POINTS,
            _ => "",
        };
        out.row(&[
            ("id", int(i as i64 + 1)),
            ("npc", int(r.int("ownerid"))),
            ("item", int(r.int("itemtype"))),
            ("currency", text(currency)),
        ]);
    }
    out.table
}

/// The lottery's prizes (`cq_lottery`): which draw, how likely, the item and how it comes.
fn lottery(t: &Table) -> Table {
    let column = |name: &str, kind, doc: &str| Column {
        doc: doc.into(),
        ..Column::new(name, kind)
    };
    let int_column = |name: &str, doc: &str| column(name, ColumnKind::Int, doc);
    let mut out = Table::new(
        LOTTERY_TABLE,
        vec![
            int_column("key", "TQ's row id"),
            int_column("draw", "Which lottery (the action's number)"),
            int_column("rank", "The prize's rank"),
            int_column("chance", "Its weight in the draw"),
            column("name", ColumnKind::Text, "What the winner is told"),
            int_column("item", "The item type won"),
            int_column("color", "Its colour"),
            int_column("sockets", "Open sockets (0-2)"),
            int_column("plus", "Its plus"),
        ],
    );
    out.doc = "The lottery's prizes (TQ's cq_lottery)".into();
    for r in rows(t) {
        out.rows.push(vec![
            int(r.int("id")),
            int(r.int("type")),
            int(r.int("rank")),
            int(r.int("chance")),
            text(r.text("prize_name")),
            int(r.int("prize_item")),
            int(r.int("color")),
            int(r.int("hole_num")),
            int(r.int("addition_lev")),
        ]);
    }
    out
}

/// TQ's level-difference factors (`cq_disdain`, type 0: the only one the data has), keyed
/// by the level difference.
fn disdain(t: &Table) -> Table {
    let column = |name: &str, doc: &str| Column {
        doc: doc.into(),
        ..Column::new(name, ColumnKind::Int)
    };
    let columns = std::iter::once(column("key", "Level difference (attacker − target)"))
        .chain(DISDAIN_COLUMNS.iter().map(|(name, doc)| column(name, doc)))
        .collect();
    let mut out = Table::new(DISDAIN_TABLE, columns);
    out.doc = "Level-difference factors of damage and experience (TQ's cq_disdain)".into();
    let mut rows: Vec<Vec<Value>> = rows(t)
        .filter(|r| r.int("type") == 0)
        .map(|r| {
            std::iter::once(int(r.int("delta_lev")))
                .chain(DISDAIN_COLUMNS.iter().map(|(name, _)| int(r.int(name))))
                .collect()
        })
        .collect();
    rows.sort_by_key(|r| r[0].as_int());
    out.rows = rows;
    out
}

/// The player attribute prefix of a magic's level (`magic_<type>`).
pub const MAGIC_LEVEL_PREFIX: &str = "magic_";

/// A magic level's numbers as the engine's skill columns: range, area, cooldown, mana and
/// power (a negative power, Restore's −32768, heals fully).
fn magic_numbers(r: &TqRow) -> [(&'static str, Value); 5] {
    let area = if r.int("multi") != 0 {
        r.int("range")
    } else {
        0
    };
    let power = if r.int("power") < 0 {
        i64::from(u32::MAX)
    } else {
        r.int("power")
    };
    [
        ("range", int(r.int("distance").max(1))),
        ("area", int(area)),
        ("cooldown_ms", int(r.int("delay_ms").max(500))),
        ("mana", int(r.int("use_mp"))),
        ("power", int(power)),
    ]
}

/// Each magic type's levels above the first as the engine's skill ranks.
fn skill_ranks(t: &Table) -> Table {
    let mut columns = vec![
        Column::new("id", ColumnKind::Int),
        Column::new("skill", ColumnKind::Int),
        Column::new("rank", ColumnKind::Int),
    ];
    columns.extend(
        ["range", "area", "cooldown_ms", "mana", "power"].map(|c| Column::new(c, ColumnKind::Int)),
    );
    let mut out = Table::new(c3_play::data_rules::SKILL_RANKS, columns);
    out.doc = "Skills at each level above the first (TQ's cq_magictype)".into();
    for r in rows(t).filter(|r| r.int("level") > 0) {
        let mut row = vec![int(r.int("id")), int(r.int("type")), int(r.int("level"))];
        row.extend(magic_numbers(&r).map(|(_, v)| v));
        out.rows.push(row);
    }
    out
}

/// The magic levels table: what each level of each magic type needs to reach the next.
pub const MAGIC_LEVELS_TABLE: &str = "magic_levels";

/// Each magic level (`cq_magictype`), keyed type × 100 + level: the experience it needs to
/// go up (0 or −1: it doesn't) and the player level needed.
fn magic_levels(t: &Table) -> Table {
    let column = |name: &str, doc: &str| Column {
        doc: doc.into(),
        ..Column::new(name, ColumnKind::Int)
    };
    let mut out = Table::new(
        MAGIC_LEVELS_TABLE,
        vec![
            column("key", "Magic type × 100 + level"),
            column(
                "need_exp",
                "Experience to go up a level (0 or −1: it doesn't)",
            ),
            column("need_level", "The player level needed to go up"),
        ],
    );
    out.doc = "Magic levels: what each needs to go up (TQ's cq_magictype)".into();
    let mut rows: Vec<Vec<Value>> = rows(t)
        .map(|r| {
            vec![
                int(r.int("type") * 100 + r.int("level")),
                int(r.int("need_exp")),
                int(r.int("need_level")),
            ]
        })
        .collect();
    rows.sort_by_key(|r| r[0].as_int());
    rows.dedup_by_key(|r| r[0].as_int());
    out.rows = rows;
    out
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
