//! The functions scripts call: dialogue, the player, the world and the game's tables.
//! They act on the [`Call`] being run; outside a call they fail with a clear error.

use crate::dialog::{DialogOption, DialogPage, Effect, NpcInfo, PlayerState};
use crate::tables::Tables;
use rhai::{Dynamic, Engine, EvalAltResult};
use std::cell::RefCell;
use std::rc::Rc;

/// The dialogue step being run: who it is for and what it has built so far.
pub(crate) struct Call {
    pub npc: NpcInfo,
    pub player: PlayerState,
    pub page: DialogPage,
    pub effects: Vec<Effect>,
}

pub(crate) type Shared = Rc<RefCell<Option<Call>>>;

type Fallible<T> = Result<T, Box<EvalAltResult>>;

/// Runs `f` on the current call.
fn with<T>(shared: &Shared, f: impl FnOnce(&mut Call) -> T) -> Fallible<T> {
    match shared.borrow_mut().as_mut() {
        Some(call) => Ok(f(call)),
        None => Err("this function only works while an NPC dialogue runs".into()),
    }
}

fn positive(count: i64, what: &str) -> Fallible<()> {
    if count > 0 {
        Ok(())
    } else {
        Err(format!("{what} must be more than 0 (got {count})").into())
    }
}

/// Registers every script function on `engine`.
pub(crate) fn register(engine: &mut Engine, shared: &Shared, tables: &Rc<RefCell<Tables>>) {
    dialogue(engine, shared);
    player(engine, shared);
    items(engine, shared);
    world(engine, shared);
    data(engine, tables);
    let s = shared.clone();
    engine.on_print(move |text| {
        let _ = with(&s, |c| c.effects.push(Effect::Log(text.to_owned())));
    });
}

fn dialogue(engine: &mut Engine, shared: &Shared) {
    let s = shared.clone();
    engine.register_fn("text", move |line: &str| {
        with(&s, |c| c.page.lines.push(line.to_owned()))
    });
    let s = shared.clone();
    engine.register_fn("option", move |label: &str, next: &str| {
        with(&s, |c| {
            c.page.options.push(DialogOption {
                label: label.to_owned(),
                next: Some(next.to_owned()),
            })
        })
    });
    let s = shared.clone();
    engine.register_fn("option", move |label: &str| {
        with(&s, |c| {
            c.page.options.push(DialogOption {
                label: label.to_owned(),
                next: None,
            })
        })
    });
    let s = shared.clone();
    engine.register_fn("npc_id", move || with(&s, |c| c.npc.id as i64));
    let s = shared.clone();
    engine.register_fn("npc_name", move || with(&s, |c| c.npc.name.clone()));
}

fn player(engine: &mut Engine, shared: &Shared) {
    let s = shared.clone();
    engine.register_fn("player_name", move || with(&s, |c| c.player.name.clone()));
    let s = shared.clone();
    engine.register_fn("level", move || with(&s, |c| c.player.level));
    let s = shared.clone();
    engine.register_fn("money", move || with(&s, |c| c.player.money));
    let s = shared.clone();
    engine.register_fn("give_money", move |amount: i64| {
        positive(amount, "money")?;
        with(&s, |c| {
            c.player.money += amount;
            c.effects.push(Effect::Money(amount));
        })
    });
    let s = shared.clone();
    engine.register_fn("take_money", move |amount: i64| {
        positive(amount, "money")?;
        with(&s, |c| {
            if c.player.money < amount {
                return false;
            }
            c.player.money -= amount;
            c.effects.push(Effect::Money(-amount));
            true
        })
    });
    let s = shared.clone();
    engine.register_fn("flag", move |name: &str| {
        with(&s, |c| c.player.flags.get(name).copied().unwrap_or(0))
    });
    let s = shared.clone();
    engine.register_fn("set_flag", move |name: &str, value: i64| {
        with(&s, |c| {
            if value == 0 {
                c.player.flags.remove(name);
            } else {
                c.player.flags.insert(name.to_owned(), value);
            }
        })
    });
}

fn items(engine: &mut Engine, shared: &Shared) {
    let s = shared.clone();
    engine.register_fn("item_count", move |item: i64| {
        with(&s, |c| c.player.item_count(item))
    });
    let s = shared.clone();
    engine.register_fn("has_item", move |item: i64| {
        with(&s, |c| c.player.item_count(item) > 0)
    });
    let s = shared.clone();
    engine.register_fn("has_item", move |item: i64, count: i64| {
        with(&s, |c| c.player.item_count(item) >= count)
    });
    let give = |s: Shared| {
        move |item: i64, count: i64| {
            positive(count, "count")?;
            with(&s, |c| {
                c.player.give_item(item, count);
                c.effects.push(Effect::GaveItem { item, count });
            })
        }
    };
    let one = give(shared.clone());
    engine.register_fn("give_item", move |item: i64| one(item, 1));
    engine.register_fn("give_item", give(shared.clone()));
    let take = |s: Shared| {
        move |item: i64, count: i64| {
            positive(count, "count")?;
            with(&s, |c| {
                let taken = c.player.take_item(item, count);
                if taken {
                    c.effects.push(Effect::TookItem { item, count });
                }
                taken
            })
        }
    };
    let one = take(shared.clone());
    engine.register_fn("take_item", move |item: i64| one(item, 1));
    engine.register_fn("take_item", take(shared.clone()));
}

fn world(engine: &mut Engine, shared: &Shared) {
    let s = shared.clone();
    engine.register_fn("message", move |text: &str| {
        with(&s, |c| c.effects.push(Effect::Message(text.to_owned())))
    });
    let s = shared.clone();
    engine.register_fn("teleport", move |map: i64, x: i64, y: i64| {
        with(&s, |c| {
            c.effects.push(Effect::Teleport { map, cell: (x, y) })
        })
    });
}

fn data(engine: &mut Engine, tables: &Rc<RefCell<Tables>>) {
    let t = tables.clone();
    engine.register_fn("lookup", move |table: &str, key: i64, column: &str| {
        t.borrow_mut().lookup(table, key, column)
    });
    let t = tables.clone();
    engine.register_fn("has_row", move |table: &str, key: i64| -> Fallible<bool> {
        Ok(t.borrow_mut().table(table)?.find(key).is_some())
    });
}

/// A table value as a script value.
pub(crate) fn to_dynamic(value: &c3_data::Value) -> Dynamic {
    match value {
        c3_data::Value::Bool(b) => (*b).into(),
        c3_data::Value::Int(i) => (*i).into(),
        c3_data::Value::Float(f) => (*f).into(),
        c3_data::Value::Text(t) => t.clone().into(),
    }
}
