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
/// Trojans 5–15% more by their step.
const LIFE: &str = "base + if((profession % 1000) / 10 == 1, base * if(profession % 10 == 1, 5, if(profession % 10 == 2, 8, if(profession % 10 == 3, 10, if(profession % 10 == 4, 12, if(profession % 10 == 5, 15, 0))))) / 100, 0)";
const LIFE_BASE: &str = "(strength + agility + spirit + vitality * 8) * 3";
/// TQ's MaxMana (§5.2): spirit × 5; Fire and Water Taoists × 5 more per step (up to × 30).
const MANA: &str =
    "spirit * if((profession % 1000) / 10 > 10, min(5 + (profession % 10) * 5, 30), 5)";

/// TQ's hit chance (battle §2.3), in percent: half the attacker's agility (a monster's
/// dexterity) against the target's dodge, 10–90; a player hitting a lower monster gets 10 more
/// per 5 levels.
const HIT_CHANCE: &str = "min(max(if(attacker.monster == 0, attacker.agility, attacker.dexterity) / 2 - target.dodge + 100, 10), 90) + if(attacker.monster == 0 && target.monster != 0 && attacker.level > target.level, (attacker.level - target.level) / 5 * 10, 0)";
/// Melee damage (battle §2.4): the roll less the target's defence, at least 1; nothing when it
/// misses.
const HIT: &str = "if(attacker.chance < chance, max(attacker.roll - target.defence, 1), 0)";

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
    exp(&mut bp.events, next_lane(300.0));
    can_wear(&mut bp.events, next_lane(500.0));
    level_up(&mut bp.events, next_lane(300.0));
    death(&mut bp.events, next_lane(700.0));
    revive(&mut bp.events, next_lane(200.0));
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

    fn write(&self, w: &mut ExprWriter, text: &str) -> Operand {
        w.write(text, &mut |w, n| self.resolve(w, n))
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

fn hit(g: &mut Graph, top: f32) {
    formula(
        g,
        top,
        "formula.hit",
        &["attacker", "target"],
        None,
        |w, names| {
            names.define(w, "chance", HIT_CHANCE);
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
        &["level", "target", "damage"],
        None,
        |w, names| names.write(w, "damage"),
    );
    formula(
        g,
        top + 150.0,
        "formula.kill_exp",
        &["level", "target"],
        None,
        |w, names| names.write(w, "target.life * 5 / 100"),
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
            w.write("if(ok, 1, 0)", &mut |_, n| (n == "ok").then(|| ok.clone()))
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
            &mut |w, n| player_value(w, &names, n),
        )
        .expect("a plain formula");
    let branch = w.node("flow.branch", &[]);
    w.feed(&fresh, branch, "condition");
    let call = w.node("flow.call", &[("name", text("allot"))]);
    wire(g, (login, "then"), (branch, "exec"));
    wire(g, (branch, "true"), (call, "exec"));
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
        .write(text, &mut |w, n| player_value(w, names, n))
        .unwrap_or_else(|e| panic!("Conquer's formula \"{text}\": {e}"));
    let branch = w.node("flow.branch", &[]);
    w.feed(&cond, branch, "condition");
    branch
}

/// An action node `def` whose input `pin` is `text` worked out (a player's values by name).
fn action_of(w: &mut ExprWriter, names: &Names, def: &str, pin: &str, text: &str) -> u32 {
    let value = w
        .write(text, &mut |w, n| player_value(w, names, n))
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
        "pk" => w.node("player.pk", &[]),
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
        "strength" | "agility" | "vitality" | "spirit" => {
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
        .write(&format!("({CLASS}) * 1000 + level"), &mut |w, n| {
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
        let formulas = c3_script::Formulas::compile(&script).expect("Rhai reads it");
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
        assert_eq!(formulas.int("hit", (attacker, target)).unwrap(), 26);
    }
}
