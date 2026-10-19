//! Conquer's formulas as a blueprint (`conquer/rules`), written from TQ's server
//! (`tq_re/spec`: progression §4–5, battle §2): a player's life, mana, attack and dodge from
//! its attribute points and profession; hit chance and damage; experience for hits and kills;
//! who may wear what; and the points a class gets as it levels. Each formula is text here and
//! nodes in the blueprint ([`ExprWriter`]), so the project's rules are edited as graphs.

use crate::rules::POINTS_TABLE;
use c3_script::blueprint::{
    Blueprint, ExprWriter, Function, Graph, GraphNode, Literal, Operand, PinRef, Wire,
};

/// The blueprint's name in a project.
pub const RULES_BLUEPRINT: &str = "conquer/rules";

/// TQ's class of a profession: 1 Trojan, 2 Warrior, 4 Archer, 10 Taoist (`AutoAllot`).
const CLASS: &str =
    "if((profession % 1000) / 10 > 10, (profession % 1000) / 100 * 10, (profession % 1000) / 10)";

/// TQ's MaxLife (progression §5.1): 3 × (strength + agility + spirit) + 24 × vitality, and
/// Trojans 5–15% more by their step; worn items' enchanting adds its life (items §6.4).
const LIFE: &str = "worn_enchant + base + if((profession % 1000) / 10 == 1, base * if(profession % 10 == 1, 5, if(profession % 10 == 2, 8, if(profession % 10 == 3, 10, if(profession % 10 == 4, 12, if(profession % 10 == 5, 15, 0))))) / 100, 0)";
const LIFE_BASE: &str = "(strength + agility + spirit + vitality * 8) * 3";
/// TQ's MaxMana (§5.2): spirit × 5; Fire and Water Taoists × 5 more per step (up to × 30).
const MANA: &str =
    "spirit * if((profession % 1000) / 10 > 10, min(5 + (profession % 10) * 5, 30), 5)";

/// TQ's hit chance (battle §2.3), in percent: half the attacker's agility (a monster's
/// dexterity) against the target's dodge, 10–90; a player hitting a lower monster gets 10 more
/// per 5 levels.
const HIT_CHANCE: &str = "min(max(if(attacker.monster == 0, attacker.agility, attacker.dexterity) / 2 - target.dodge + 100, 10), 90) + if(attacker.monster == 0 && target.monster != 0 && attacker.level > target.level, (attacker.level - target.level) / 5 * 10, 0)";
/// Melee damage (battle §2.4–2.6), worked out in steps; nothing when it misses.
const HIT_STEPS: [(&str, &str); 14] = [
    // The attack: the roll, Dragon gems, and against a monster the level difference.
    (
        "pve",
        "attacker.monster == 0 && target.monster != 0 || attacker.monster != 0 && target.monster == 0",
    ),
    (
        "atk0",
        "attacker.roll + attacker.status_attack_add + attacker.socket_gem_attack * attacker.roll / 100",
    ),
    // A weapon skill above level 12: 1% more a level.
    (
        "prof",
        "if(attacker.monster == 0, weapon_skill(attacker), 0)",
    ),
    ("atk1", "atk0 + max(prof - 12, 0) * atk0 / 100"),
    (
        "atk",
        "if(pve, max(100 + if(attacker.monster == 0, disdain_usr_atk_mst(attacker.level - target.level), disdain_mst_atk(attacker.level - target.level)), 0) * atk1 / 100, atk1)",
    ),
    // Superman halves the defence.
    (
        "def",
        "if(attacker.status_superman > 0, (target.defence + target.status_defence_add) / 2, target.defence + target.status_defence_add)",
    ),
    // The least damage: the attacker's attack over the target armour's quality.
    (
        "armor",
        "if(target.monster != 0, target.armor_type, target.worn3) % 10",
    ),
    (
        "least",
        "max(if(attacker.monster == 0, attacker.strength, (attacker.attack_min + attacker.attack_max) / 2) / if(armor == 6, 100, if(armor == 7, 200, if(armor == 8, 500, if(armor == 9, 1000, 10)))), 1)",
    ),
    // A bow (500xxx) goes against dodge, not defence, and players take a share by level.
    (
        "bow",
        "attacker.monster == 0 && attacker.worn4 / 1000 == 500",
    ),
    (
        "raw",
        "if(bow, max(atk - atk * max(target.dodge - if(target.monster == 0, 50, 25), 0) / 100, least) * if(target.monster != 0, 100, if(target.level < 22, 35, if(target.level < 50, 25, if(target.level < 90, 20, if(target.level < 110, 15, 12))))) / 100, max(max(atk - def, 0), least))",
    ),
    // A player target: Tortoise gems take half, rebirths a share, blessing its percent.
    (
        "lucky_hit",
        // Lucky time (battle §2.7): 3% (1% against players) of hits do double.
        "attacker.monster == 0 && attacker.lucky_time > 0 && random(0, 99) < if(target.monster != 0, 3, 1)",
    ),
    ("hit_raw", "if(lucky_hit, raw * 2, raw)"),
    (
        "tortoise",
        "if(target.monster == 0, hit_raw - hit_raw / 2 * min(target.socket_gem_defence, 100) / 100, hit_raw)",
    ),
    (
        "reborn",
        "if(target.monster != 0, if(target.defence2 > 0, tortoise * target.defence2 / 10000, tortoise), tortoise * if(target.rebirths >= 2, if(attacker.monster != 0 || attacker.rebirths >= 2, 7000, 5000), if(target.rebirths >= 1 && target.profession % 10 >= 3, 7000, 10000)) / 10000)",
    ),
];
/// A player in lucky time takes nothing from 2% of hits (battle §2.5a).
const HIT: &str = "if(attacker.chance < chance && !(target.monster == 0 && target.lucky_time > 0 && random(0, 99) < 2), max(reborn - if(target.monster == 0, reborn * min(target.worn_bless, 100) / 100, 0), 1), 0)";

/// Experience (battle exp §1–3): the life taken (and on a kill 5% of the monster's), Rainbow
/// gems' percent more, then the level difference's factor (the XP one under an XP skill).
const EXP_STEPS: [(&str, &str); 4] = [
    ("gem", "base + attacker.socket_gem_exp * base / 100"),
    ("delta", "attacker.level - target.level"),
    ("adjusted", EXP_DISDAIN),
    (
        "b120",
        "if(attacker.blessing_until > attacker.clock, adjusted + adjusted * 20 / 100, adjusted)",
    ),
];
const EXP_DISDAIN: &str = "max((100 + if(attacker.status_superman > 0 || attacker.status_cyclone > 0, disdain_xp_exp_factor(delta), disdain_exp_factor(delta))) * gem / 100, 0)";
/// What the player is given of it (battle exp §5, AwardBattleExp): nothing while cursed, a
/// fifth more while blessed, half from level 120, a third after a second rebirth, and the
/// timed percent (action 1048).
const EXP: &str = "if(attacker.cursed > 0, 0, if(attacker.exp_until > attacker.clock, attacker.exp_percent, 100) * max(if(attacker.rebirths == 2, 1, 0), if(attacker.level >= 120, b120 / 2, b120) / if(attacker.rebirths == 2, 3, 1)) / 100)";

/// TQ's equip check (items §9.1): sex, class and step, attribute points.
const CAN_WEAR: &str = "(item.req_sex == 0 || item.req_sex == sex) && (item.req_profession == 0 || (item.req_profession % 1000) / 10 == class && profession % 10 >= item.req_profession % 10) && player.strength >= item.req_force && player.agility >= item.req_speed && player.vitality >= item.req_health && player.spirit >= item.req_soul";

/// The rules blueprint.
pub fn rules_blueprint() -> Blueprint {
    let mut bp = Blueprint::default();
    let mut lane = 0.0;
    let mut next_lane = |height: f32| {
        let top = lane;
        lane += height;
        top
    };
    player_stats(&mut bp.events, next_lane(600.0));
    hit(&mut bp.events, next_lane(500.0));
    skill_hit(&mut bp.events, next_lane(700.0));
    exp(&mut bp.events, next_lane(300.0));
    can_wear(&mut bp.events, next_lane(500.0));
    level_up(&mut bp.events, next_lane(300.0));
    death(&mut bp.events, next_lane(700.0));
    revive(&mut bp.events, next_lane(200.0));
    xp_bar(&mut bp.events, next_lane(500.0));
    weapon_skills(&mut bp.events, next_lane(1800.0));
    wanted_hunt(&mut bp.events, next_lane(500.0));
    red_name_jailed(&mut bp.events, next_lane(500.0));
    magic_skills(&mut bp.events, next_lane(900.0));
    upgrades(&mut bp.events, next_lane(900.0));
    rebirth(&mut bp.events, next_lane(1200.0));
    upequipment(&mut bp.events, next_lane(1600.0));
    combine(&mut bp.events, next_lane(2400.0));
    lottery(&mut bp.events, next_lane(700.0));
    reset_points(&mut bp.events, next_lane(500.0));
    bp.functions.push(allot());
    bp
}

/// Adds node `def` at `pos`; its id.
fn node(g: &mut Graph, def: &str, pos: [f32; 2], values: &[(&str, Literal)]) -> u32 {
    let id = g.next_id();
    g.nodes.push(GraphNode {
        id,
        def: def.to_owned(),
        pos,
        values: (values.iter())
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
    });
    id
}

fn wire(g: &mut Graph, from: (u32, &str), to: (u32, &str)) {
    g.wires.push(Wire {
        from: PinRef {
            node: from.0,
            pin: from.1.to_owned(),
        },
        to: PinRef {
            node: to.0,
            pin: to.1.to_owned(),
        },
    });
}

fn text(t: &str) -> Literal {
    Literal::Text(t.to_owned())
}

/// Names a formula reads: `record.field` (or a plain field of `default_record`) as a Read a
/// number node, pins of the event by name, and values worked out before (`let`s).
struct Names<'a> {
    /// Event output pins by name: (node, pin).
    pins: Vec<(&'a str, u32)>,
    /// The record plain names are fields of.
    default_record: Option<&'a str>,
    lets: Vec<(String, Operand)>,
}

impl Names<'_> {
    fn resolve(&self, w: &mut ExprWriter, name: &str) -> Option<Operand> {
        if let Some((_, v)) = self.lets.iter().find(|(n, _)| n == name) {
            return Some(v.clone());
        }
        if let Some(&(pin, node)) = self.pins.iter().find(|(p, _)| *p == name) {
            return Some(Operand::Pin {
                node,
                pin: pin.to_owned(),
            });
        }
        let (record, field) = match name.split_once('.') {
            Some((record, field)) => (record, field),
            None => (self.default_record?, name),
        };
        let &(pin, node) = self.pins.iter().find(|(p, _)| *p == record)?;
        let get = w.node("record.get", &[("field", text(field))]);
        w.feed(
            &Operand::Pin {
                node,
                pin: pin.to_owned(),
            },
            get,
            "record",
        );
        Some(Operand::Pin {
            node: get,
            pin: "value".into(),
        })
    }

    /// Writes `text` and keeps its value as `name` for the formulas after.
    fn define(&mut self, w: &mut ExprWriter, name: &str, text: &str) {
        let value = self.write(w, text);
        self.lets.push((name.to_owned(), value));
    }

    /// Writes `text`; `disdain_<column>(delta)` is that column of the disdain table at the
    /// level difference (kept within the table's −400..400).
    fn write(&self, w: &mut ExprWriter, text: &str) -> Operand {
        w.write(
            text,
            &mut |w, n, args| match (n.strip_prefix("disdain_"), args) {
                // `magic_<column>(key)`: that column of the magic levels table.
                (None, [key]) if n.starts_with("magic_") => {
                    let lookup = w.node(
                        "table.lookup",
                        &[
                            ("table", self::text(crate::rules::MAGIC_LEVELS_TABLE)),
                            ("column", self::text(n.trim_start_matches("magic_"))),
                        ],
                    );
                    w.feed(key, lookup, "key");
                    Some(Operand::Pin {
                        node: lookup,
                        pin: "value".into(),
                    })
                }
                (Some(column), [delta]) => {
                    let clamped = w
                        .write("max(min(d, 400), -400)", &mut |_, n, _| {
                            (n == "d").then(|| delta.clone())
                        })
                        .ok()?;
                    let lookup = w.node(
                        "table.lookup",
                        &[
                            ("table", self::text(crate::rules::DISDAIN_TABLE)),
                            ("column", self::text(column)),
                        ],
                    );
                    w.feed(&clamped, lookup, "key");
                    Some(Operand::Pin {
                        node: lookup,
                        pin: "value".into(),
                    })
                }
                // The level of the weapon skill of what `who` holds in the right hand.
                (None, [who]) if n == "weapon_skill" => {
                    let worn = w.node("record.get", &[("field", self::text("worn4"))]);
                    w.feed(who, worn, "record");
                    let kind = Operand::Pin {
                        node: worn,
                        pin: "value".into(),
                    };
                    let subtype = w
                        .write("k / 1000", &mut |_, n, _| (n == "k").then(|| kind.clone()))
                        .ok()?;
                    let skill = w.node("record.get_named", &[("prefix", self::text("weapon_"))]);
                    w.feed(who, skill, "record");
                    w.feed(&subtype, skill, "key");
                    Some(Operand::Pin {
                        node: skill,
                        pin: "value".into(),
                    })
                }
                _ => self.resolve(w, n),
            },
        )
        .unwrap_or_else(|e| panic!("Conquer's formula \"{text}\": {e}"))
    }
}

/// Formula event `def` ending at a Return of `value` worked out by `body`.
fn formula(
    g: &mut Graph,
    top: f32,
    def: &str,
    pins: &[&'static str],
    default_record: Option<&'static str>,
    body: impl FnOnce(&mut ExprWriter, &mut Names) -> Operand,
) {
    let event = node(g, def, [0.0, top], &[]);
    let mut names = Names {
        pins: pins.iter().map(|p| (*p, event)).collect(),
        default_record,
        lets: Vec::new(),
    };
    let mut w = ExprWriter::new(g, [260.0, top]);
    let value = body(&mut w, &mut names);
    let ret = w.node("flow.return", &[]);
    w.feed(&value, ret, "value");
    wire(g, (event, "then"), (ret, "exec"));
}

fn player_stats(g: &mut Graph, top: f32) {
    formula(
        g,
        top,
        "formula.player_stats",
        &["level", "player"],
        Some("player"),
        |w, names| {
            names.define(w, "base", LIFE_BASE);
            let stats = [
                ("life", LIFE),
                ("mana", MANA),
                // Attack is strength (weapons add theirs); dodge starts at 50.
                ("attack_min", "strength"),
                ("attack_max", "strength"),
                ("defence", "0"),
                ("dodge", "50"),
                ("dexterity", "agility"),
            ];
            let values: Vec<(&str, Operand)> = (stats.iter())
                .map(|(pin, text)| (*pin, names.write(w, text)))
                .collect();
            let record = w.node("record.stats", &[]);
            for (pin, value) in &values {
                w.feed(value, record, pin);
            }
            Operand::Pin {
                node: record,
                pin: "value".into(),
            }
        },
    );
}

/// Magic damage (battle M.6) in steps: the skill's power (at the caster's level of it) and
/// the caster's gear, less against players, the level difference against monsters, the
/// target's magic defence, Phoenix gems.
const MAGIC_STEPS: [(&str, &str); 6] = [
    (
        "m_power",
        "skill.power + if(attacker.monster == 0, attacker.gear_magic_atk, 0)",
    ),
    (
        "m_level",
        "if(target.monster == 0, m_power - m_power / 4, if(attacker.monster == 0, max(100 + disdain_usr_atk_mst(attacker.level - target.level), 0) * m_power / 100, m_power))",
    ),
    (
        "m_def",
        "m_level - min(if(target.monster != 0, target.magic_def, target.gear_magic_def), 90) * m_level / 100",
    ),
    (
        "m_gem",
        "if(attacker.monster == 0, m_def + attacker.socket_gem_magic * m_def / 100, m_def)",
    ),
    (
        "m_super",
        "if(attacker.socket_gem_magic >= 180, m_gem + m_gem / 20, m_gem)",
    ),
    (
        "m_tortoise",
        "if(target.monster == 0, m_super - m_super / 2 * min(target.socket_gem_defence, 100) / 100, m_super)",
    ),
];
/// A skill's damage: physical skills (power 30000 + percent) a weapon hit's share, others
/// magic; both then rebirth defence (players) or `defence2` (monsters), and blessing.
const SKILL_HIT: &str = "if(skill.power >= 30000, reborn * (skill.power - 30000) / 100, if(target.monster != 0, if(target.defence2 > 0, m_tortoise * target.defence2 / 10000, m_tortoise), m_tortoise * if(target.rebirths >= 2, if(attacker.monster != 0 || attacker.rebirths >= 2, 7000, 5000), if(target.rebirths >= 1 && target.profession % 10 >= 3, 7000, 10000)) / 10000))";

fn skill_hit(g: &mut Graph, top: f32) {
    formula(
        g,
        top,
        "formula.skill_hit",
        &["attacker", "target", "skill"],
        None,
        |w, names| {
            for (name, text) in HIT_STEPS.iter().chain(&MAGIC_STEPS) {
                names.define(w, name, text);
            }
            names.define(w, "damage", SKILL_HIT);
            names.write(
                w,
                "max(damage - if(target.monster == 0, damage * min(target.worn_bless, 100) / 100, 0), 1)",
            )
        },
    );
}

fn hit(g: &mut Graph, top: f32) {
    formula(
        g,
        top,
        "formula.hit",
        &["attacker", "target"],
        None,
        |w, names| {
            names.define(w, "chance", HIT_CHANCE);
            for (name, text) in HIT_STEPS {
                names.define(w, name, text);
            }
            names.write(w, HIT)
        },
    );
}

fn exp(g: &mut Graph, top: f32) {
    // Experience is the life a hit takes (battle exp §1); a kill adds 5% of the monster's.
    formula(
        g,
        top,
        "formula.hit_exp",
        &["attacker", "target", "damage"],
        None,
        |w, names| {
            names.define(w, "base", "damage");
            for (name, text) in EXP_STEPS {
                names.define(w, name, text);
            }
            names.write(w, EXP)
        },
    );
    formula(
        g,
        top + 150.0,
        "formula.kill_exp",
        &["attacker", "target"],
        None,
        |w, names| {
            names.define(w, "base", "target.life * 5 / 100");
            for (name, text) in EXP_STEPS {
                names.define(w, name, text);
            }
            names.write(w, EXP)
        },
    );
}

fn can_wear(g: &mut Graph, top: f32) {
    formula(
        g,
        top,
        "formula.can_wear",
        &["player", "item"],
        Some("player"),
        |w, names| {
            names.define(w, "class", CLASS);
            // Conquer's male bodies are 3 and 4: TQ's sex 1; female 2.
            names.define(w, "sex", "if(body > 2, 1, 2)");
            let ok = names.write(w, CAN_WEAR);
            w.write("if(ok, 1, 0)", &mut |_, n, _| {
                (n == "ok").then(|| ok.clone())
            })
            .expect("a plain formula")
        },
    );
}

/// Levelling up (and a new character's first login) sets the class's points for the level.
fn level_up(g: &mut Graph, top: f32) {
    let up = node(g, "event.level_up", [0.0, top], &[]);
    let call = node(g, "flow.call", [260.0, top], &[("name", text("allot"))]);
    wire(g, (up, "then"), (call, "exec"));
    let login = node(g, "event.login", [0.0, top + 150.0], &[]);
    let mut w = ExprWriter::new(g, [260.0, top + 150.0]);
    let names = Names {
        pins: Vec::new(),
        default_record: None,
        lets: Vec::new(),
    };
    let fresh = w
        .write(
            "level == 1 && strength + agility + vitality + spirit == 0",
            &mut |w, n, _| player_value(w, &names, n),
        )
        .expect("a plain formula");
    let branch = w.node("flow.branch", &[]);
    w.feed(&fresh, branch, "condition");
    let call = w.node("flow.call", &[("name", text("allot"))]);
    // Logging in also starts the XP bar's clock.
    let seq = w.node("flow.sequence", &[]);
    let clock = w.node(
        "player.start_timer",
        &[("name", text(XP_TIMER)), ("seconds", Literal::Number(30.0))],
    );
    wire(g, (login, "then"), (seq, "exec"));
    wire(g, (seq, "then 0"), (branch, "exec"));
    wire(g, (branch, "true"), (call, "exec"));
    wire(g, (seq, "then 1"), (clock, "exec"));
}

/// The XP bar's timer.
const XP_TIMER: &str = "xp";

/// The XP bar (progression §5.5, battle M.11): every 30 seconds from level 3 it fills by 10;
/// Experience a weapon skill level needs (battle exp §9, TQ's NEED table, levels 1–19).
const WEAPON_NEED: &str = "if(lvl == 1, 1200, if(lvl == 2, 68000, if(lvl == 3, 250000, if(lvl == 4, 640000, if(lvl == 5, 1600000, if(lvl == 6, 4000000, if(lvl == 7, 10000000, if(lvl == 8, 22000000, if(lvl == 9, 40000000, if(lvl == 10, 90000000, if(lvl == 11, 95000000, if(lvl == 12, 142500000, if(lvl == 13, 213750000, if(lvl == 14, 320625000, if(lvl == 15, 480937500, if(lvl == 16, 721406250, if(lvl == 17, 1082109375, if(lvl == 18, 1623164062, if(lvl == 19, 2100000000, 0)))))))))))))))))))";

/// The hand whose weapon skill a hit teaches.
#[derive(Clone, Copy, PartialEq)]
enum WeaponHand {
    /// Its weapon's skill, or bare hands' (skill 0).
    Right,
    /// A weapon there learns half.
    Left,
}

/// Each hit teaches both hands' weapon skills.
fn weapon_skills(g: &mut Graph, top: f32) {
    let event = node(g, "event.hit", [0.0, top], &[]);
    let seq = node(g, "flow.sequence", [130.0, top], &[]);
    wire(g, (event, "then"), (seq, "exec"));
    weapon_skill(g, top, event, (seq, "then 0"), WeaponHand::Right);
    weapon_skill(g, top + 900.0, event, (seq, "then 1"), WeaponHand::Left);
}

/// Weapon skills learn from hits (battle exp §9): the life a hit took, Violet gems' percent
/// more, less above level 12, into the skill of the hand's weapon, which a first hit learns at
/// level 1; past a level's need the skill goes up (to 20).
fn weapon_skill(g: &mut Graph, top: f32, event: u32, from: (u32, &'static str), hand: WeaponHand) {
    let names = Names {
        pins: vec![("damage", event), ("player", event)],
        default_record: None,
        lets: Vec::new(),
    };
    let mut w = ExprWriter::new(g, [260.0, top]);
    let player = Operand::Pin {
        node: event,
        pin: "player".into(),
    };
    let mut names = names;
    let (worn, share) = match hand {
        WeaponHand::Right => ("player.worn4", "100"),
        WeaponHand::Left => ("player.worn5", "50"),
    };
    names.define(&mut w, "t", &format!("{worn} / 1000"));
    names.define(&mut w, "share", share);
    let t = names.resolve(&mut w, "t").expect("defined");
    // The skill's level and experience: attributes weapon_<t> and weapon_exp_<t>.
    let named = |w: &mut ExprWriter, prefix: &str| {
        let n = w.node("record.get_named", &[("prefix", text(prefix))]);
        w.feed(&player, n, "record");
        w.feed(&t, n, "key");
        Operand::Pin {
            node: n,
            pin: "value".into(),
        }
    };
    let lvl = named(&mut w, "weapon_");
    let xp = named(&mut w, "weapon_exp_");
    names.lets.push(("lvl".into(), lvl));
    names.lets.push(("xp".into(), xp));
    names.define(
        &mut w,
        "gained",
        "damage * (100 + player.socket_gem_skill_exp) / 100 * share / 100",
    );
    names.define(
        &mut w,
        "e",
        "if(lvl > 12, gained * max(10, (17 - lvl) * 20) / 100 / 2, gained)",
    );
    names.define(&mut w, "new", "min(xp + e, 2147483647)");
    names.define(&mut w, "need", WEAPON_NEED);
    let attr_name = |w: &mut ExprWriter, prefix: &str| {
        let n = w.node("text.number", &[("text", text(prefix))]);
        w.feed(&t, n, "number");
        Operand::Pin {
            node: n,
            pin: "value".into(),
        }
    };
    let level_name = attr_name(&mut w, "weapon_");
    let exp_name = attr_name(&mut w, "weapon_exp_");
    let set = |w: &mut ExprWriter, names: &Names, name: &Operand, value: &str| {
        let v = names.write(w, value);
        let n = w.node("player.set_attr", &[]);
        w.feed(name, n, "name");
        w.feed(&v, n, "value");
        n
    };
    let new_skill = branch_on(&mut w, &names, "lvl == 0");
    let learn_level = set(&mut w, &names, &level_name, "1");
    let learn_exp = set(&mut w, &names, &exp_name, "gained");
    let grows = branch_on(&mut w, &names, "lvl <= 19");
    let up = branch_on(&mut w, &names, "new > need");
    let level_up = set(&mut w, &names, &level_name, "lvl + 1");
    let reset = set(&mut w, &names, &exp_name, "0");
    let said = w.node(
        "player.message",
        &[("text", text("Your weapon skill went up a level."))],
    );
    let keep = set(&mut w, &names, &exp_name, "new");
    // The left hand learns only with a one-handed weapon in it; arrows (50) never.
    let held = branch_on(
        &mut w,
        &names,
        match hand {
            WeaponHand::Right => "t != 50",
            WeaponHand::Left => "t / 100 == 4",
        },
    );
    wire(g, from, (held, "exec"));
    wire(g, (held, "true"), (new_skill, "exec"));
    chain(g, (new_skill, "true"), &[learn_level, learn_exp]);
    wire(g, (new_skill, "false"), (grows, "exec"));
    wire(g, (grows, "true"), (up, "exec"));
    chain(g, (up, "true"), &[level_up, reset, said]);
    wire(g, (up, "false"), (keep, "exec"));
}

/// TQ's jail (pk bonus spec §2.1 B): a red-named player (PK 30 or more) killed by a player,
/// outside map 6002, comes back in the black jail (map 6000 at its portal, 29,72) and a
/// bounty is recorded on the killer.
fn red_name_jailed(g: &mut Graph, top: f32) {
    let event = node(g, "event.killed_by_player", [0.0, top], &[]);
    let names = Names {
        pins: vec![("killer", event)],
        default_record: None,
        lets: Vec::new(),
    };
    let mut w = ExprWriter::new(g, [260.0, top]);
    let red = branch_on(&mut w, &names, "pk >= 30 && map != 6002");
    wire(g, (event, "then"), (red, "exec"));
    let record = node(g, "bounty.record", [780.0, top], &[]);
    wire(g, (event, "killer"), (record, "killer"));
    let jail = node(
        g,
        "player.set_record",
        [1040.0, top],
        &[
            ("map", Literal::Number(6000.0)),
            ("x", Literal::Number(29.0)),
            ("y", Literal::Number(72.0)),
        ],
    );
    chain(g, (red, "true"), &[record, jail]);
}

/// A wanted target killed (pk bonus spec §4.3): the hunter who took the entry on is paid 70%
/// of its bounty, and the entry and the hunter's order end.
fn wanted_hunt(g: &mut Graph, top: f32) {
    let order_flag = crate::npc_logic::WANTED_ORDER;
    let event = node(g, "event.kill_player", [0.0, top], &[]);
    let order = node(
        g,
        "player.flag",
        [260.0, top],
        &[("name", text(order_flag))],
    );
    let target = node(g, "wanted.target", [520.0, top], &[]);
    wire(g, (order, "value"), (target, "id"));
    let same = node(g, "text.equal", [780.0, top], &[]);
    wire(g, (target, "value"), (same, "a"));
    wire(g, (event, "victim"), (same, "b"));
    let branch = node(g, "flow.branch", [1040.0, top], &[]);
    wire(g, (same, "value"), (branch, "condition"));
    wire(g, (event, "then"), (branch, "exec"));
    let bounty = node(g, "wanted.bounty", [520.0, top + 160.0], &[]);
    wire(g, (order, "value"), (bounty, "id"));
    let share = node(
        g,
        "math.multiply",
        [780.0, top + 160.0],
        &[("b", Literal::Number(70.0))],
    );
    wire(g, (bounty, "value"), (share, "a"));
    let paid = node(
        g,
        "math.divide",
        [1040.0, top + 160.0],
        &[("b", Literal::Number(100.0))],
    );
    wire(g, (share, "value"), (paid, "a"));
    let give = node(g, "player.give_money", [1300.0, top], &[]);
    wire(g, (paid, "value"), (give, "amount"));
    let remove = node(
        g,
        "wanted.set_bounty",
        [1560.0, top],
        &[("bounty", Literal::Number(0.0))],
    );
    wire(g, (order, "value"), (remove, "id"));
    let clear = node(
        g,
        "player.set_flag",
        [1820.0, top],
        &[("name", text(order_flag)), ("value", Literal::Number(0.0))],
    );
    chain(g, (branch, "true"), &[give, remove, clear]);
}

/// Skills learn from casts (battle M.7): what a cast did goes into the skill's experience
/// (attribute `magic_exp_<type>`, its level `magic_<type>`); past the level's need, at the
/// player level it needs, the skill goes up a level and the leftover is lost.
fn magic_skills(g: &mut Graph, top: f32) {
    let event = node(g, "event.cast", [0.0, top], &[]);
    let mut names = Names {
        pins: vec![("skill", event), ("amount", event), ("player", event)],
        default_record: None,
        lets: Vec::new(),
    };
    let mut w = ExprWriter::new(g, [260.0, top]);
    let player = Operand::Pin {
        node: event,
        pin: "player".into(),
    };
    let skill = names.resolve(&mut w, "skill").expect("a pin");
    // The skill's level and experience, and those attributes' names.
    let pin = |node| Operand::Pin {
        node,
        pin: "value".into(),
    };
    let get = |w: &mut ExprWriter, prefix: &str| {
        let n = w.node("record.get_named", &[("prefix", text(prefix))]);
        w.feed(&player, n, "record");
        w.feed(&skill, n, "key");
        pin(n)
    };
    let name = |w: &mut ExprWriter, prefix: &str| {
        let n = w.node("text.number", &[("text", text(prefix))]);
        w.feed(&skill, n, "number");
        pin(n)
    };
    let lvl = get(&mut w, "magic_");
    let xp = get(&mut w, "magic_exp_");
    let level_name = name(&mut w, "magic_");
    let exp_name = name(&mut w, "magic_exp_");
    names.lets.push(("lvl".into(), lvl));
    names.lets.push(("xp".into(), xp));
    names.define(&mut w, "need", "magic_need_exp(skill * 100 + lvl)");
    names.define(&mut w, "need_level", "magic_need_level(skill * 100 + lvl)");
    names.define(&mut w, "new", "min(xp + amount, 2147483647)");
    let set = |w: &mut ExprWriter, names: &Names, name: &Operand, value: &str| {
        let v = names.write(w, value);
        let n = w.node("player.set_attr", &[]);
        w.feed(name, n, "name");
        w.feed(&v, n, "value");
        n
    };
    let grows = branch_on(&mut w, &names, "need > 0 && player.level >= need_level");
    let up = branch_on(&mut w, &names, "new >= need");
    let level_up = set(&mut w, &names, &level_name, "lvl + 1");
    let reset = set(&mut w, &names, &exp_name, "0");
    let said = w.node(
        "player.message",
        &[("text", text("Your skill went up a level."))],
    );
    let keep = set(&mut w, &names, &exp_name, "new");
    wire(g, (event, "then"), (grows, "exec"));
    wire(g, (grows, "true"), (up, "exec"));
    chain(g, (up, "true"), &[level_up, reset, said]);
    wire(g, (up, "false"), (keep, "exec"));
}

/// full, it empties at the next tick (a window to use an XP skill); each kill adds 1 below
/// 99. XP skills spend the full bar.
fn xp_bar(g: &mut Graph, top: f32) {
    let timer = node(g, "event.timer", [0.0, top], &[]);
    let names = Names {
        pins: vec![("name", timer)],
        default_record: None,
        lets: Vec::new(),
    };
    let mut w = ExprWriter::new(g, [260.0, top]);
    let is_xp = w.node("text.equal", &[("b", text(XP_TIMER))]);
    w.feed(
        &Operand::Pin {
            node: timer,
            pin: "name".into(),
        },
        is_xp,
        "a",
    );
    let ours = w.node("flow.branch", &[]);
    w.feed(
        &Operand::Pin {
            node: is_xp,
            pin: "value".into(),
        },
        ours,
        "condition",
    );
    let grown = branch_on(&mut w, &names, "level >= 3");
    let full = branch_on(&mut w, &names, "xp >= 100");
    let empty = w.node(
        "player.set_attr",
        &[("name", text("xp")), ("value", Literal::Number(0.0))],
    );
    let fill = w.node(
        "player.add_attr",
        &[("name", text("xp")), ("amount", Literal::Number(10.0))],
    );
    let again = w.node(
        "player.start_timer",
        &[("name", text(XP_TIMER)), ("seconds", Literal::Number(30.0))],
    );
    let seq = w.node("flow.sequence", &[]);
    wire(g, (timer, "then"), (ours, "exec"));
    wire(g, (ours, "true"), (seq, "exec"));
    wire(g, (seq, "then 0"), (grown, "exec"));
    wire(g, (grown, "true"), (full, "exec"));
    wire(g, (full, "true"), (empty, "exec"));
    wire(g, (full, "false"), (fill, "exec"));
    wire(g, (seq, "then 1"), (again, "exec"));
    // A kill adds 1.
    let kill = node(g, "event.kill", [0.0, top + 300.0], &[]);
    let mut w = ExprWriter::new(g, [260.0, top + 300.0]);
    let below = branch_on(&mut w, &names, "xp < 99");
    let one = w.node(
        "player.add_attr",
        &[("name", text("xp")), ("amount", Literal::Number(1.0))],
    );
    wire(g, (kill, "then"), (below, "exec"));
    wire(g, (below, "true"), (one, "exec"));
}

/// TQ's jail (map 6000, at its first portal): red and black names killed by guards or
/// players go there (battle spec S8).
const JAIL: (i64, i64, i64) = (6000, 29, 72);

/// Exec wires `from` → each of `nodes` in turn; the last one's `then`.
fn chain(g: &mut Graph, from: (u32, &'static str), nodes: &[u32]) -> (u32, &'static str) {
    let mut at = from;
    for &n in nodes {
        wire(g, at, (n, "exec"));
        at = (n, "then");
    }
    at
}

/// A Branch on `text` (a player's values by name) from `from`; its two ways out.
fn branch_on(w: &mut ExprWriter, names: &Names, text: &str) -> u32 {
    let cond = w
        .write(text, &mut |w, n, _| player_value(w, names, n))
        .unwrap_or_else(|e| panic!("Conquer's formula \"{text}\": {e}"));
    let branch = w.node("flow.branch", &[]);
    w.feed(&cond, branch, "condition");
    branch
}

/// An action node `def` whose input `pin` is `text` worked out (a player's values by name).
fn action_of(w: &mut ExprWriter, names: &Names, def: &str, pin: &str, text: &str) -> u32 {
    let value = w
        .write(text, &mut |w, n, _| player_value(w, names, n))
        .unwrap_or_else(|e| panic!("Conquer's formula \"{text}\": {e}"));
    let node = w.node(def, &[]);
    w.feed(&value, node, pin);
    node
}

/// Dying (battle spec S8): killed by a monster, a player of level 16 or more drops a third of
/// its money at most and each bag item with a third's chance (a twentieth under level 10);
/// a red or black name a guard kills goes to jail. Killed by a player, it drops more the
/// more PK points it has, loses experience (more for red and black names) and red and black
/// names go to jail; on free-PK maps only some bag items fall.
fn death(g: &mut Graph, top: f32) {
    let event = node(g, "event.death", [0.0, top], &[]);
    let names = Names {
        pins: vec![("killer", event)],
        default_record: None,
        lets: Vec::new(),
    };
    let mut w = ExprWriter::new(g, [260.0, top]);
    let by_monster = branch_on(&mut w, &names, "killed_by_monster != 0");
    // A monster's kill.
    let grown = branch_on(&mut w, &names, "level > 15");
    let money = action_of(
        &mut w,
        &names,
        "player.drop_money",
        "amount",
        "random(0, money / 3)",
    );
    let items = action_of(
        &mut w,
        &names,
        "player.drop_bag_items",
        "percent",
        "if(level < 10, 5, 33)",
    );
    let guard = branch_on(&mut w, &names, "pk >= 30 && (guard / 8) % 4 != 0");
    // By a player.
    let by_player = branch_on(&mut w, &names, "killer != 0");
    let free = branch_on(&mut w, &names, "pk_free");
    let free_items = w.node(
        "player.drop_bag_items",
        &[("percent", Literal::Number(30.0))],
    );
    let pk_money = action_of(
        &mut w,
        &names,
        "player.drop_money",
        "amount",
        "money * min(pk + 10, 95) / 100",
    );
    let pk_items = action_of(
        &mut w,
        &names,
        "player.drop_bag_items",
        "percent",
        "min(pk / 2 + 20, 90)",
    );
    let lost = action_of(
        &mut w,
        &names,
        "player.lose_exp",
        "amount",
        "exp / if(pk >= 100, 5, if(pk >= 30, 10, 50))",
    );
    let evil = branch_on(&mut w, &names, "pk >= 30");
    let jail = |w: &mut ExprWriter| {
        let (map, x, y) = JAIL;
        let values = [
            ("map", Literal::Number(map as f64)),
            ("x", Literal::Number(x as f64)),
            ("y", Literal::Number(y as f64)),
        ];
        let record = w.node("player.set_record", &values);
        let go = w.node("player.teleport", &values);
        (record, go)
    };
    let (guard_record, guard_go) = jail(&mut w);
    let (pk_record, pk_go) = jail(&mut w);
    wire(g, (event, "then"), (by_monster, "exec"));
    wire(g, (by_monster, "true"), (grown, "exec"));
    chain(g, (grown, "true"), &[money, items]);
    wire(g, (grown, "false"), (guard, "exec"));
    wire(g, (items, "then"), (guard, "exec"));
    chain(g, (guard, "true"), &[guard_record, guard_go]);
    wire(g, (by_monster, "false"), (by_player, "exec"));
    wire(g, (by_player, "true"), (free, "exec"));
    wire(g, (free, "true"), (free_items, "exec"));
    let after = chain(g, (free, "false"), &[pk_money, pk_items, lost]);
    wire(g, after, (evil, "exec"));
    chain(g, (evil, "true"), &[pk_record, pk_go]);
}

/// TQ's upgrade stones.
const METEOR: i64 = 1_088_001;
const METEOR_TEAR: i64 = 1_088_002;
const DRAGON_BALL: i64 = 1_088_000;

/// A Meteor's next level for `t` (items §2.1: the next type +10 that exists; three tried).
const NEXT_LEVEL: &str =
    "if(exists(t + 10), t + 10, if(exists(t + 20), t + 20, if(exists(t + 30), t + 30, 0)))";
/// Helmets, armour and shields go by their last level digit, others by two (items §2.2).
const HAS: &str = "t / 10000 == 11 || t / 10000 == 13 || t / 10000 == 14 || t / 100000 == 9";
/// The Meteor's chance (items §2.2): by level, less for better qualities, 1–100, a
/// MeteorTear 30% more.
const METEOR_CHANCE: &str = "min(max(if(has, if(lv == 5, 50, if(lv == 6, 40, if(lv == 7, 30, if(lv == 8 || lv == 9, 20, 500)))), if(lv >= 11 && lv <= 22, 95 - (lv - 11) * 5, 500)) * if(q == 6, 90, if(q == 7, 70, if(q == 8, 30, if(q == 9, 10, 100)))) / 100, 1), 100) * if(used == 1088002, 130, 100) / 100";
/// A DragonBall's next quality (items §3.1): 3, 4 and 5 become 6, then 7, 8, 9.
const NEXT_QUALITY: &str = "if(q < 5, t / 10 * 10 + 5, t) + 1";
/// Its chance (items §3.2): by quality, less above level 70.
const QUALITY_CHANCE: &str = "max(if(q == 6, 50, if(q == 7, 33, if(q == 8, 20, 100))) * if(need > 70, 100 - (need - 70), 100) / 100, 1)";

/// Upgrade stones used on an item (items §2.3, §3.3): a Meteor (or MeteorTear) raises its
/// level, a DragonBall its quality; the stone is spent, then the chance rolled.
fn upgrades(g: &mut Graph, top: f32) {
    let event = node(g, "event.use_on", [0.0, top], &[]);
    let used = Operand::Pin {
        node: event,
        pin: "used".into(),
    };
    let target = Operand::Pin {
        node: event,
        pin: "target".into(),
    };
    let mut w = ExprWriter::new(g, [260.0, top]);
    let t = {
        let kind = w.node("held.type", &[]);
        w.feed(&target, kind, "item");
        Operand::Pin {
            node: kind,
            pin: "value".into(),
        }
    };
    let mut lets: Vec<(String, Operand)> = vec![("used".into(), used.clone()), ("t".into(), t)];
    let write = |w: &mut ExprWriter, lets: &mut Vec<(String, Operand)>, name: &str, text: &str| {
        let value = w
            .write(text, &mut |w, n, args| upgrade_name(w, lets, n, args))
            .unwrap_or_else(|e| panic!("Conquer's formula \"{text}\": {e}"));
        if !name.is_empty() {
            lets.push((name.to_owned(), value.clone()));
        }
        value
    };
    write(&mut w, &mut lets, "q", "t % 10");
    let is_meteor = write(
        &mut w,
        &mut lets,
        "",
        &format!("used == {METEOR} || used == {METEOR_TEAR}"),
    );
    let is_ball = write(&mut w, &mut lets, "", &format!("used == {DRAGON_BALL}"));
    write(&mut w, &mut lets, "next", NEXT_LEVEL);
    write(&mut w, &mut lets, "has", HAS);
    write(
        &mut w,
        &mut lets,
        "lv",
        "if(has, t % 100 / 10, t % 1000 / 10)",
    );
    write(&mut w, &mut lets, "chance", METEOR_CHANCE);
    let meteor_ok = write(&mut w, &mut lets, "", "next != 0 && q >= 3");
    let meteor_roll = write(&mut w, &mut lets, "", "random(0, 99) < chance");
    write(&mut w, &mut lets, "next_q", NEXT_QUALITY);
    write(&mut w, &mut lets, "need", "required(t)");
    write(&mut w, &mut lets, "q_chance", QUALITY_CHANCE);
    let ball_ok = write(&mut w, &mut lets, "", "q >= 3 && q <= 8 && exists(next_q)");
    let ball_roll = write(&mut w, &mut lets, "", "random(0, 99) < q_chance");
    let get = |name: &str| {
        lets.iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
            .expect("written above")
    };
    let (next, next_q) = (get("next"), get("next_q"));

    let branch = |w: &mut ExprWriter, cond: &Operand| {
        let b = w.node("flow.branch", &[]);
        w.feed(cond, b, "condition");
        b
    };
    let say = |w: &mut ExprWriter, line: &str| w.node("player.message", &[("text", text(line))]);
    // Spends the stone, rolls, and on success changes the item's type to `to`.
    let attempt = |w: &mut ExprWriter, roll: &Operand, to: &Operand, done: &str| {
        let take = w.node("item.take", &[("count", Literal::Number(1.0))]);
        w.feed(&used, take, "item");
        let lucky = w.node("flow.branch", &[]);
        w.feed(roll, lucky, "condition");
        let set = w.node("held.set_type", &[]);
        w.feed(&target, set, "item");
        w.feed(to, set, "type");
        let won = w.node("player.message", &[("text", text(done))]);
        let lost = w.node(
            "player.message",
            &[("text", text("The upgrade failed; the stone is gone."))],
        );
        (take, lucky, set, won, lost)
    };
    let meteor = branch(&mut w, &is_meteor);
    let meteor_can = branch(&mut w, &meteor_ok);
    let (m_take, m_lucky, m_set, m_won, m_lost) =
        attempt(&mut w, &meteor_roll, &next, "Your equipment's level rose!");
    let m_no = say(
        &mut w,
        "That can't be raised with a Meteor (quality too low, or the highest level).",
    );
    let ball = branch(&mut w, &is_ball);
    let ball_can = branch(&mut w, &ball_ok);
    let (b_take, b_lucky, b_set, b_won, b_lost) = attempt(
        &mut w,
        &ball_roll,
        &next_q,
        "Your equipment's quality rose!",
    );
    let b_no = say(&mut w, "That can't be improved with a DragonBall.");

    wire(g, (event, "then"), (meteor, "exec"));
    wire(g, (meteor, "true"), (meteor_can, "exec"));
    chain(g, (meteor_can, "true"), &[m_take]);
    wire(g, (m_take, "then"), (m_lucky, "exec"));
    chain(g, (m_lucky, "true"), &[m_set, m_won]);
    wire(g, (m_lucky, "false"), (m_lost, "exec"));
    wire(g, (meteor_can, "false"), (m_no, "exec"));
    wire(g, (meteor, "false"), (ball, "exec"));
    wire(g, (ball, "true"), (ball_can, "exec"));
    chain(g, (ball_can, "true"), &[b_take]);
    wire(g, (b_take, "then"), (b_lucky, "exec"));
    chain(g, (b_lucky, "true"), &[b_set, b_won]);
    wire(g, (b_lucky, "false"), (b_lost, "exec"));
    wire(g, (ball_can, "false"), (b_no, "exec"));
}

/// Names the upgrade formulas use: values worked out before, `exists(type)` (the items table
/// has it) and `required(type)` (its level).
fn upgrade_name(
    w: &mut ExprWriter,
    lets: &[(String, Operand)],
    name: &str,
    args: &[Operand],
) -> Option<Operand> {
    if let Some((_, v)) = lets.iter().find(|(n, _)| n == name) {
        return Some(v.clone());
    }
    let items = text(c3_play::data_rules::ITEMS);
    let node = match (name, args) {
        ("exists", [kind]) => {
            let n = w.node("table.has_row", &[("table", items)]);
            w.feed(kind, n, "key");
            n
        }
        ("required", [kind]) => {
            let n = w.node(
                "table.lookup",
                &[("table", items), ("column", text("requires_level"))],
            );
            w.feed(kind, n, "key");
            n
        }
        _ => return None,
    };
    Some(Operand::Pin {
        node,
        pin: "value".into(),
    })
}

/// Points reset (action 1047, rebirths only): the class's starting strength, agility,
/// vitality and spirit, and every point above them (less 10) to spend again.
fn reset_points(g: &mut Graph, top: f32) {
    let signal = node(
        g,
        "event.signal",
        [0.0, top],
        &[("name", text("reset_points"))],
    );
    let mut names = Names {
        pins: Vec::new(),
        default_record: None,
        lets: Vec::new(),
    };
    let mut w = ExprWriter::new(g, [260.0, top]);
    let reborn = branch_on(&mut w, &names, "rebirths > 0");
    // The player's values by name, and `class`.
    let write = |w: &mut ExprWriter, names: &Names, text: &str| {
        w.write(text, &mut |w, n, _| player_value(w, names, n))
            .unwrap_or_else(|e| panic!("Conquer's formula \"{text}\": {e}"))
    };
    let class = write(&mut w, &names, CLASS);
    names.lets.push(("class".into(), class));
    let free = write(
        &mut w,
        &names,
        "max(strength + agility + vitality + spirit + attribute_points - 10, 0)",
    );
    let set = |w: &mut ExprWriter, name: &str, value: &Operand| {
        let n = w.node("player.set_attr", &[("name", text(name))]);
        w.feed(value, n, "value");
        n
    };
    let mut steps = vec![set(&mut w, "attribute_points", &free)];
    for (attr, formula) in [
        ("strength", "if(class == 4, 2, if(class >= 10, 0, 5))"),
        ("agility", "if(class == 4, 7, 2)"),
        ("vitality", "if(class == 4, 1, 3)"),
        ("spirit", "if(class >= 10, 5, 0)"),
    ] {
        let v = write(&mut w, &names, formula);
        steps.push(set(&mut w, attr, &v));
    }
    wire(g, (signal, "then"), (reborn, "exec"));
    chain(g, (reborn, "true"), &steps);
}

/// A lottery draw (action 1508), the `lottery` signal with the draw's number: a prize drawn
/// by the prizes' weights, given with its colour, open sockets and plus, and named.
fn lottery(g: &mut Graph, top: f32) {
    let signal = node(g, "event.signal", [0.0, top], &[("name", text("lottery"))]);
    let mut w = ExprWriter::new(g, [260.0, top]);
    let table = text(crate::rules::LOTTERY_TABLE);
    let pick = w.node(
        "table.pick",
        &[
            ("table", table.clone()),
            ("column", text("draw")),
            ("weight", text("chance")),
        ],
    );
    w.feed(
        &Operand::Pin {
            node: signal,
            pin: "a".into(),
        },
        pick,
        "value",
    );
    let key = Operand::Pin {
        node: pick,
        pin: "key".into(),
    };
    let column = |w: &mut ExprWriter, name: &str| {
        let n = w.node(
            "table.lookup",
            &[("table", table.clone()), ("column", text(name))],
        );
        w.feed(&key, n, "key");
        Operand::Pin {
            node: n,
            pin: "value".into(),
        }
    };
    let item = column(&mut w, "item");
    let won = w
        .write("k != 0", &mut |_, n, _| (n == "k").then(|| key.clone()))
        .expect("a plain formula");
    let branch = w.node("flow.branch", &[]);
    w.feed(&won, branch, "condition");
    let give = w.node("item.give", &[("count", Literal::Number(1.0))]);
    w.feed(&item, give, "item");
    let mut steps = vec![give];
    let sockets = column(&mut w, "sockets");
    for (name, value) in [
        ("color", column(&mut w, "color")),
        ("plus", column(&mut w, "plus")),
        (
            "gem1",
            w.write("if(s >= 1, 255, 0)", &mut |_, n, _| {
                (n == "s").then(|| sockets.clone())
            })
            .expect("a plain formula"),
        ),
        (
            "gem2",
            w.write("if(s >= 2, 255, 0)", &mut |_, n, _| {
                (n == "s").then(|| sockets.clone())
            })
            .expect("a plain formula"),
        ),
    ] {
        let set = w.node("item.set_given_attr", &[("name", text(name))]);
        w.feed(&item, set, "item");
        w.feed(&value, set, "value");
        steps.push(set);
    }
    let name = column(&mut w, "name");
    let said = {
        let line = w.node("text.join", &[("a", text("You won "))]);
        w.feed(&name, line, "b");
        let say = w.node("player.message", &[]);
        w.feed(
            &Operand::Pin {
                node: line,
                pin: "value".into(),
            },
            say,
            "text",
        );
        say
    };
    steps.push(said);
    wire(g, (signal, "then"), (pick, "exec"));
    wire(g, (pick, "then"), (branch, "exec"));
    chain(g, (branch, "true"), &steps);
}

/// One way of working items together (items §6–7): its kind, the checks that must hold (and
/// what is said when one doesn't), and what is done, in order.
struct Work {
    kind: u32,
    checks: &'static [(&'static str, &'static str)],
    steps: &'static [Step],
}

/// A step of a [`Work`].
enum Step {
    /// Item attribute `name` of the item `item` names set to `value`.
    Set {
        item: &'static str,
        name: &'static str,
        value: &'static str,
    },
    /// The item `item` names erased, when `when` holds (empty: always).
    Erase {
        item: &'static str,
        when: &'static str,
    },
    /// The player told `text`.
    Say(&'static str),
}

/// A gem item (700xxx); an open socket holds 255.
const IS_GEM: &str = "t(gem) / 10000 == 70";

/// TQ's ways of working items together, as `Input::Combine` raises them.
const WORKS: [Work; 4] = {
    use crate::protocol::packets::combine;
    [
        // A gem set in an open socket (§6.1; no restriction of gem to item).
        Work {
            kind: combine::EMBED,
            checks: &[
                (IS_GEM, "That isn't a gem."),
                ("socket == 255", "That socket isn't open."),
            ],
            steps: &[
                Step::Set {
                    item: "item",
                    name: "socket",
                    value: "t(gem) % 1000",
                },
                Step::Erase {
                    item: "gem",
                    when: "",
                },
                Step::Say("The gem is set."),
            ],
        },
        // The gem in a socket taken out: destroyed, the socket open again.
        Work {
            kind: combine::REMOVE_GEM,
            checks: &[("socket != 0 && socket != 255", "There is no gem there.")],
            steps: &[
                Step::Set {
                    item: "item",
                    name: "socket",
                    value: "255",
                },
                Step::Say("The gem is taken out."),
            ],
        },
        // +N composing (§7.1): two minors of the same kind (or +N stones) at least the main's
        // plus; from +5 a gem, and for weapons two; no roll.
        Work {
            kind: combine::COMPOSE,
            checks: &[
                ("t(item) / 10000 != 73", "A stone can't be the main item."),
                ("plus(item) < 9", "It can't be raised any further."),
                (
                    "fits(item2) && fits(item3)",
                    "Those items don't go with it.",
                ),
                (
                    "plus(item2) > 0 && plus(item3) > 0 && plus(item2) >= plus(item) && plus(item3) >= plus(item)",
                    "The other items' plus must be at least the main item's.",
                ),
                (
                    "plus(item) < 5 || t(item4) / 10000 == 70 && (!weapon(item) || t(item5) / 10000 == 70)",
                    "Composing past +5 takes gems.",
                ),
            ],
            steps: &[
                Step::Erase {
                    item: "item2",
                    when: "",
                },
                Step::Erase {
                    item: "item3",
                    when: "",
                },
                Step::Erase {
                    item: "item4",
                    when: "plus(item) >= 5",
                },
                Step::Erase {
                    item: "item5",
                    when: "plus(item) >= 5 && weapon(item)",
                },
                Step::Set {
                    item: "item",
                    name: "plus",
                    value: "plus(item) + 1",
                },
                Step::Say("Composed."),
            ],
        },
        // A gem enchanting an item (§6.4): extra life by the gem's kind and grade, kept when
        // higher; the gem is spent either way.
        Work {
            kind: combine::ENCHANT,
            checks: &[(IS_GEM, "That isn't a gem.")],
            steps: &[
                Step::Set {
                    item: "item",
                    name: "enchant",
                    value: "max(enchant(item), roll)",
                },
                Step::Erase {
                    item: "gem",
                    when: "",
                },
                Step::Say("The item is enchanted."),
            ],
        },
    ]
};

/// What an enchanting gem gives: by its grade and kind (items §6.4 table).
const ENCHANT_ROLL: &str = "if(t(gem) % 10 == 1, random(1, 59), if(t(gem) % 10 == 2, random(e2, e2 + 49), random(e3, e3 + if(t(gem) / 10 % 10 == 1, 55, 59))))";
const ENCHANT_GRADE2: &str = "if(t(gem) / 10 % 10 == 0, 60, if(t(gem) / 10 % 10 == 1, 100, if(t(gem) / 10 % 10 == 3, 80, if(t(gem) / 10 % 10 == 5 || t(gem) / 10 % 10 == 6, 60, 40))))";
const ENCHANT_GRADE3: &str = "if(t(gem) / 10 % 10 == 1, 200, if(t(gem) / 10 % 10 == 2, 90, if(t(gem) / 10 % 10 == 4, 70, if(t(gem) / 10 % 10 == 5 || t(gem) / 10 % 10 == 6, 140, 170))))";

/// Items worked together (`on_combine`): each of [`WORKS`] on its kind, its checks in turn,
/// then its steps.
fn combine(g: &mut Graph, top: f32) {
    let event = node(g, "event.combine", [0.0, top], &[]);
    let pin = |p: &str| Operand::Pin {
        node: event,
        pin: p.into(),
    };
    let mut w = ExprWriter::new(g, [260.0, top]);
    let mut lets: Vec<(String, Operand)> = vec![
        ("kind".into(), pin("kind")),
        ("item".into(), pin("item1")),
        ("gem".into(), pin("item2")),
        ("pos".into(), pin("value")),
    ];
    for i in 2..=5 {
        lets.push((format!("item{i}"), pin(&format!("item{i}"))));
    }
    let write = |w: &mut ExprWriter, lets: &mut Vec<(String, Operand)>, name: &str, text: &str| {
        let value = w
            .write(text, &mut |w, n, args| item_name(w, lets, n, args))
            .unwrap_or_else(|e| panic!("Conquer's formula \"{text}\": {e}"));
        if !name.is_empty() {
            lets.push((name.to_owned(), value.clone()));
        }
        value
    };
    write(
        &mut w,
        &mut lets,
        "socket",
        "if(pos == 1, gem1(item), gem2(item))",
    );
    write(&mut w, &mut lets, "e2", ENCHANT_GRADE2);
    write(&mut w, &mut lets, "e3", ENCHANT_GRADE3);
    write(&mut w, &mut lets, "roll", ENCHANT_ROLL);
    let mut steps: Vec<((u32, &'static str), u32)> = Vec::new();
    let mut from = (event, "then");
    for work in &WORKS {
        let is = write(&mut w, &mut lets, "", &format!("kind == {}", work.kind));
        let which = w.node("flow.branch", &[]);
        w.feed(&is, which, "condition");
        steps.push((from, which));
        from = (which, "false");
        let mut at = (which, "true");
        for (check, refusal) in work.checks {
            let ok = write(&mut w, &mut lets, "", check);
            let branch = w.node("flow.branch", &[]);
            w.feed(&ok, branch, "condition");
            steps.push((at, branch));
            let say = w.node("player.message", &[("text", text(refusal))]);
            steps.push(((branch, "false"), say));
            at = (branch, "true");
        }
        for step in work.steps {
            let mut nodes = Vec::new();
            match step {
                Step::Set { item, name, value } => {
                    let id = write(&mut w, &mut lets, "", item);
                    // A socket: both written, only the one `pos` names changed.
                    let sets: Vec<(&str, String)> = match *name {
                        "socket" => vec![
                            ("gem1", format!("if(pos == 1, {value}, gem1({item}))")),
                            ("gem2", format!("if(pos != 1, {value}, gem2({item}))")),
                        ],
                        other => vec![(other, (*value).to_owned())],
                    };
                    for (attr, value) in sets {
                        let v = write(&mut w, &mut lets, "", &value);
                        let set = w.node("held.set_attr", &[("name", text(attr))]);
                        w.feed(&id, set, "item");
                        w.feed(&v, set, "value");
                        nodes.push(set);
                    }
                }
                // Not wanted: item 0, which erases nothing.
                Step::Erase { item, when } => {
                    let id = match *when {
                        "" => write(&mut w, &mut lets, "", item),
                        when => write(&mut w, &mut lets, "", &format!("if({when}, {item}, 0)")),
                    };
                    let erase = w.node("held.erase", &[]);
                    w.feed(&id, erase, "item");
                    nodes.push(erase);
                }
                Step::Say(line) => nodes.push(w.node("player.message", &[("text", text(line))])),
            }
            for node in nodes {
                steps.push((at, node));
                at = (node, "then");
            }
        }
    }
    for (from, to) in steps {
        wire(g, from, (to, "exec"));
    }
}

/// Names the item works use: their pins and values (`lets`), `t(x)` an item's type, `plus`,
/// `gem1`, `gem2`, `enchant` its numbers (a +N stone's plus is its grade), `weapon(x)`, and
/// `fits(x)`: item `x` may be composed into the main item.
fn item_name(
    w: &mut ExprWriter,
    lets: &[(String, Operand)],
    name: &str,
    args: &[Operand],
) -> Option<Operand> {
    if let Some((_, v)) = lets.iter().find(|(n, _)| n == name) {
        return Some(v.clone());
    }
    let pin = |node| Operand::Pin {
        node,
        pin: "value".into(),
    };
    let [x] = args else {
        return None;
    };
    let ty = |w: &mut ExprWriter| {
        let n = w.node("held.type", &[]);
        w.feed(x, n, "item");
        pin(n)
    };
    let derived = |w: &mut ExprWriter, text: &str| {
        let t = ty(w);
        w.write(text, &mut |w, n, a| match n {
            "x" => Some(x.clone()),
            "t" if a.is_empty() => Some(t.clone()),
            _ => item_name(w, lets, n, a),
        })
        .ok()
    };
    match name {
        "t" => Some(ty(w)),
        "plus" => derived(w, "if(t / 10000 == 73, t % 10, attr_plus(x))"),
        "weapon" => derived(w, "t / 100000 == 4 || t / 100000 == 5"),
        // A +N stone, or the main item's sort (weapons: one- or two-handed, bows only with
        // bows, backswords only with backswords; others their subtype).
        "fits" => {
            let main = lets
                .iter()
                .find(|(n, _)| n == "item")
                .map(|(_, v)| v.clone())?;
            let m = {
                let n = w.node("held.type", &[]);
                w.feed(&main, n, "item");
                pin(n)
            };
            let t = ty(w);
            w.write(
                "t / 10000 == 73 || (m / 100000 == 4 || m / 100000 == 5) && t / 100000 == m / 100000 && (m / 1000 != 500 || t / 1000 == 500) && (m / 1000 == 500 || t / 1000 != 500) && (m / 1000 != 421 || t / 1000 == 421) && (m / 1000 == 421 || t / 1000 != 421) || m / 100000 != 4 && m / 100000 != 5 && t / 1000 == m / 1000",
                &mut |_, n, _| match n {
                    "t" => Some(t.clone()),
                    "m" => Some(m.clone()),
                    _ => None,
                },
            )
            .ok()
        }
        _ => {
            let attr = name.strip_prefix("attr_").unwrap_or(name);
            if !matches!(attr, "plus" | "gem1" | "gem2" | "enchant" | "bless") {
                return None;
            }
            let n = w.node("held.attr", &[("name", text(attr))]);
            w.feed(x, n, "item");
            Some(pin(n))
        }
    }
}

/// The NPC upgrades of TQ's action 509, by the number the converter raises them with.
pub const UPEQUIPMENT: [&str; 6] = [
    "up_lev",
    "down_lev",
    "up_quality",
    "recover_dur",
    "up_levultra",
    "up_levultra2",
];

/// An item's lowest level (progression §8.3, items §2.7): helmets, armour and shields 0,
/// rings and boots 1, necklaces and weapons 2.
const LOWEST_LEVEL: &str = "if(t / 10000 == 11 || t / 10000 == 13 || t / 10000 == 14 || t / 100000 == 9, t - (t / 10 % 100) * 10, if(t / 10000 == 15 || t / 10000 == 16, t - (t / 10 % 100) * 10 + 10, if(t / 10000 == 12 || t / 100000 == 4 || t / 100000 == 5, t - (t / 10 % 100) * 10 + 20, t)))";

/// What an upgrade takes from the bag: items with types low..=high, and how many.
type Cost = (i64, i64, Operand);

/// TQ's NPC upgrades (action 509, items §2.4–2.7, §3.4, §5.2), the `upequipment` signal with
/// the upgrade's number (1 up_lev ... 6 up_levultra2), the slot and a flag: raising the worn
/// item's level for Meteors, its quality for DragonBalls, lowering its level, restoring its
/// durability, and the free raises.
fn upequipment(g: &mut Graph, top: f32) {
    let signal = node(
        g,
        "event.signal",
        [0.0, top],
        &[("name", text("upequipment"))],
    );
    let pin = |p: &str| Operand::Pin {
        node: signal,
        pin: p.into(),
    };
    let mut w = ExprWriter::new(g, [260.0, top]);
    let id = {
        let n = w.node("item.worn_id", &[]);
        w.feed(&pin("b"), n, "slot");
        Operand::Pin {
            node: n,
            pin: "value".into(),
        }
    };
    let t = {
        let n = w.node("held.type", &[]);
        w.feed(&id, n, "item");
        Operand::Pin {
            node: n,
            pin: "value".into(),
        }
    };
    let mut lets: Vec<(String, Operand)> = vec![
        ("cmd".into(), pin("a")),
        ("t".into(), t),
        ("used".into(), Operand::Number(0.0)),
    ];
    let write = |w: &mut ExprWriter, lets: &mut Vec<(String, Operand)>, name: &str, text: &str| {
        let value = w
            .write(text, &mut |w, n, args| {
                upgrade_name(w, lets, n, args).or_else(|| {
                    let names = Names {
                        pins: Vec::new(),
                        default_record: None,
                        lets: Vec::new(),
                    };
                    player_value(w, &names, n)
                })
            })
            .unwrap_or_else(|e| panic!("Conquer's formula \"{text}\": {e}"));
        if !name.is_empty() {
            lets.push((name.to_owned(), value.clone()));
        }
        value
    };
    write(&mut w, &mut lets, "q", "t % 10");
    write(&mut w, &mut lets, "next", NEXT_LEVEL);
    write(&mut w, &mut lets, "has", HAS);
    write(
        &mut w,
        &mut lets,
        "lv",
        "if(has, t % 100 / 10, t % 1000 / 10)",
    );
    write(&mut w, &mut lets, "chance", METEOR_CHANCE);
    write(&mut w, &mut lets, "next_q", NEXT_QUALITY);
    write(&mut w, &mut lets, "need", "required(t)");
    write(&mut w, &mut lets, "q_chance", QUALITY_CHANCE);
    let meteors = write(&mut w, &mut lets, "", "(100 / chance + 1) * 12 / 10");
    let balls = write(&mut w, &mut lets, "", "100 / q_chance + 1");
    let lowest = write(&mut w, &mut lets, "", LOWEST_LEVEL);
    let worn = write(&mut w, &mut lets, "", "t != 0");
    let plus10 = write(&mut w, &mut lets, "", "t + 10");
    let get = |name: &str| {
        lets.iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
            .expect("written")
    };
    let (next, next_q, t) = (get("next"), get("next_q"), get("t"));
    // Each upgrade: when its number and whether it may happen, what it costs, what it gives.
    let cases: [(i64, String, Option<Cost>, Operand); 5] = [
        (1, "next != 0 && q >= 3 && required(next) <= level".into(), Some((1_088_001, 1_088_002, meteors)), next.clone()),
        (2, "q != 0".into(), None, lowest),
        (3, "q >= 3 && q <= 8 && exists(next_q)".into(), Some((DRAGON_BALL, DRAGON_BALL, balls)), next_q),
        (5, "t / 100000 >= 4 && t / 100000 <= 5 && t / 10 % 100 >= 22 && t / 10 % 100 <= 32 && exists(t + 10) && required(t + 10) <= level".into(), None, plus10),
        (6, "next != 0 && q >= 3 && required(next) <= level".into(), None, next),
    ];
    let mut from = (signal, "then");
    let worn_branch = w.node("flow.branch", &[]);
    w.feed(&worn, worn_branch, "condition");
    let mut steps: Vec<((u32, &'static str), u32)> = vec![(from, worn_branch)];
    from = (worn_branch, "true");
    for (code, allowed, cost, to) in cases {
        let is = write(&mut w, &mut lets, "", &format!("cmd == {code}"));
        let which = w.node("flow.branch", &[]);
        w.feed(&is, which, "condition");
        steps.push((from, which));
        let may = write(&mut w, &mut lets, "", &allowed);
        let ok = w.node("flow.branch", &[]);
        w.feed(&may, ok, "condition");
        steps.push(((which, "true"), ok));
        let set = w.node("held.set_type", &[]);
        w.feed(&id, set, "item");
        w.feed(&to, set, "type");
        match cost {
            Some((low, high, count)) => {
                let take = w.node(
                    "item.take_between",
                    &[
                        ("low", Literal::Number(low as f64)),
                        ("high", Literal::Number(high as f64)),
                    ],
                );
                w.feed(&count, take, "count");
                let paid = w.node("flow.branch", &[]);
                w.feed(
                    &Operand::Pin {
                        node: take,
                        pin: "taken".into(),
                    },
                    paid,
                    "condition",
                );
                steps.push(((ok, "true"), take));
                steps.push(((take, "then"), paid));
                steps.push(((paid, "true"), set));
                let short = w.node(
                    "player.message",
                    &[("text", text("You haven't enough to pay for that."))],
                );
                steps.push(((paid, "false"), short));
            }
            None => steps.push(((ok, "true"), set)),
        }
        let refused = w.node(
            "player.message",
            &[("text", text("That can't be done to this item."))],
        );
        steps.push(((ok, "false"), refused));
        from = (which, "false");
    }
    // 4: restore the durability to the item's full.
    let is = write(&mut w, &mut lets, "", "cmd == 4");
    let which = w.node("flow.branch", &[]);
    w.feed(&is, which, "condition");
    steps.push((from, which));
    let full = {
        let n = w.node(
            "table.lookup",
            &[
                ("table", text(c3_play::data_rules::ITEMS)),
                ("column", text("durability")),
            ],
        );
        w.feed(&t, n, "key");
        Operand::Pin {
            node: n,
            pin: "value".into(),
        }
    };
    let restore = w.node("held.set_durability", &[]);
    w.feed(&id, restore, "item");
    w.feed(&full, restore, "durability");
    steps.push(((which, "true"), restore));
    for (from, to) in steps {
        wire(g, from, (to, "exec"));
    }
}

/// A rebirth's key in the rebirths table.
const REBIRTH_KEY: &str = "profession * 100000 + to * 10 + rule";
/// The points a rebirth leaves to spend (progression §8.2, RebirthPoints).
const REBIRTH_POINTS: &str = "if(rule == 1, if(profession == 135, ((min(level, 130) - 110) / 2 + 1) * ((min(level, 130) - 110) / 2) / 2 + 72, (min(level, 130) - 120) * (min(level, 130) - 119) / 2 + 72), max(min(max(level - 120, 0), 10) * (min(max(level - 120, 0), 10) + 1) / 2 + new_level * 3 - 3 + max(strength + agility + vitality + spirit + attribute_points - level * 3 - 7, 0), 0))";

/// Rebirth (progression §8), the signal NPCs raise with the new profession, look and rule:
/// a profession, level and rebirth count the rebirths table allows; the class's starting
/// points and the points left to spend; the new look, level and profession; worn equipment
/// back to its lowest level.
fn rebirth(g: &mut Graph, top: f32) {
    let signal = node(g, "event.signal", [0.0, top], &[("name", text("rebirth"))]);
    let pin = |p: &str| Operand::Pin {
        node: signal,
        pin: p.into(),
    };
    let mut names = Names {
        pins: Vec::new(),
        default_record: None,
        lets: vec![
            ("to".into(), pin("a")),
            ("look".into(), pin("b")),
            ("rule".into(), pin("c")),
        ],
    };
    let mut w = ExprWriter::new(g, [260.0, top]);
    let rebirths = text(crate::rules::REBIRTHS_TABLE);
    let write = |w: &mut ExprWriter, names: &mut Names, name: &str, text: &str| {
        let value = w
            .write(text, &mut |w, n, args| {
                if let Some((_, v)) = names.lets.iter().find(|(l, _)| l == n) {
                    return Some(v.clone());
                }
                rebirth_name(w, &rebirths, names, n, args)
            })
            .unwrap_or_else(|e| panic!("Conquer's formula \"{text}\": {e}"));
        if !name.is_empty() {
            names.lets.push((name.to_owned(), value.clone()));
        }
        value
    };
    write(&mut w, &mut names, "key", REBIRTH_KEY);
    write(&mut w, &mut names, "new_level", "column(key, 1)");
    let allowed = write(
        &mut w,
        &mut names,
        "",
        "has(key) && rebirths < rule && level >= column(key, 0)",
    );
    let points = write(&mut w, &mut names, "", REBIRTH_POINTS);
    write(&mut w, &mut names, "p", "if(to == 0, 20, to)");
    let stats = [
        ("strength", "if(p >= 100, 0, if(p / 10 == 4, 2, 5))"),
        ("agility", "if(p >= 100, 2, if(p / 10 == 4, 7, 2))"),
        ("vitality", "if(p >= 100, 3, if(p / 10 == 4, 1, 3))"),
        ("spirit", "if(p >= 100, 5, 0)"),
    ];
    let stats: Vec<(&str, Operand)> = (stats.iter())
        .map(|(name, text)| (*name, write(&mut w, &mut names, "", text)))
        .collect();
    let new_level = names
        .lets
        .iter()
        .find(|(n, _)| n == "new_level")
        .map(|(_, v)| v.clone())
        .expect("written");
    let body = write(&mut w, &mut names, "", "look % 1000");
    let branch = w.node("flow.branch", &[]);
    w.feed(&allowed, branch, "condition");
    // Points first (they count the old stats), then the rest.
    let mut steps = Vec::new();
    let set_attr = |w: &mut ExprWriter, name: &str, value: &Operand| {
        let n = w.node("player.set_attr", &[("name", text(name))]);
        w.feed(value, n, "value");
        n
    };
    steps.push(set_attr(&mut w, "attribute_points", &points));
    for (name, value) in &stats {
        steps.push(set_attr(&mut w, name, value));
    }
    let profession = w.node("player.set_profession", &[]);
    w.feed(&pin("a"), profession, "profession");
    let level = w.node("player.set_level", &[]);
    w.feed(&new_level, level, "level");
    let exp = w.node("player.set_exp", &[("exp", Literal::Number(0.0))]);
    let look = w.node("player.set_body", &[]);
    w.feed(&body, look, "body");
    // The profession changes last: the formulas above read the old one.
    steps.extend([level, exp, look]);
    steps.push(set_attr(&mut w, "rebirths", &pin("c")));
    // Worn equipment drops to its lowest level (progression §8.3).
    for slot in 1..=9 {
        let id = {
            let n = w.node(
                "item.worn_id",
                &[("slot", Literal::Number(f64::from(slot)))],
            );
            Operand::Pin {
                node: n,
                pin: "value".into(),
            }
        };
        let t = {
            let n = w.node("held.type", &[]);
            w.feed(&id, n, "item");
            Operand::Pin {
                node: n,
                pin: "value".into(),
            }
        };
        let down = w
            .write(
                "if(t / 10000 == 11 || t / 10000 == 13 || t / 10000 == 14 || t / 100000 == 9, t - (t / 10 % 100) * 10, if(t / 10000 == 15 || t / 10000 == 16, t - (t / 10 % 100) * 10 + 10, if(t / 10000 == 12 || t / 100000 == 4 || t / 100000 == 5, t - (t / 10 % 100) * 10 + 20, t)))",
                &mut |_, n, _| (n == "t").then(|| t.clone()),
            )
            .expect("a plain formula");
        let set = w.node("held.set_type", &[]);
        w.feed(&id, set, "item");
        w.feed(&down, set, "type");
        steps.push(set);
    }
    steps.push(profession);
    wire(g, (signal, "then"), (branch, "exec"));
    chain(g, (branch, "true"), &steps);
}

/// Names the rebirth formulas use: the player's values, `has(key)` and `column(key, i)` of
/// the rebirths table (0 the level needed, 1 the level after).
fn rebirth_name(
    w: &mut ExprWriter,
    table: &Literal,
    names: &Names,
    name: &str,
    args: &[Operand],
) -> Option<Operand> {
    let node = match (name, args) {
        ("has", [key]) => {
            let n = w.node("table.has_row", &[("table", table.clone())]);
            w.feed(key, n, "key");
            n
        }
        ("column", [key, Operand::Number(i)]) => {
            let column = if *i == 0.0 { "need_level" } else { "new_level" };
            let n = w.node(
                "table.lookup",
                &[("table", table.clone()), ("column", text(column))],
            );
            w.feed(key, n, "key");
            n
        }
        _ => return player_value(w, names, name),
    };
    Some(Operand::Pin {
        node,
        pin: "value".into(),
    })
}

/// Coming back to life fills the stamina (battle spec S9).
fn revive(g: &mut Graph, top: f32) {
    let event = node(g, "event.revive", [0.0, top], &[]);
    let fill = node(
        g,
        "player.add_attr",
        [260.0, top],
        &[
            ("name", text("stamina")),
            ("amount", Literal::Number(100.0)),
        ],
    );
    wire(g, (event, "then"), (fill, "exec"));
}

/// A player's own value by name, read by its node.
fn player_value(w: &mut ExprWriter, names: &Names, name: &str) -> Option<Operand> {
    let id = match name {
        "level" => w.node("player.level", &[]),
        "profession" => w.node("player.profession", &[]),
        "money" => w.node("player.money", &[]),
        "exp" => w.node("player.exp", &[]),
        "xp" => w.node("player.attr", &[("name", text("xp"))]),
        "pk" => w.node("player.pk", &[]),
        "map" => w.node("player.map", &[]),
        "pk_free" => w.node("world.map_pk_free", &[]),
        "killed_by_monster" => w.node("player.killed_by_monster", &[]),
        // The `attack_user` bits of the monster that killed the player.
        "guard" => {
            let kind = player_value(w, names, "killed_by_monster")?;
            let lookup = w.node(
                "table.lookup",
                &[
                    ("table", text(c3_play::data_rules::MONSTERS)),
                    ("column", text("attack_user")),
                ],
            );
            w.feed(&kind, lookup, "key");
            lookup
        }
        "strength" | "agility" | "vitality" | "spirit" | "rebirths" | "attribute_points" => {
            w.node("player.attr", &[("name", text(name))])
        }
        _ => return names.resolve(w, name),
    };
    Some(Operand::Pin {
        node: id,
        pin: "value".into(),
    })
}

/// Function `allot`: the class's points for the player's level (TQ's `AutoAllot`,
/// progression §4.2), or 3 points to spend when the table has no row for it.
fn allot() -> Function {
    let mut g = Graph::default();
    let entry = node(&mut g, "flow.entry", [0.0, 0.0], &[]);
    let mut w = ExprWriter::new(&mut g, [260.0, 200.0]);
    let names = Names {
        pins: Vec::new(),
        default_record: None,
        lets: Vec::new(),
    };
    let key = w
        .write(&format!("({CLASS}) * 1000 + level"), &mut |w, n, _| {
            player_value(w, &names, n)
        })
        .expect("a plain formula");
    let has = w.node("table.has_row", &[("table", text(POINTS_TABLE))]);
    w.feed(&key, has, "key");
    let branch = w.node("flow.branch", &[]);
    w.feed(
        &Operand::Pin {
            node: has,
            pin: "value".into(),
        },
        branch,
        "condition",
    );
    let mut from = (branch, "true");
    for attr in ["strength", "agility", "vitality", "spirit"] {
        let lookup = w.node(
            "table.lookup",
            &[("table", text(POINTS_TABLE)), ("column", text(attr))],
        );
        w.feed(&key, lookup, "key");
        let set = w.node("player.set_attr", &[("name", text(attr))]);
        w.feed(
            &Operand::Pin {
                node: lookup,
                pin: "value".into(),
            },
            set,
            "value",
        );
        w.graph.wires.push(Wire {
            from: PinRef {
                node: from.0,
                pin: from.1.to_owned(),
            },
            to: PinRef {
                node: set,
                pin: "exec".into(),
            },
        });
        from = (set, "then");
    }
    let points = w.node(
        "player.add_attr",
        &[
            ("name", text("attribute_points")),
            ("amount", Literal::Number(3.0)),
        ],
    );
    wire(&mut g, (entry, "then"), (branch, "exec"));
    wire(&mut g, (branch, "false"), (points, "exec"));
    Function {
        name: "allot".into(),
        inputs: Vec::new(),
        output: None,
        inline: false,
        graph: g,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rules_blueprint_compiles_and_runs() {
        let bp = rules_blueprint();
        let script = c3_script::blueprint::compile("conquer_rules", &bp).expect("compiles");
        // The disdain table's row for 5 levels above a monster: ×1.8 attack, −70% experience.
        let tables = std::env::temp_dir().join(format!("c3-conquer-rules-{}", std::process::id()));
        std::fs::create_dir_all(&tables).unwrap();
        let mut disdain = c3_data::Table::new(
            crate::rules::DISDAIN_TABLE,
            std::iter::once("key")
                .chain(crate::rules::DISDAIN_COLUMNS.iter().map(|(c, _)| *c))
                .map(|c| c3_data::Column::new(c, c3_data::ColumnKind::Int))
                .collect(),
        );
        let mut row = vec![c3_data::Value::Int(0); disdain.columns.len()];
        row[0] = c3_data::Value::Int(5);
        row[1] = c3_data::Value::Int(80);
        row[5] = c3_data::Value::Int(-70);
        row[6] = c3_data::Value::Int(-70);
        disdain.rows.push(row);
        let file = tables.join(format!("{}.table.ron", crate::rules::DISDAIN_TABLE));
        c3_data::save_table(&disdain, &file).unwrap();
        // No magic levels: skills don't level up.
        let levels = c3_data::Table::new(
            crate::rules::MAGIC_LEVELS_TABLE,
            ["key", "need_exp", "need_level"]
                .map(|c| c3_data::Column::new(c, c3_data::ColumnKind::Int))
                .to_vec(),
        );
        let file = tables.join(format!("{}.table.ron", crate::rules::MAGIC_LEVELS_TABLE));
        c3_data::save_table(&levels, &file).unwrap();
        let formulas = c3_script::Formulas::compile(&script)
            .expect("Rhai reads it")
            .with_tables(Some(tables.clone()));
        // A level-1 Trojan with 5 strength, 2 agility, 3 vitality: TQ's 114 life.
        let mut player = c3_script::rhai::Map::new();
        for (k, v) in [
            ("strength", 5_i64),
            ("agility", 2),
            ("vitality", 3),
            ("spirit", 0),
            ("profession", 10),
        ] {
            player.insert(k.into(), v.into());
        }
        let stats = formulas.map("player_stats", (1_i64, player)).unwrap();
        assert_eq!(c3_script::map_int(&stats, "life"), (5 + 2 + 3 * 8) * 3);
        let mut attacker = c3_script::rhai::Map::new();
        let mut target = c3_script::rhai::Map::new();
        for (k, v) in [
            ("monster", 0_i64),
            ("agility", 40),
            ("level", 10),
            ("roll", 30),
            ("chance", 0),
        ] {
            attacker.insert(k.into(), v.into());
        }
        for (k, v) in [
            ("monster", 1_i64),
            ("dodge", 50),
            ("defence", 4),
            ("level", 5),
        ] {
            target.insert(k.into(), v.into());
        }
        // (30 × 1.8) − 4 defence.
        let hit = formulas
            .int("hit", (attacker.clone(), target.clone()))
            .unwrap();
        assert_eq!(hit, 50);
        // 100 life taken, 70% less for the level difference.
        let exp = formulas
            .int("hit_exp", (attacker.clone(), target.clone(), 100_i64))
            .unwrap();
        assert_eq!(exp, 30);
        // A spell of power 100: ×1.8 for the level difference; a 150% physical skill: 50 × 1.5.
        let skill = |power: i64| {
            let mut skill = c3_script::rhai::Map::new();
            skill.insert("power".into(), power.into());
            skill
        };
        let spell = (attacker.clone(), target.clone(), skill(100));
        assert_eq!(formulas.int("skill_hit", spell).unwrap(), 180);
        let strike = (attacker.clone(), target.clone(), skill(30_150));
        assert_eq!(formulas.int("skill_hit", strike).unwrap(), 75);
        // Blade skill 22: 10% more, (33 × 1.8) − 4.
        let mut blade = attacker.clone();
        blade.insert("worn4".into(), 410_003_i64.into());
        blade.insert("weapon_410".into(), 22_i64.into());
        assert_eq!(formulas.int("hit", (blade, target.clone())).unwrap(), 55);
        // A bow: 54 attack against 50 dodge less 25, 25% off.
        attacker.insert("worn4".into(), 500_003_i64.into());
        assert_eq!(formulas.int("hit", (attacker, target)).unwrap(), 41);
        let _ = std::fs::remove_dir_all(tables);
    }
}
