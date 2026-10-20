//! TQ's guild war (`tq_re/spec/systems/syndicate.md` §6) as the blueprint `tq/guild_war`,
//! made at import from the guild-war poles (dynamic NPCs of kind 10): each minute a pole's
//! map turns its war on (status bit 1, world variable `map<id>.status`) inside the pole's
//! weekly window (`data1` to `data2`) and off outside it; a hit on a pole outside the war
//! is given back; in the war each guild's damage is its score (`npc<id>.score.<guild>`) and
//! the hitter is paid 2% of it from the owner guild's fund. The pole's fall (its `linkid`
//! chain, 207) gives the map to the top scorer.

use crate::npc_logic::SCORE;
use crate::rule_blueprint::{chain, event_lane, text, wire};
use c3_script::blueprint::{Blueprint, ExprWriter, Graph, Literal, Operand};

/// The blueprint's name in a project.
pub const GUILD_WAR_BLUEPRINT: &str = "tq/guild_war";

/// A guild-war pole: its NPC id, its map and its war's weekly window (`time_within` mode 3).
pub struct Pole {
    pub npc: u32,
    pub map: u32,
    pub window: String,
}

impl Pole {
    /// The pole of NPC row `npc` (kind 10), when its `data1` and `data2` hold a window
    /// (`dhhmm00`: weekday 1-7, hour, minute; the end is not in it).
    pub fn of(npc: &c3_scene::Npc) -> Option<Pole> {
        let number = |k: &str| npc.extra.get(k)?.trim().parse::<i64>().ok();
        let (start, end, map) = (number("data1")?, number("data2")?, number("mapid")?);
        let at = |v: i64| (v / 1_000_000, v / 10_000 % 100, v / 100 % 100);
        let ((d0, h0, m0), (d1, h1, m1)) = (at(start), at(end));
        if npc.kind != POLE_KIND || !(1..=7).contains(&d0) || !(1..=7).contains(&d1) {
            return None;
        }
        // The window's last minute (TQ's end is the first minute out of it).
        let last = (d1 * 24 + h1) * 60 + m1 - 1;
        let (d1, h1, m1) = (last / (24 * 60), last / 60 % 24, last % 60);
        Some(Pole {
            npc: npc.id,
            map: u32::try_from(map).ok()?,
            window: format!("{d0} {h0:02}:{m0:02} {d1} {h1:02}:{m1:02}"),
        })
    }
}

/// TQ's kind of a guild-war pole (`_SYNFLAG_NPC`).
const POLE_KIND: u32 = 10;

/// The guild-war blueprint of `poles`.
pub fn guild_war_blueprint(poles: &[Pole]) -> Blueprint {
    let mut bp = Blueprint::default();
    for (i, pole) in poles.iter().enumerate() {
        let top = i as f32 * 900.0;
        war_clock(&mut bp.events, pole, top);
        pole_hit(&mut bp.events, pole, top + 300.0);
    }
    bp
}

/// Each minute: the war on inside the window, off outside it, told to everyone.
fn war_clock(g: &mut Graph, pole: &Pole, top: f32) {
    let (_, on) = event_lane(g, "event.minute", [0.0, top]);
    let status = format!("map{}.status", pole.map);
    let mut w = ExprWriter::new(g, [260.0, top]);
    let flag = w.node("world.flag", &[("name", text(&status))]);
    let within = w.node(
        "time.within",
        &[
            ("mode", Literal::Number(3.0)),
            ("window", text(&pole.window)),
        ],
    );
    let write = |w: &mut ExprWriter, formula: &str| {
        w.write(formula, &mut |_, n, _| match n {
            "status" => Some(pin(flag)),
            "within" => Some(pin(within)),
            _ => None,
        })
        .unwrap_or_else(|e| panic!("the guild war's formula \"{formula}\": {e}"))
    };
    let starts = write(&mut w, "within && status % 2 == 0");
    let ends = write(&mut w, "!within && status % 2 == 1");
    let begun = write(&mut w, "status + 1");
    let over = write(&mut w, "status - 1");
    let start = w.node("flow.branch", &[]);
    w.feed(&starts, start, "condition");
    let end = w.node("flow.branch", &[]);
    w.feed(&ends, end, "condition");
    let set_on = w.node("world.set_flag", &[("name", text(&status))]);
    w.feed(&begun, set_on, "value");
    let say_on = w.node(
        "world.announce",
        &[("text", text("The guild war has begun."))],
    );
    let set_off = w.node("world.set_flag", &[("name", text(&status))]);
    w.feed(&over, set_off, "value");
    let say_off = w.node(
        "world.announce",
        &[("text", text("The guild war is over."))],
    );
    wire(g, on, (start, "exec"));
    chain(g, (start, "true"), &[set_on, say_on]);
    wire(g, (start, "false"), (end, "exec"));
    chain(g, (end, "true"), &[set_off, say_off]);
}

/// A hit on the pole: given back outside the war; in it, the hitter's guild (not the owner)
/// scores the damage and the hitter is paid 2% of it from the owner's fund.
fn pole_hit(g: &mut Graph, pole: &Pole, top: f32) {
    let (event, on) = event_lane(g, "event.hit_npc", [0.0, top]);
    let status = format!("map{}.status", pole.map);
    let life = c3_game::npc_life_flag(pole.npc);
    let mut w = ExprWriter::new(g, [260.0, top]);
    let npc = w.node("npc.id", &[]);
    let war = w.node("world.flag", &[("name", text(&status))]);
    let left = w.node("world.flag", &[("name", text(&life))]);
    let guild = w.node("guild.id", &[]);
    let owner = w.node(
        "guild.map_owner",
        &[("map", Literal::Number(f64::from(pole.map)))],
    );
    let fund = w.node("guild.fund", &[]);
    w.feed(&pin(owner), fund, "guild");
    let score_name = w.node(
        "text.number",
        &[("text", text(&format!("npc{}{SCORE}", pole.npc)))],
    );
    w.feed(&pin(guild), score_name, "number");
    let score = w.node("world.flag", &[]);
    w.feed(&pin(score_name), score, "name");
    let damage = Operand::Pin {
        node: event,
        pin: "damage".into(),
    };
    let pole_id = f64::from(pole.npc);
    let write = |w: &mut ExprWriter, formula: &str| {
        w.write(formula, &mut |_, n, _| match n {
            "npc" => Some(pin(npc)),
            "war" => Some(pin(war)),
            "left" => Some(pin(left)),
            "guild" => Some(pin(guild)),
            "owner" => Some(pin(owner)),
            "fund" => Some(pin(fund)),
            "score" => Some(pin(score)),
            "damage" => Some(damage.clone()),
            "pole" => Some(Operand::Number(pole_id)),
            _ => None,
        })
        .unwrap_or_else(|e| panic!("the guild war's formula \"{formula}\": {e}"))
    };
    let is_pole = write(&mut w, "npc == pole");
    let in_war = write(&mut w, "war % 2 == 1");
    let scores = write(&mut w, "guild > 0 && guild != owner");
    let healed = write(&mut w, "left + damage");
    let scored = write(&mut w, "score + damage");
    let pay = write(
        &mut w,
        "if(owner > 0, min(damage * 2 / 100, max(fund, 0)), 0)",
    );
    let paid = write(
        &mut w,
        "0 - if(owner > 0, min(damage * 2 / 100, max(fund, 0)), 0)",
    );
    let branch = |w: &mut ExprWriter, condition: &Operand| {
        let b = w.node("flow.branch", &[]);
        w.feed(condition, b, "condition");
        b
    };
    let ours = branch(&mut w, &is_pole);
    let war_on = branch(&mut w, &in_war);
    let counts = branch(&mut w, &scores);
    let give_back = w.node("world.set_flag", &[("name", text(&life))]);
    w.feed(&healed, give_back, "value");
    let add_score = w.node("world.set_flag", &[]);
    w.feed(&pin(score_name), add_score, "name");
    w.feed(&scored, add_score, "value");
    let from_fund = w.node("guild.add_fund", &[]);
    w.feed(&pin(owner), from_fund, "guild");
    w.feed(&paid, from_fund, "amount");
    let to_hitter = w.node("player.give_money", &[]);
    w.feed(&pay, to_hitter, "amount");
    wire(g, on, (ours, "exec"));
    wire(g, (ours, "true"), (war_on, "exec"));
    wire(g, (war_on, "false"), (give_back, "exec"));
    wire(g, (war_on, "true"), (counts, "exec"));
    chain(g, (counts, "true"), &[add_score, from_fund, to_hitter]);
}

fn pin(node: u32) -> Operand {
    Operand::Pin {
        node,
        pin: "value".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pole_s_window_and_blueprint() {
        let npc = c3_scene::Npc {
            id: 810,
            name: "BaiBangGateStn".into(),
            cell: (84, 99),
            direction: 0,
            look: 113,
            kind: 10,
            script: String::new(),
            extra: [
                ("data1", "6140000"),
                ("data2", "7210000"),
                ("mapid", "1038"),
            ]
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .into(),
        };
        let pole = Pole::of(&npc).expect("a pole");
        assert_eq!((pole.map, pole.window.as_str()), (1038, "6 14:00 7 20:59"));
        let bp = guild_war_blueprint(&[pole]);
        c3_script::blueprint::compile(GUILD_WAR_BLUEPRINT, &bp).expect("compiles");
    }
}
