//! The engine's own game rules, made of data: a project lists its levels, monsters, items and
//! shops in tables (`tables/levels.table.ron`, …) and may change any formula in
//! `scripts/rules.rhai`; a game needs no Rust. Tables the project lacks and formulas it
//! doesn't define fall back to the engine's simple defaults, so a new project plays at once.
//!
//! Formulas (all optional) get numbers and maps of numbers:
//! - `player_stats(level)` → a map of stats (levels without a row in `levels`)
//! - `level_exp(level)` → experience to the next level (0: the top)
//! - `hit(attacker, target)` → damage, 0 for a miss; each side is a map of its stats and
//!   `level`, the attacker's has `roll` (a number between its attack_min and attack_max) and
//!   `chance` (0 to 99)
//! - `hit_exp(level, target, damage)` and `kill_exp(level, target)` → experience
//! - `skill_hit(attacker, target, skill)` → a skill's damage (`skill` holds its power,
//!   range and area; the attacker's `roll` as for `hit`), `skill_heal(healer, skill)` → life

use crate::module::GameRules;
use c3_data::{Column, ColumnKind, Table, Value};
use c3_game::{
    Drop, Fighter, ItemType, MonsterType, Rng, Rules, SkillKind, SkillTarget, SkillType, Stats,
    StatsFor, StatusType,
};
use c3_script::Formulas;
use c3_script::formulas::map_int;
use c3_script::rhai::Map;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

pub const LEVELS: &str = "levels";
pub const MONSTERS: &str = "monsters";
pub const ITEMS: &str = "items";
pub const SHOPS: &str = "shops";
pub const SKILLS: &str = "skills";
pub const STATUSES: &str = "statuses";

/// A project's first `scripts/rules.rhai`: the engine's own formulas, ready to change.
pub const FORMULAS_TEMPLATE: &str = r#"// The game's formulas. Every function is optional: take one out and the engine's own
// formula answers instead.
//
// Fighters are maps: level, life, mana, attack_min, attack_max, defence, dodge, dexterity.
// The attacker's also has roll (its attack this hit, between attack_min and attack_max)
// and chance (0 to 99).

// Damage of one hit; 0 is a miss.
fn hit(attacker, target) {
    if attacker.chance < target.dodge - attacker.dexterity {
        return 0;
    }
    let damage = attacker.roll - target.defence;
    if damage < 1 { 1 } else { damage }
}

// Experience a player at `level` earns for dealing `damage` to a monster.
fn hit_exp(level, target, damage) {
    damage
}

// Extra experience for the killing blow.
fn kill_exp(level, target) {
    target.level * 10
}

// Damage of a skill: its power and half an attack, less half the target's defence.
fn skill_hit(attacker, target, skill) {
    let damage = skill.power + attacker.roll / 2 - target.defence / 2;
    if damage < 1 { 1 } else { damage }
}

// Life a healing skill restores.
fn skill_heal(healer, skill) {
    skill.power
}
"#;

/// The engine's stat columns, as tables name them, with what each means.
const STAT_COLUMNS: [(&str, &str); 7] = [
    ("life", "Most life"),
    ("mana", "Most mana"),
    ("attack_min", "Least damage of a hit"),
    ("attack_max", "Most damage of a hit"),
    ("defence", "Damage taken off each hit"),
    ("dodge", "Chance to avoid hits"),
    ("dexterity", "Accuracy"),
];

fn int(name: &str, doc: &str) -> Column {
    Column {
        doc: doc.into(),
        ..Column::new(name, ColumnKind::Int)
    }
}

fn text(name: &str, doc: &str) -> Column {
    Column {
        doc: doc.into(),
        ..Column::new(name, ColumnKind::Text)
    }
}

fn stat_columns() -> impl Iterator<Item = Column> {
    STAT_COLUMNS.iter().map(|(name, doc)| int(name, doc))
}

fn table(name: &str, doc: &str, columns: Vec<Column>, rows: Vec<Vec<Value>>) -> Table {
    Table {
        doc: doc.into(),
        rows,
        ..Table::new(name, columns)
    }
}

fn ints(values: &[i64]) -> impl Iterator<Item = Value> + '_ {
    values.iter().map(|&v| Value::Int(v))
}

/// The items table's columns the engine reads (others are the game's facts for formulas).
const ITEM_COLUMNS: [&str; 15] = [
    "id",
    "name",
    "price",
    "slot",
    "life",
    "mana",
    "attack_min",
    "attack_max",
    "defence",
    "dodge",
    "dexterity",
    "stack",
    "durability",
    "currency",
    "requires_level",
];

/// The tables the engine's rules read, each with an example row, as a new project gets them.
pub fn table_templates() -> Vec<Table> {
    let mut levels = vec![
        int("level", "The level (the key)"),
        int("exp", "Experience to the next level; 0 at the top"),
    ];
    levels.extend(stat_columns());
    let mut monsters = vec![
        int("id", "The monster type (spawn areas name it)"),
        text("name", "What players see"),
        int("look", "The game's model id for it"),
        int("level", "Its level"),
    ];
    monsters.extend(stat_columns());
    monsters.extend([
        int("attack_range", "Cells from its target it can hit from"),
        int("attack_ms", "Milliseconds between hits"),
        int("step_ms", "Milliseconds per step"),
        int("view_range", "Cells within which it notices players"),
        Column {
            doc: "Attacks players on sight".into(),
            ..Column::new("aggressive", ColumnKind::Bool)
        },
        int(
            "drop_money",
            "Most money it drops (half to all of it); 0 for none",
        ),
        text(
            "drops",
            "Items it may drop: `item:percent, ...` (e.g. `1:30, 2:5`)",
        ),
        Column {
            doc: "Hit, it calls the idle monsters of its kind around".into(),
            ..Column::new("assist", ColumnKind::Bool)
        },
        text(
            "hunts",
            "Players it attacks by their name: criminal, red, black, good (guards: criminal)",
        ),
        int(
            "flee_percent",
            "Below this percent of its life it runs; 0: never",
        ),
        Column {
            doc: "Never gives up a chase and is never stunned".into(),
            ..Column::new("boss", ColumnKind::Bool)
        },
        int("skill", "A skill it casts instead of hitting; 0: none"),
        int("skill_percent", "How often it casts its skill, in percent"),
    ]);
    let mut items = vec![
        int("id", "The item type"),
        text("name", "What players see"),
        int(
            "price",
            "What shops sell it for (they buy it back for a third)",
        ),
        int("slot", "The equipment slot it is worn in; 0: not worn"),
    ];
    items.extend(stat_columns());
    items.extend([
        int("stack", "Most one bag slot holds; 0 or 1: one each"),
        int(
            "durability",
            "Durability of a new one; 0: it never wears out",
        ),
        text("currency", "The currency it is priced in; empty: the money"),
        int("requires_level", "The level needed to wear it; 0: any"),
    ]);
    let slime = |id, name: &str, level, life| {
        let mut row = vec![
            Value::Int(id),
            Value::Text(name.into()),
            Value::Int(0),
            Value::Int(level),
        ];
        row.extend(ints(&[life, 0, 4, 7, 1, 2, 3, 1, 1500, 900, 6]));
        row.extend([
            Value::Bool(false),
            Value::Int(20),
            Value::Text("1:25".into()),
            Value::Bool(true),
            Value::Text(String::new()),
            Value::Int(0),
            Value::Bool(false),
            Value::Int(0),
            Value::Int(0),
        ]);
        row
    };
    let mut potion = vec![Value::Int(1), Value::Text("Healing Potion".into())];
    potion.extend(ints(&[20, 0, 70, 0, 0, 0, 0, 0, 0, 10, 0]));
    potion.extend([Value::Text(String::new()), Value::Int(0)]);
    let mut sword = vec![Value::Int(2), Value::Text("Short Sword".into())];
    sword.extend(ints(&[150, 4, 0, 0, 6, 10, 0, 0, 0, 1, 40]));
    sword.extend([Value::Text(String::new()), Value::Int(1)]);
    let mut first = ints(&[1, 100]).collect::<Vec<_>>();
    first.extend(ints(&[120, 60, 10, 15, 1, 5, 5]));
    vec![
        table(
            LEVELS,
            "Each level's experience and base stats",
            levels,
            vec![first],
        ),
        table(
            MONSTERS,
            "The monster types",
            monsters,
            vec![slime(1, "Slime", 1, 60)],
        ),
        table(ITEMS, "The item types", items, vec![potion, sword]),
        table(
            SHOPS,
            "What NPCs sell: one row per item an NPC has",
            vec![
                int("id", "The row"),
                int("npc", "The NPC selling"),
                int("item", "The item type sold"),
            ],
            vec![ints(&[1, 1, 1]).collect()],
        ),
        table(
            SKILLS,
            "The skills and spells players learn",
            skill_columns(),
            skill_rows(),
        ),
        table(
            STATUSES,
            "Status effects and buffs skills put on",
            status_columns(),
            status_rows(),
        ),
    ]
}

fn skill_columns() -> Vec<Column> {
    vec![
        int("id", "The skill (scripts and new characters name it)"),
        text("name", "What players see"),
        text("target", "What it is aimed at: enemy, self or ground"),
        text("kind", "What it does: damage or heal"),
        int("range", "Cells from the caster it reaches"),
        int(
            "area",
            "Cells around where it lands it reaches too; 0: only its target",
        ),
        int("cooldown_ms", "Milliseconds before it can be cast again"),
        int("mana", "Mana a cast costs"),
        int("power", "How strong it is (damage, or life healed)"),
        int("level", "The level needed to learn it"),
        text("effect", "The effect shown on each one it reaches"),
        int("status", "The status it puts on those it reaches; 0: none"),
    ]
}

fn status_columns() -> Vec<Column> {
    vec![
        int("id", "The status (skills name it)"),
        text("name", "What players see"),
        int("duration_ms", "Milliseconds it lasts"),
        int(
            "life_per_tick",
            "Life it gives (or takes, below 0) every tick",
        ),
        int("tick_ms", "Milliseconds between its life changes"),
        int("attack_percent", "Attack change in percent (+50, -30)"),
        int("defence_percent", "Defence change in percent"),
        int("dodge_percent", "Dodge and accuracy change in percent"),
        int(
            "speed_percent",
            "Walking and hitting speed change in percent",
        ),
        int("stun", "1: its bearer can't act"),
        text("effect", "The effect shown on its bearer while it lasts"),
    ]
}

fn status_rows() -> Vec<Vec<Value>> {
    let status = |id, name: &str, numbers: &[i64]| {
        let mut row = vec![Value::Int(id), Value::Text(name.into())];
        row.extend(ints(numbers));
        row.push(Value::Text(String::new()));
        row
    };
    vec![
        status(1, "Guard", &[15000, 0, 1000, 0, 50, 0, 0, 0]),
        status(2, "Poison", &[8000, -4, 1000, 0, 0, 0, -20, 0]),
        status(3, "Stun", &[2000, 0, 1000, 0, 0, 0, 0, 1]),
    ]
}

fn skill_rows() -> Vec<Vec<Value>> {
    let skill = |id, name: &str, target: &str, kind: &str, numbers: &[i64]| {
        let mut row = vec![
            Value::Int(id),
            Value::Text(name.into()),
            Value::Text(target.into()),
            Value::Text(kind.into()),
        ];
        row.extend(ints(&numbers[..6]));
        row.push(Value::Text(String::new()));
        row.push(Value::Int(numbers[6]));
        row
    };
    vec![
        skill(
            1,
            "Fire Bolt",
            "enemy",
            "damage",
            &[8, 0, 1500, 10, 18, 1, 0],
        ),
        skill(2, "Mend", "self", "heal", &[0, 0, 5000, 15, 40, 1, 0]),
        skill(3, "Quake", "ground", "damage", &[6, 2, 6000, 25, 12, 1, 3]),
        skill(4, "Guard", "self", "status", &[0, 0, 20000, 10, 0, 1, 1]),
    ]
}

/// The engine's rules for any game (a project's tables and formulas).
#[derive(Debug, Default)]
pub struct DataGameRules;

impl GameRules for DataGameRules {
    fn rules(&self, tables: Option<&Path>) -> (Box<dyn Rules>, Vec<String>) {
        let (rules, problems) = DataRules::load(tables);
        (Box::new(rules), problems)
    }
}

/// A level's row.
#[derive(Debug, Clone, Copy)]
struct Level {
    exp: u64,
    stats: Stats,
}

/// What a monster type drops.
#[derive(Debug, Clone, Default)]
struct Loot {
    money: u64,
    /// Item types and their chance in percent.
    items: Vec<(u32, u32)>,
}

#[derive(Default)]
pub struct DataRules {
    monsters: HashMap<u32, MonsterType>,
    loot: HashMap<u32, Loot>,
    items: HashMap<u32, ItemType>,
    levels: BTreeMap<u32, Level>,
    shops: HashMap<u32, Vec<u32>>,
    skills: HashMap<u32, SkillType>,
    statuses: HashMap<u32, StatusType>,
    formulas: Option<Formulas>,
    /// Formula failures: those not handed out yet, and every one seen (each is told once).
    problems: RefCell<(Vec<String>, HashSet<String>)>,
}

/// One row of a table, read by column name; missing columns read as nothing.
struct Row<'a> {
    table: &'a Table,
    values: &'a [Value],
}

impl Row<'_> {
    fn value(&self, column: &str) -> Option<&Value> {
        self.values.get(self.table.column(column)?)
    }

    fn int(&self, column: &str) -> i64 {
        self.value(column).and_then(Value::as_int).unwrap_or(0)
    }

    /// A yes/no column (a bool, or 1).
    fn flag(&self, column: &str) -> bool {
        self.value(column).and_then(Value::as_int) == Some(1)
    }

    fn uint(&self, column: &str) -> u32 {
        u32::try_from(self.int(column).max(0)).unwrap_or(u32::MAX)
    }

    fn text(&self, column: &str) -> &str {
        self.value(column)
            .and_then(Value::as_text)
            .unwrap_or_default()
    }

    fn stats(&self) -> Stats {
        Stats {
            max_life: self.uint("life"),
            max_mana: self.uint("mana"),
            attack_min: self.uint("attack_min"),
            attack_max: self.uint("attack_max"),
            defence: self.uint("defence"),
            dodge: self.uint("dodge"),
            dexterity: self.uint("dexterity"),
            ..Stats::default()
        }
    }
}

fn rows(table: &Table) -> impl Iterator<Item = Row<'_>> {
    table.rows.iter().map(move |values| Row { table, values })
}

/// `1:30, 2:5` → items 1 (30 %) and 2 (5 %); what can't be read is reported.
/// Who a monster hunts: `criminal`, `red`, `black` and `good`, separated by commas.
fn parse_hunts(text: &str) -> u8 {
    (text.split(','))
        .map(|w| match w.trim().to_ascii_lowercase().as_str() {
            "criminal" | "criminals" | "blue" => c3_game::HUNTS_CRIMINALS,
            "red" => c3_game::HUNTS_RED,
            "black" => c3_game::HUNTS_BLACK,
            "good" => c3_game::HUNTS_GOOD,
            _ => 0,
        })
        .fold(0, |all, bit| all | bit)
}

fn parse_drops(text: &str) -> Result<Vec<(u32, u32)>, String> {
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (item, chance) = part.split_once(':').unwrap_or((part, "100"));
            match (item.trim().parse(), chance.trim().parse()) {
                (Ok(item), Ok(chance)) => Ok((item, chance)),
                _ => Err(format!("\"{part}\" isn't `item:percent`")),
            }
        })
        .collect()
}

impl DataRules {
    /// The rules in the project's `tables/` (and `scripts/rules.rhai` beside it), and what
    /// was wrong with them.
    pub fn load(tables: Option<&Path>) -> (Self, Vec<String>) {
        let mut rules = Self::default();
        let mut problems = Vec::new();
        let Some(dir) = tables else {
            return (rules, problems);
        };
        let read = |name: &str, problems: &mut Vec<String>| {
            let file = dir.join(format!("{name}{}", c3_data::TABLE_SUFFIX));
            if !file.exists() {
                return None;
            }
            c3_data::load_table(&file)
                .inspect_err(|e| problems.push(format!("{name}: {e}")))
                .ok()
        };
        let tables: Vec<Table> = [LEVELS, MONSTERS, ITEMS, SKILLS, STATUSES, SHOPS]
            .iter()
            .filter_map(|name| read(name, &mut problems))
            .collect();
        rules.read_tables(&tables, &mut problems);
        let scripts = dir.parent().map(|p| p.join("scripts"));
        if let Some(scripts) = scripts {
            match Formulas::load_dir(&scripts) {
                Ok(formulas) => rules.formulas = formulas,
                Err(e) => problems.push(format!("{}: {e}", c3_script::RULES_SCRIPT)),
            }
        }
        (rules, problems)
    }

    /// The rules of `tables` (by their names), without formulas.
    pub fn from_tables(tables: &[Table]) -> Self {
        let mut rules = Self::default();
        rules.read_tables(tables, &mut Vec::new());
        rules
    }

    fn read_tables(&mut self, tables: &[Table], problems: &mut Vec<String>) {
        for t in tables {
            match t.name.as_str() {
                LEVELS => self.read_levels(t),
                MONSTERS => self.read_monsters(t, problems),
                ITEMS => self.read_items(t),
                SKILLS => self.read_skills(t, problems),
                STATUSES => self.read_statuses(t),
                SHOPS => {
                    for row in rows(t) {
                        self.shops
                            .entry(row.uint("npc"))
                            .or_default()
                            .push(row.uint("item"));
                    }
                }
                _ => {}
            }
        }
    }

    fn read_levels(&mut self, t: &Table) {
        for row in rows(t) {
            let level = Level {
                exp: row.int("exp").max(0) as u64,
                stats: Stats {
                    attack_range: 1,
                    ..row.stats()
                },
            };
            self.levels.insert(row.uint("level"), level);
        }
    }

    fn read_monsters(&mut self, t: &Table, problems: &mut Vec<String>) {
        for row in rows(t) {
            let id = row.uint("id");
            let mut stats = row.stats();
            stats.attack_range = row.uint("attack_range").max(1);
            stats.attack_ms = row.uint("attack_ms");
            stats.step_ms = row.uint("step_ms");
            let monster = MonsterType {
                id,
                name: row.text("name").to_owned(),
                look: row.uint("look"),
                level: row.uint("level").max(1),
                stats,
                view_range: row.uint("view_range"),
                aggressive: row.flag("aggressive"),
                hunts: parse_hunts(row.text("hunts")),
                assist: row.flag("assist"),
                flee_percent: row.uint("flee_percent").min(100),
                boss: row.flag("boss"),
                skill: row.uint("skill"),
                skill_percent: row.uint("skill_percent").min(100),
            };
            let items = parse_drops(row.text("drops")).unwrap_or_else(|e| {
                problems.push(format!("monster {id}'s drops: {e}"));
                Vec::new()
            });
            let money = row.int("drop_money").max(0) as u64;
            self.loot.insert(id, Loot { money, items });
            self.monsters.insert(id, monster);
        }
    }

    fn read_skills(&mut self, t: &Table, problems: &mut Vec<String>) {
        for row in rows(t) {
            let id = row.uint("id");
            let target = match row.text("target").trim().to_ascii_lowercase().as_str() {
                "" | "enemy" => SkillTarget::Enemy,
                "self" => SkillTarget::Myself,
                "ground" => SkillTarget::Ground,
                other => {
                    problems.push(format!(
                        "skill {id}: target \"{other}\" isn't enemy, self or ground"
                    ));
                    SkillTarget::Enemy
                }
            };
            let kind = match row.text("kind").trim().to_ascii_lowercase().as_str() {
                "" | "damage" => SkillKind::Damage,
                "heal" => SkillKind::Heal,
                "status" => SkillKind::Status,
                other => {
                    problems.push(format!(
                        "skill {id}: kind \"{other}\" isn't damage, heal or status"
                    ));
                    SkillKind::Damage
                }
            };
            let skill = SkillType {
                id,
                name: row.text("name").to_owned(),
                target,
                kind,
                range: row.uint("range"),
                area: row.uint("area"),
                cooldown_ms: row.uint("cooldown_ms"),
                mana: row.uint("mana"),
                power: row.uint("power"),
                level: row.uint("level"),
                effect: row.text("effect").to_owned(),
                status: row.uint("status"),
            };
            self.skills.insert(id, skill);
        }
    }

    fn read_statuses(&mut self, t: &Table) {
        let percent = |row: &Row, column: &str| row.int(column).clamp(-100, 10_000) as i32;
        for row in rows(t) {
            let id = row.uint("id");
            let status = StatusType {
                id,
                name: row.text("name").to_owned(),
                duration_ms: row.uint("duration_ms"),
                life_per_tick: row.int("life_per_tick").clamp(-1_000_000, 1_000_000) as i32,
                tick_ms: row.uint("tick_ms"),
                attack_percent: percent(&row, "attack_percent"),
                defence_percent: percent(&row, "defence_percent"),
                dodge_percent: percent(&row, "dodge_percent"),
                speed_percent: percent(&row, "speed_percent"),
                stun: row.int("stun") != 0,
                effect: row.text("effect").to_owned(),
            };
            self.statuses.insert(id, status);
        }
    }

    fn read_items(&mut self, t: &Table) {
        for row in rows(t) {
            let id = row.uint("id");
            let slot = u8::try_from(row.int("slot")).ok().filter(|&s| s > 0);
            let stats = row.stats();
            // Worn items add their life and mana; others (potions) restore them when used.
            let (restores_life, restores_mana, bonus) = match slot {
                Some(_) => (0, 0, stats),
                None => (stats.max_life, stats.max_mana, Stats::default()),
            };
            // Columns the engine doesn't read are the game's numbers for its formulas.
            let facts = (t.columns.iter().enumerate())
                .filter(|(_, c)| !ITEM_COLUMNS.contains(&c.name.as_str()))
                .filter_map(|(i, c)| Some((c.name.clone(), row.values.get(i)?.as_int()?)))
                .collect();
            let item = ItemType {
                id,
                name: row.text("name").to_owned(),
                price: row.int("price").max(0) as u64,
                restores_life,
                restores_mana,
                slot,
                bonus,
                stack: row.uint("stack"),
                durability: row.uint("durability"),
                currency: row.text("currency").to_owned(),
                requires_level: row.uint("requires_level"),
                facts,
            };
            self.items.insert(id, item);
        }
    }

    /// `name(args)` of the formulas, if the project defines it; failures are reported once.
    fn formula<T>(
        &self,
        name: &str,
        run: impl FnOnce(&Formulas) -> Result<T, String>,
    ) -> Option<T> {
        let formulas = self.formulas.as_ref().filter(|f| f.has(name))?;
        run(formulas)
            .inspect_err(|e| {
                let (pending, seen) = &mut *self.problems.borrow_mut();
                let line = format!("{}: {e}", c3_script::RULES_SCRIPT);
                if seen.insert(line.clone()) {
                    pending.push(line);
                }
            })
            .ok()
    }
}

/// `player_stats` of the formulas for `who`: `player_stats(level, player)` (the player its
/// profession and attributes), or a formula of the level alone.
pub(crate) fn stats_formula(f: &Formulas, who: &StatsFor) -> Result<Map, String> {
    let level = i64::from(who.level);
    if f.params("player_stats") == Some(1) {
        return f.map("player_stats", (level,));
    }
    let mut player: Map = (who.attributes.iter())
        .map(|(k, &v)| (k.as_str().into(), v.into()))
        .collect();
    player.insert("level".into(), level.into());
    player.insert("profession".into(), i64::from(who.profession).into());
    f.map("player_stats", (level, player))
}

/// `stats` (and `level`) as a script map.
pub(crate) fn fighter_map(f: &Fighter) -> Map {
    let s = &f.stats;
    let mut map = Map::new();
    for (key, value) in [
        ("level", f.level),
        ("life", s.max_life),
        ("mana", s.max_mana),
        ("attack_min", s.attack_min),
        ("attack_max", s.attack_max),
        ("defence", s.defence),
        ("dodge", s.dodge),
        ("dexterity", s.dexterity),
    ] {
        map.insert(key.into(), i64::from(value).into());
    }
    for (key, &value) in &f.facts {
        map.insert(key.as_str().into(), value.into());
    }
    map
}

/// An item type's numbers as a script map: its id, slot, level, stats and facts.
pub(crate) fn item_map(t: &ItemType) -> Map {
    let mut map = Map::new();
    for (key, value) in [
        ("id", t.id),
        ("slot", t.slot.map_or(0, u32::from)),
        ("requires_level", t.requires_level),
        ("attack_min", t.bonus.attack_min),
        ("attack_max", t.bonus.attack_max),
        ("defence", t.bonus.defence),
    ] {
        map.insert(key.into(), i64::from(value).into());
    }
    for (key, &value) in &t.facts {
        map.insert(key.as_str().into(), value.into());
    }
    map
}

/// A skill's numbers as a script map.
pub(crate) fn skill_map(s: &SkillType) -> Map {
    let mut map = Map::new();
    for (key, value) in [
        ("id", s.id),
        ("power", s.power),
        ("range", s.range),
        ("area", s.area),
        ("mana", s.mana),
        ("level", s.level),
    ] {
        map.insert(key.into(), i64::from(value).into());
    }
    map
}

pub(crate) fn uint(v: i64) -> u32 {
    u32::try_from(v.max(0)).unwrap_or(u32::MAX)
}

/// What the formula `drops(monster)` says monster type `kind` leaves: a list of item types,
/// and `#{money: n}` for money. The monster goes in as its numbers (`kind`, `level`, …).
pub(crate) fn drops_from(
    f: &Formulas,
    kind: u32,
    monster: Option<&MonsterType>,
) -> Result<Vec<Drop>, String> {
    let mut map = Map::new();
    map.insert("kind".into(), i64::from(kind).into());
    if let Some(m) = monster {
        map.insert("level".into(), i64::from(m.level).into());
        map.insert("life".into(), i64::from(m.stats.max_life).into());
    }
    let list = f.array("drops", (map,))?;
    Ok(list
        .into_iter()
        .filter_map(|v| {
            if let Ok(item) = v.as_int() {
                return u32::try_from(item).ok().map(Drop::Item);
            }
            let money = map_int(&v.try_cast::<Map>()?, "money");
            (money > 0).then_some(Drop::Money(money as u64))
        })
        .collect())
}

/// The engine's stats for `level` when the project gives none.
fn default_stats(level: u32) -> Stats {
    Stats {
        max_life: 100 + level * 20,
        max_mana: 50 + level * 10,
        attack_min: 8 + level * 2,
        attack_max: 12 + level * 3,
        defence: level,
        dodge: 5,
        dexterity: 5,
        attack_range: 1,
        ..Stats::default()
    }
}

impl Rules for DataRules {
    fn monster(&self, kind: u32) -> Option<&MonsterType> {
        self.monsters.get(&kind)
    }

    fn item(&self, kind: u32) -> Option<&ItemType> {
        self.items.get(&kind)
    }

    fn player_stats(&self, who: &StatsFor) -> Stats {
        let level = who.level;
        // The formula (it knows the player), else the level's row, else the engine's own.
        let mut stats = if let Some(map) = self.formula("player_stats", |f| stats_formula(f, who)) {
            Stats {
                max_life: uint(map_int(&map, "life")),
                max_mana: uint(map_int(&map, "mana")),
                attack_min: uint(map_int(&map, "attack_min")),
                attack_max: uint(map_int(&map, "attack_max")),
                defence: uint(map_int(&map, "defence")),
                dodge: uint(map_int(&map, "dodge")),
                dexterity: uint(map_int(&map, "dexterity")),
                ..Stats::default()
            }
        } else if let Some(row) = self.levels.get(&level) {
            row.stats
        } else {
            default_stats(level)
        };
        stats.attack_range = stats.attack_range.max(1);
        stats.attack_ms = 1000;
        stats.step_ms = 500;
        stats
    }

    fn level_exp(&self, level: u32) -> Option<u64> {
        let exp = if let Some(row) = self.levels.get(&level) {
            row.exp
        } else if !self.levels.is_empty() {
            // Past the last level of the table: the top.
            0
        } else if let Some(exp) =
            self.formula("level_exp", |f| f.int("level_exp", (i64::from(level),)))
        {
            exp.max(0) as u64
        } else if level < 100 {
            u64::from(level) * 100
        } else {
            0
        };
        (exp > 0).then_some(exp)
    }

    fn hit(&self, attacker: &Fighter, target: &Fighter, rng: &mut Rng) -> u32 {
        let a = &attacker.stats;
        let roll = rng.range(a.attack_min.into(), a.attack_max.max(a.attack_min).into());
        let chance = rng.range(0, 99);
        let scripted = self.formula("hit", |f| {
            let mut me = fighter_map(attacker);
            me.insert("roll".into(), roll.into());
            me.insert("chance".into(), chance.into());
            f.int("hit", (me, fighter_map(target)))
        });
        match scripted {
            Some(damage) => uint(damage),
            None => uint(roll).saturating_sub(target.stats.defence).max(1),
        }
    }

    fn hit_exp(&self, level: u32, target: &Fighter, damage: u32) -> u64 {
        self.formula("hit_exp", |f| {
            f.int(
                "hit_exp",
                (i64::from(level), fighter_map(target), i64::from(damage)),
            )
        })
        .map_or(damage.into(), |e| e.max(0) as u64)
    }

    fn kill_exp(&self, level: u32, target: &Fighter) -> u64 {
        self.formula("kill_exp", |f| {
            f.int("kill_exp", (i64::from(level), fighter_map(target)))
        })
        .map_or(u64::from(target.level) * 10, |e| e.max(0) as u64)
    }

    fn drops(&self, kind: u32, rng: &mut Rng) -> Vec<Drop> {
        if let Some(drops) =
            self.formula("drops", |f| drops_from(f, kind, self.monsters.get(&kind)))
        {
            return drops;
        }
        let Some(loot) = self.loot.get(&kind) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if loot.money > 0 {
            let money = rng.range((loot.money / 2).max(1) as i64, loot.money as i64);
            out.push(Drop::Money(money as u64));
        }
        for &(item, chance) in &loot.items {
            if rng.chance(chance, 100) {
                out.push(Drop::Item(item));
            }
        }
        out
    }

    fn loot(&self, kind: u32) -> c3_game::Loot {
        self.loot
            .get(&kind)
            .map_or_else(Default::default, |l| c3_game::Loot {
                money: l.money,
                money_percent: 100,
                items: l.items.clone(),
            })
    }

    fn shop(&self, npc: u32) -> &[u32] {
        self.shops.get(&npc).map_or(&[], Vec::as_slice)
    }

    fn skill(&self, id: u32) -> Option<&SkillType> {
        self.skills.get(&id)
    }

    fn status(&self, id: u32) -> Option<&StatusType> {
        self.statuses.get(&id)
    }

    fn skill_hit(
        &self,
        attacker: &Fighter,
        target: &Fighter,
        skill: &SkillType,
        rng: &mut Rng,
    ) -> u32 {
        let a = &attacker.stats;
        let roll = rng.range(a.attack_min.into(), a.attack_max.max(a.attack_min).into());
        let scripted = self.formula("skill_hit", |f| {
            let mut me = fighter_map(attacker);
            me.insert("roll".into(), roll.into());
            f.int("skill_hit", (me, fighter_map(target), skill_map(skill)))
        });
        match scripted {
            Some(damage) => uint(damage),
            None => (skill.power + uint(roll) / 2)
                .saturating_sub(target.stats.defence / 2)
                .max(1),
        }
    }

    fn can_wear(&self, player: &Fighter, item: &ItemType) -> bool {
        self.formula("can_wear", |f| {
            f.int("can_wear", (fighter_map(player), item_map(item)))
        })
        .is_none_or(|ok| ok != 0)
    }

    fn skill_heal(&self, healer: &Fighter, skill: &SkillType) -> u32 {
        self.formula("skill_heal", |f| {
            f.int("skill_heal", (fighter_map(healer), skill_map(skill)))
        })
        .map_or(skill.power, uint)
    }

    fn take_problems(&self) -> Vec<String> {
        std::mem::take(&mut self.problems.borrow_mut().0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_with(tables: &[Table], rules_script: Option<&str>) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "c3-data-rules-{}-{}",
            std::process::id(),
            tables.len() + rules_script.map_or(0, str::len)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("tables")).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        for t in tables {
            let file = dir
                .join("tables")
                .join(format!("{}{}", t.name, c3_data::TABLE_SUFFIX));
            c3_data::save_table(t, &file).unwrap();
        }
        if let Some(source) = rules_script {
            std::fs::write(dir.join("scripts").join(c3_script::RULES_SCRIPT), source).unwrap();
        }
        dir
    }

    #[test]
    fn the_templates_are_valid_tables_the_rules_read() {
        let templates = table_templates();
        for t in &templates {
            assert_eq!(t.problems(), Vec::new(), "{}", t.name);
        }
        let dir = project_with(&templates, None);
        let (rules, problems) = DataRules::load(Some(&dir.join("tables")));
        assert_eq!(problems, Vec::<String>::new());
        let slime = rules.monster(1).expect("the slime");
        assert_eq!((slime.name.as_str(), slime.stats.max_life), ("Slime", 60));
        assert_eq!(
            rules.item(1).unwrap().restores_life,
            70,
            "a potion restores"
        );
        assert_eq!(rules.item(2).unwrap().slot, Some(4));
        assert_eq!(rules.item(2).unwrap().bonus.attack_max, 10, "a sword adds");
        assert_eq!(rules.shop(1), &[1]);
        let quake = rules.skill(3).expect("the quake");
        assert_eq!((quake.target, quake.area), (SkillTarget::Ground, 2));
        assert_eq!(rules.skill(2).unwrap().kind, SkillKind::Heal);
        assert_eq!(rules.player_stats(&StatsFor::level(1)).max_life, 120);
        assert_eq!(rules.level_exp(1), Some(100));
        assert_eq!(rules.level_exp(2), None, "past the table: the top");
        let mut rng = Rng::new(3);
        let drops: Vec<Drop> = (0..40).flat_map(|_| rules.drops(1, &mut rng)).collect();
        assert!(drops.iter().any(|d| matches!(d, Drop::Item(1))));
        assert!(
            drops
                .iter()
                .all(|d| !matches!(d, Drop::Money(m) if *m < 10 || *m > 20))
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn formulas_replace_the_defaults_and_report_failures() {
        let dir = project_with(
            &[],
            Some(
                "fn hit(a, t) { if a.chance < 0 { 0 } else { 7 } }\nfn kill_exp(level, t) { t.nope.x }",
            ),
        );
        let (rules, problems) = DataRules::load(Some(&dir.join("tables")));
        assert!(problems.is_empty(), "{problems:?}");
        let f = Fighter {
            level: 1,
            stats: default_stats(1),
            ..Default::default()
        };
        assert_eq!(rules.hit(&f, &f, &mut Rng::new(1)), 7);
        assert_eq!(rules.kill_exp(1, &f), 10, "a failing formula falls back");
        let reported = rules.take_problems();
        assert_eq!(reported.len(), 1, "{reported:?}");
        rules.kill_exp(1, &f);
        assert!(rules.take_problems().is_empty(), "told once");
        assert_eq!(
            rules.player_stats(&StatsFor::level(3)).max_life,
            160,
            "the default stays"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_formulas_template_plays_like_the_defaults() {
        let dir = project_with(&table_templates(), Some(FORMULAS_TEMPLATE));
        let (rules, problems) = DataRules::load(Some(&dir.join("tables")));
        assert!(problems.is_empty(), "{problems:?}");
        let (strong, weak) = (
            Fighter {
                level: 5,
                stats: default_stats(5),
                ..Default::default()
            },
            Fighter {
                level: 1,
                stats: default_stats(1),
                ..Default::default()
            },
        );
        let mut rng = Rng::new(9);
        for _ in 0..20 {
            let damage = rules.hit(&strong, &weak, &mut rng);
            assert!((17..=26).contains(&damage), "{damage}");
        }
        assert_eq!(rules.kill_exp(5, &weak), 10);
        assert_eq!(rules.hit_exp(5, &weak, 12), 12);
        assert!(rules.take_problems().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
