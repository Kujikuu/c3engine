//! TQ `ini/*.dat` game tables. Encrypted ones are text after [`crate::cipher::decrypt`]
//! (itemtype, Monster, MapDestination, UserHelpInfo → [`ItemTypeTable`] / [`crate::ini::Ini`];
//! levexp → [`LevExpTable`]). The rest are binary:
//!
//! - `MagicType.dat` (`3drole/MagicData.cpp`): `count u32 · keys u32[count] · records[count]` (0x630 each)
//! - `Action.dat` (`3DRoleData.cpp` `CreateActionInfo`): `count u32 · {check1, index, check2, interval, check3}`
//! - `AutoAllot.dat` (`3drole/AutoAllot.cpp`): `profs u32 · levels u32 · prof_ids u32[profs] · {force, speed, health, soul}[profs][levels]`
//! - `Tips.dat` (writer `DataCovertDlg.cpp`): `header char[32] · version u32 · count u32 · {n u32, n × {len u32, bytes}}`
//! - `LevelExp.dat` (legacy, unused by 5065): u32s XORed with a GBK password.
//!
//! Game meaning (what a field does) belongs to the game module; these keep every field.

use crate::io::{Error, FixedStr, Reader, Result, Writer};

fn done(r: &Reader, what: &str) -> Result<()> {
    match r.remaining() {
        0 => Ok(()),
        n => Err(Error::Invalid(format!("{n} trailing bytes in {what}"))),
    }
}

fn text(data: &[u8]) -> String {
    encoding_rs::GBK
        .decode_without_bom_handling(data)
        .0
        .into_owned()
}

// ---------------------------------------------------------------- MagicType

/// Number of u32 fields between `szName` and `szUpgrade` in `MagicTypeInfo`.
const MAGIC_INTS_A: usize = 34;

/// One `MagicTypeInfo` (1,584 bytes). Field names follow `MagicData.h`.
#[derive(Debug, Clone, PartialEq)]
pub struct MagicType {
    pub id: u32,
    pub action_sort: u32,
    pub name: FixedStr<16>,
    /// Crime, Ground, Multi, Target, Level, MpCost, Power, IntoneDuration, HitPoint,
    /// Duration, Range, Distance, Status, ProfessionalRequired, ExpRequired,
    /// MonsterLevelRequired, Xp, WeaponSubType, ActiveTime, AutoActive, FloorAttribute,
    /// AutoLearn, LearnLevel, DropWeapon, UsePP, WeaponHit, idUseItem, idNextMagic,
    /// MagicBreak, ClientRepresent, SenderAction — in file order (see [`MagicType::field`]).
    pub ints: [u32; MAGIC_INTS_A - 3],
    pub upgrade: FixedStr<64>,
    /// `~` stands for a space.
    pub description: FixedStr<256>,
    pub intone_effect: FixedStr<64>,
    pub intone_sound: FixedStr<260>,
    pub sender_effect: FixedStr<64>,
    pub sender_sound: FixedStr<260>,
    pub target_delay: u32,
    pub target_effect: FixedStr<64>,
    pub target_sound: FixedStr<260>,
    pub ground_effect: FixedStr<64>,
    pub trace_effect: FixedStr<64>,
    pub screen_represent: u32,
    pub usable_in_market: u32,
    pub target_wound_delay: u32,
}

/// Names of [`MagicType::ints`], in order.
pub const MAGIC_INT_FIELDS: [&str; MAGIC_INTS_A - 3] = [
    "Crime",
    "Ground",
    "Multi",
    "Target",
    "Level",
    "MpCost",
    "Power",
    "IntoneDuration",
    "HitPoint",
    "Duration",
    "Range",
    "Distance",
    "Status",
    "ProfessionalRequired",
    "ExpRequired",
    "MonsterLevelRequired",
    "Xp",
    "WeaponSubType",
    "ActiveTime",
    "AutoActive",
    "FloorAttribute",
    "AutoLearn",
    "LearnLevel",
    "DropWeapon",
    "UsePP",
    "WeaponHit",
    "idUseItem",
    "idNextMagic",
    "MagicBreak",
    "ClientRepresent",
    "SenderAction",
];

impl MagicType {
    pub const SIZE: usize = 0x630;

    /// An integer field by its `MagicData.h` name.
    pub fn field(&self, name: &str) -> Option<u32> {
        MAGIC_INT_FIELDS
            .iter()
            .position(|f| f.eq_ignore_ascii_case(name))
            .map(|i| self.ints[i])
    }

    fn read(r: &mut Reader) -> Result<Self> {
        let id = r.u32()?;
        let action_sort = r.u32()?;
        let name = r.fixed()?;
        let mut ints = [0; MAGIC_INTS_A - 3];
        for v in &mut ints {
            *v = r.u32()?;
        }
        Ok(Self {
            id,
            action_sort,
            name,
            ints,
            upgrade: r.fixed()?,
            description: r.fixed()?,
            intone_effect: r.fixed()?,
            intone_sound: r.fixed()?,
            sender_effect: r.fixed()?,
            sender_sound: r.fixed()?,
            target_delay: r.u32()?,
            target_effect: r.fixed()?,
            target_sound: r.fixed()?,
            ground_effect: r.fixed()?,
            trace_effect: r.fixed()?,
            screen_represent: r.u32()?,
            usable_in_market: r.u32()?,
            target_wound_delay: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) {
        w.u32(self.id);
        w.u32(self.action_sort);
        w.fixed(&self.name);
        self.ints.iter().for_each(|&v| w.u32(v));
        w.fixed(&self.upgrade);
        w.fixed(&self.description);
        w.fixed(&self.intone_effect);
        w.fixed(&self.intone_sound);
        w.fixed(&self.sender_effect);
        w.fixed(&self.sender_sound);
        w.u32(self.target_delay);
        w.fixed(&self.target_effect);
        w.fixed(&self.target_sound);
        w.fixed(&self.ground_effect);
        w.fixed(&self.trace_effect);
        w.u32(self.screen_represent);
        w.u32(self.usable_in_market);
        w.u32(self.target_wound_delay);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MagicTypeTable {
    /// `id * 10 + level`, parallel to `records`.
    pub keys: Vec<u32>,
    pub records: Vec<MagicType>,
}

impl MagicTypeTable {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let n = r.u32()? as usize;
        if n * (4 + MagicType::SIZE) > r.remaining() {
            return Err(Error::Invalid(format!("{n} magic types exceed file")));
        }
        let keys = (0..n).map(|_| r.u32()).collect::<Result<_>>()?;
        let records = (0..n)
            .map(|_| MagicType::read(&mut r))
            .collect::<Result<_>>()?;
        done(&r, "MagicType.dat")?;
        Ok(Self { keys, records })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u32(self.records.len() as u32);
        self.keys.iter().for_each(|&k| w.u32(k));
        self.records.iter().for_each(|m| m.write(&mut w));
        w.buf
    }

    /// Magic `id` at `level`.
    pub fn get(&self, id: u32, level: u32) -> Option<&MagicType> {
        let key = id * 10 + level;
        self.keys
            .iter()
            .position(|&k| k == key)
            .map(|i| &self.records[i])
    }
}

// ---------------------------------------------------------------- Action

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionTiming {
    pub action: u32,
    /// Milliseconds.
    pub interval: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionTable {
    pub entries: Vec<ActionTiming>,
}

/// The three integrity words the client expects for record `i` (u32 wrapping).
fn action_checks(i: u32, action: u32, interval: u32) -> [u32; 3] {
    let c1 = action
        .wrapping_mul(2)
        .wrapping_add(interval)
        .wrapping_add(i);
    let c2 = i
        .wrapping_mul(c1)
        .wrapping_add(action.wrapping_mul(interval))
        .wrapping_sub(i);
    let c3 = c1
        .wrapping_mul(action)
        .wrapping_add(c2.wrapping_mul(7))
        .wrapping_add(interval.wrapping_mul(i));
    [c1, c2, c3]
}

impl ActionTable {
    /// Parses and verifies every record's check words.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let n = r.u32()?;
        let mut entries = Vec::new();
        for i in 0..n {
            let (c1, action, c2, interval, c3) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?, r.u32()?);
            if [c1, c2, c3] != action_checks(i, action, interval) {
                return Err(Error::Invalid(format!(
                    "Action.dat record {i} fails its checksum"
                )));
            }
            entries.push(ActionTiming { action, interval });
        }
        done(&r, "Action.dat")?;
        Ok(Self { entries })
    }

    /// Writes records with freshly computed check words.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u32(self.entries.len() as u32);
        for (i, e) in self.entries.iter().enumerate() {
            let [c1, c2, c3] = action_checks(i as u32, e.action, e.interval);
            [c1, e.action, c2, e.interval, c3]
                .into_iter()
                .for_each(|v| w.u32(v));
        }
        w.buf
    }
}

// ---------------------------------------------------------------- AutoAllot

/// Attribute points a profession gets at a level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allot {
    pub force: u32,
    pub speed: u32,
    pub health: u32,
    pub soul: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoAllotTable {
    /// Profession ids (profession / 10).
    pub professions: Vec<u32>,
    pub levels: u32,
    /// `allots[profession_index][level - 1]`.
    pub allots: Vec<Vec<Allot>>,
}

impl AutoAllotTable {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let (profs, levels) = (r.u32()? as usize, r.u32()?);
        if profs * (4 + levels as usize * 16) > r.remaining() {
            return Err(Error::Invalid("AutoAllot.dat sizes exceed file".into()));
        }
        let professions = (0..profs).map(|_| r.u32()).collect::<Result<_>>()?;
        let allots = (0..profs)
            .map(|_| {
                (0..levels)
                    .map(|_| {
                        Ok(Allot {
                            force: r.u32()?,
                            speed: r.u32()?,
                            health: r.u32()?,
                            soul: r.u32()?,
                        })
                    })
                    .collect::<Result<_>>()
            })
            .collect::<Result<_>>()?;
        done(&r, "AutoAllot.dat")?;
        Ok(Self {
            professions,
            levels,
            allots,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u32(self.professions.len() as u32);
        w.u32(self.levels);
        self.professions.iter().for_each(|&p| w.u32(p));
        for a in self.allots.iter().flatten() {
            [a.force, a.speed, a.health, a.soul]
                .into_iter()
                .for_each(|v| w.u32(v));
        }
        w.buf
    }

    pub fn get(&self, profession: u32, level: u32) -> Option<Allot> {
        let p = self
            .professions
            .iter()
            .position(|&id| id == profession / 10)?;
        self.allots[p].get(level.checked_sub(1)? as usize).copied()
    }
}

// ---------------------------------------------------------------- Tips

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TipsFile {
    pub header: FixedStr<32>,
    pub version: u32,
    /// Each record's strings, raw bytes including the stored terminator.
    pub records: Vec<Vec<Vec<u8>>>,
}

impl TipsFile {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let header = r.fixed()?;
        let version = r.u32()?;
        let n = r.u32()?;
        let mut records = Vec::new();
        for _ in 0..n {
            let strings = r.u32()?;
            records.push(
                (0..strings)
                    .map(|_| {
                        let len = r.u32()? as usize;
                        Ok(r.bytes(len)?.to_vec())
                    })
                    .collect::<Result<_>>()?,
            );
        }
        done(&r, "Tips.dat")?;
        Ok(Self {
            header,
            version,
            records,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.fixed(&self.header);
        w.u32(self.version);
        w.u32(self.records.len() as u32);
        for rec in &self.records {
            w.u32(rec.len() as u32);
            for s in rec {
                w.u32(s.len() as u32);
                w.bytes(s);
            }
        }
        w.buf
    }

    /// Text of a stored string (up to its NUL).
    pub fn string(bytes: &[u8]) -> String {
        text(bytes.split(|&b| b == 0).next().unwrap_or_default())
    }
}

// ---------------------------------------------------------------- legacy LevelExp

/// GBK bytes of `★就这样被你蒸熟★`, the `LevelExp.dat` XOR password (`Hero.cpp`).
const LEVEL_EXP_PASSWORD: [u8; 18] = [
    0xA1, 0xEF, 0xBE, 0xCD, 0xD5, 0xE2, 0xD1, 0xF9, 0xB1, 0xBB, 0xC4, 0xE3, 0xD5, 0xF4, 0xCA, 0xEC,
    0xA1, 0xEF,
];

/// Legacy `LevelExp.dat` (not loaded by 5065, which uses `levexp.dat`): u32 values XORed
/// with the password's sign-extended bytes.
pub fn decode_legacy_level_exp(data: &[u8]) -> Vec<u32> {
    data.as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let key = LEVEL_EXP_PASSWORD[i % LEVEL_EXP_PASSWORD.len()] as i8 as i32 as u32;
            u32::from_le_bytes(c) ^ key
        })
        .collect()
}

// ---------------------------------------------------------------- text tables

/// `itemtype.dat` after decryption: `Amount=N`, then N whitespace-separated rows
/// (39–40 tokens; `~` stands for a space; the description is always the last token).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemTypeTable {
    pub rows: Vec<Vec<String>>,
}

impl ItemTypeTable {
    pub fn parse(plain: &[u8]) -> Result<Self> {
        let text = text(plain);
        let mut lines = text.split(['\r', '\n']).filter(|l| !l.trim().is_empty());
        let amount: usize = lines
            .next()
            .and_then(|l| l.trim().strip_prefix("Amount="))
            .and_then(|v| v.trim().parse().ok())
            .ok_or_else(|| Error::Invalid("itemtype: missing Amount=".into()))?;
        let rows: Vec<Vec<String>> = lines
            .map(|l| l.split_whitespace().map(str::to_owned).collect())
            .collect();
        if rows.len() != amount {
            return Err(Error::Invalid(format!(
                "itemtype: Amount={amount} but {} rows",
                rows.len()
            )));
        }
        if let Some(bad) = rows.iter().find(|r| !(39..=40).contains(&r.len())) {
            return Err(Error::Invalid(format!(
                "itemtype: row {:?} has {} tokens",
                bad.first(),
                bad.len()
            )));
        }
        Ok(Self { rows })
    }

    pub fn get(&self, id: u32) -> Option<&[String]> {
        let id = id.to_string();
        self.rows.iter().find(|r| r[0] == id).map(Vec::as_slice)
    }
}

/// One `levexp.dat` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevExp {
    pub level: u32,
    pub exp_needed: u64,
    pub minutes: u32,
}

/// `levexp.dat` after decryption: lines `level exp minutes` (levels 1–150 kept).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevExpTable {
    pub rows: Vec<LevExp>,
}

impl LevExpTable {
    pub fn parse(plain: &[u8]) -> Self {
        let rows = text(plain)
            .split(['\r', '\n'])
            .filter_map(|l| {
                let mut t = l.split_whitespace();
                let row = LevExp {
                    level: t.next()?.parse().ok()?,
                    exp_needed: t.next()?.parse().ok()?,
                    minutes: t.next()?.parse().ok()?,
                };
                (1..=150).contains(&row.level).then_some(row)
            })
            .collect();
        Self { rows }
    }
}
