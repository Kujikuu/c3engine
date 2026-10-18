//! Conquer's game rules for the engine's simulation, read from the project's tables (TQ's
//! `cq_monstertype`, `cq_itemtype`, `cq_levexp`, `cq_goods`, `cq_point_allot`).
//!
//! The formulas follow TQ's server where its rules are known (damage − defence, at least 1;
//! experience per point of damage, scaled by level difference; a kill bonus of 5% of the
//! monster's life) and are simplified where they are not yet (dodge, drops): M7 brings them
//! to parity.

use c3_data::{Table, Value};
use c3_game::{Drop, Fighter, ItemType, MonsterType, Rng, Rules, Stats};
use std::collections::HashMap;
use std::path::Path;

/// `attack_user` bit: attacks players on sight (`ATKUSER_ACTIVE`).
const ACTIVE: i64 = 0x02;
/// Out of 100: chance a dead monster drops money, its life potion, its mana potion.
const MONEY_CHANCE: u32 = 40;
const LIFE_POTION_CHANCE: u32 = 8;
const MANA_POTION_CHANCE: u32 = 4;
/// Profession whose stat growth players get until professions are chosen (Trojan).
const PROFESSION: i64 = 1;
/// Milliseconds per walking step and between a player's hits.
const PLAYER_STEP_MS: u32 = 500;
const PLAYER_ATTACK_MS: u32 = 1000;

/// What a monster can drop.
#[derive(Debug, Clone, Copy, Default)]
struct MonsterDrops {
    money: u64,
    life_potion: u32,
    mana_potion: u32,
}

/// Stat points of a level: strength, agility, vitality, spirit.
type Points = [u32; 4];

#[derive(Debug, Default)]
pub struct ConquerRules {
    monsters: HashMap<u32, MonsterType>,
    drops: HashMap<u32, MonsterDrops>,
    items: HashMap<u32, ItemType>,
    /// Experience to rise from level `i + 1`.
    level_exp: Vec<u64>,
    points: HashMap<u32, Points>,
    shops: HashMap<u32, Vec<u32>>,
}

/// A table's rows read by column name.
struct Rows<'a> {
    table: &'a Table,
    columns: Vec<usize>,
}

impl<'a> Rows<'a> {
    fn new(table: &'a Table, names: &[&str]) -> Result<Self, String> {
        let columns = names
            .iter()
            .map(|n| {
                table
                    .column(n)
                    .ok_or_else(|| format!("table {} has no column {n}", table.name))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { table, columns })
    }

    /// Each row's values in the order the columns were named.
    fn each(&self) -> impl Iterator<Item = Vec<&'a Value>> + '_ {
        self.table
            .rows
            .iter()
            .map(|row| self.columns.iter().map(|&c| &row[c]).collect())
    }
}

fn int(v: &Value) -> i64 {
    v.as_int().unwrap_or(0)
}

fn uint(v: &Value) -> u32 {
    int(v).clamp(0, u32::MAX.into()) as u32
}

impl ConquerRules {
    /// Reads the rules from the tables in `dir`; a missing table leaves its part empty and is
    /// reported.
    pub fn from_tables(dir: &Path) -> (Self, Vec<String>) {
        let mut rules = Self::default();
        let mut problems = Vec::new();
        let mut load = |name: &str, read: &mut dyn FnMut(&Table) -> Result<(), String>| {
            let file = dir.join(format!("{name}{}", c3_data::TABLE_SUFFIX));
            let result = c3_data::load_table(&file)
                .map_err(|e| format!("table {name}: {e}"))
                .and_then(|t| read(&t));
            if let Err(e) = result {
                problems.push(e);
            }
        };
        load("cq_monstertype", &mut |t| rules.read_monsters(t));
        load(crate::server_data::ITEM_TABLE, &mut |t| rules.read_items(t));
        load("cq_levexp", &mut |t| rules.read_levels(t));
        load("cq_point_allot", &mut |t| rules.read_points(t));
        load("cq_goods", &mut |t| rules.read_shops(t));
        (rules, problems)
    }

    fn read_monsters(&mut self, t: &Table) -> Result<(), String> {
        let rows = Rows::new(
            t,
            &[
                "id",
                "name",
                "lookface",
                "level",
                "life",
                "attack_min",
                "attack_max",
                "defence",
                "dodge",
                "dexterity",
                "attack_range",
                "view_range",
                "attack_speed",
                "move_speed",
                "attack_user",
                "drop_money",
                "drop_hp",
                "drop_mp",
            ],
        )?;
        for v in rows.each() {
            let id = uint(v[0]);
            self.monsters.insert(
                id,
                MonsterType {
                    id,
                    name: v[1].as_text().unwrap_or_default().to_owned(),
                    look: uint(v[2]),
                    level: uint(v[3]),
                    stats: Stats {
                        max_life: uint(v[4]).max(1),
                        max_mana: 0,
                        attack_min: uint(v[5]),
                        attack_max: uint(v[6]).max(uint(v[5])),
                        defence: uint(v[7]),
                        dodge: uint(v[8]),
                        dexterity: uint(v[9]),
                        attack_range: uint(v[10]).max(1),
                        attack_ms: uint(v[12]).max(300),
                        step_ms: uint(v[13]).max(200),
                    },
                    view_range: uint(v[11]),
                    aggressive: int(v[14]) & ACTIVE != 0,
                },
            );
            self.drops.insert(
                id,
                MonsterDrops {
                    money: uint(v[15]).into(),
                    life_potion: uint(v[16]),
                    mana_potion: uint(v[17]),
                },
            );
        }
        Ok(())
    }

    fn read_items(&mut self, t: &Table) -> Result<(), String> {
        let rows = Rows::new(
            t,
            &[
                "id",
                "name",
                "price",
                "life",
                "mana",
                "attack_min",
                "attack_max",
                "defense",
                "dodge",
                "dexterity",
            ],
        )?;
        for v in rows.each() {
            let id = uint(v[0]);
            let slot = slot(id);
            // Worn items add their life and mana; potions restore them.
            let (restores, adds) = if slot.is_some() {
                ((0, 0), (uint(v[3]), uint(v[4])))
            } else {
                ((uint(v[3]), uint(v[4])), (0, 0))
            };
            self.items.insert(
                id,
                ItemType {
                    id,
                    name: v[1].as_text().unwrap_or_default().to_owned(),
                    price: uint(v[2]).into(),
                    restores_life: restores.0,
                    restores_mana: restores.1,
                    slot,
                    bonus: Stats {
                        max_life: adds.0,
                        max_mana: adds.1,
                        attack_min: uint(v[5]),
                        attack_max: uint(v[6]),
                        defence: uint(v[7]),
                        dodge: uint(v[8]),
                        dexterity: uint(v[9]),
                        ..Default::default()
                    },
                },
            );
        }
        Ok(())
    }

    fn read_levels(&mut self, t: &Table) -> Result<(), String> {
        let rows = Rows::new(t, &["level", "exp"])?;
        let mut levels: Vec<(u32, u64)> = rows
            .each()
            .map(|v| (uint(v[0]), int(v[1]) as u64))
            .collect();
        levels.sort_unstable();
        self.level_exp = levels.into_iter().map(|(_, exp)| exp).collect();
        Ok(())
    }

    fn read_points(&mut self, t: &Table) -> Result<(), String> {
        let rows = Rows::new(
            t,
            &["profession", "level", "force", "Speed", "health", "soul"],
        )?;
        for v in rows.each().filter(|v| int(v[0]) == PROFESSION) {
            self.points
                .insert(uint(v[1]), [uint(v[2]), uint(v[3]), uint(v[4]), uint(v[5])]);
        }
        Ok(())
    }

    fn read_shops(&mut self, t: &Table) -> Result<(), String> {
        let rows = Rows::new(t, &["ownerid", "itemtype"])?;
        for v in rows.each() {
            self.shops.entry(uint(v[0])).or_default().push(uint(v[1]));
        }
        Ok(())
    }

    /// Stat points at `level`: the table's, else the nearest level below it.
    fn points_at(&self, level: u32) -> Points {
        (1..=level)
            .rev()
            .find_map(|l| self.points.get(&l))
            .copied()
            .unwrap_or([5, 2, 3, 0])
    }

    pub fn monster_count(&self) -> usize {
        self.monsters.len()
    }
}

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

/// Where item type `id` is worn, from its type number (TQ item sorts: 11xxxx helmets,
/// 12xxxx necklaces, 13xxxx armor, 15xxxx rings, 16xxxx boots, 4xxxxx/5xxxxx weapons,
/// 9xxxxx shields, 105xxxx arrows).
fn slot(id: u32) -> Option<u8> {
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

/// Experience scale (percent) for hitting a monster `delta` levels above the player.
fn level_scale(delta: i64) -> u64 {
    match delta {
        3.. => 120,
        -3..=2 => 100,
        -8..=-4 => 50,
        _ => 10,
    }
}

impl Rules for ConquerRules {
    fn monster(&self, kind: u32) -> Option<&MonsterType> {
        self.monsters.get(&kind)
    }

    fn item(&self, kind: u32) -> Option<&ItemType> {
        self.items.get(&kind)
    }

    /// TQ's life formula (`CUser::GetMaxLife`): vitality × 24 + the other points × 3.
    fn player_stats(&self, level: u32) -> Stats {
        let [strength, agility, vitality, spirit] = self.points_at(level);
        Stats {
            max_life: vitality * 24 + (strength + agility + spirit) * 3,
            max_mana: spirit * 5,
            attack_min: strength,
            attack_max: strength + strength / 2,
            defence: 0,
            dodge: agility,
            dexterity: agility,
            attack_range: 1,
            attack_ms: PLAYER_ATTACK_MS,
            step_ms: PLAYER_STEP_MS,
        }
    }

    fn level_exp(&self, level: u32) -> Option<u64> {
        let exp = *self.level_exp.get(level.checked_sub(1)? as usize)?;
        (exp > 0).then_some(exp)
    }

    fn hit(&self, attacker: &Fighter, target: &Fighter, rng: &mut Rng) -> u32 {
        let miss = target
            .stats
            .dodge
            .saturating_sub(attacker.stats.dexterity)
            .min(90)
            / 2;
        if rng.chance(miss, 100) {
            return 0;
        }
        let a = &attacker.stats;
        let attack = rng.range(a.attack_min.into(), a.attack_max.into()) as u32;
        attack.saturating_sub(target.stats.defence).max(1)
    }

    fn hit_exp(&self, level: u32, target: &Fighter, damage: u32) -> u64 {
        u64::from(damage) * level_scale(i64::from(target.level) - i64::from(level)) / 100
    }

    fn kill_exp(&self, level: u32, target: &Fighter) -> u64 {
        u64::from(target.stats.max_life) * 5 / 100
            * level_scale(i64::from(target.level) - i64::from(level))
            / 100
    }

    fn drops(&self, kind: u32, rng: &mut Rng) -> Vec<Drop> {
        let Some(d) = self.drops.get(&kind) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if d.money > 0 && rng.chance(MONEY_CHANCE, 100) {
            out.push(Drop::Money(
                rng.range((d.money / 2).max(1) as i64, d.money as i64) as u64,
            ));
        }
        if d.life_potion != 0 && rng.chance(LIFE_POTION_CHANCE, 100) {
            out.push(Drop::Item(d.life_potion));
        }
        if d.mana_potion != 0 && rng.chance(MANA_POTION_CHANCE, 100) {
            out.push(Drop::Item(d.mana_potion));
        }
        out
    }

    fn shop(&self, npc: u32) -> &[u32] {
        self.shops.get(&npc).map_or(&[], Vec::as_slice)
    }
}
