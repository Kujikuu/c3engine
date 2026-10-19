//! The functions scripts call: dialogue, the player, the world and the game's tables.
//! They act on the [`Call`] being run; outside a call they fail with a clear error.

use crate::dialog::{DialogOption, DialogPage, Effect, NpcInfo, PlayerState};
use crate::quest::{self, DONE, NOT_TAKEN, Quest, TAKEN};
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
    quests(engine, shared, tables);
    more(engine, shared);
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
    engine.register_fn("open_shop", move || {
        with(&s, |c| c.effects.push(Effect::OpenShop))
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
    engine.register_fn("currency", move |name: &str| {
        with(&s, |c| c.player.currency(name))
    });
    let s = shared.clone();
    engine.register_fn("give_currency", move |name: &str, amount: i64| {
        positive(amount, "amount")?;
        with(&s, |c| {
            *c.player.wallet.entry(name.to_owned()).or_default() += amount;
            c.effects.push(Effect::Currency {
                name: name.to_owned(),
                amount,
            });
        })
    });
    let s = shared.clone();
    engine.register_fn("take_currency", move |name: &str, amount: i64| {
        positive(amount, "amount")?;
        with(&s, |c| {
            if c.player.currency(name) < amount {
                return false;
            }
            *c.player.wallet.entry(name.to_owned()).or_default() -= amount;
            c.effects.push(Effect::Currency {
                name: name.to_owned(),
                amount: -amount,
            });
            true
        })
    });
    let s = shared.clone();
    engine.register_fn("knows_skill", move |skill: i64| {
        with(&s, |c| c.player.skills.contains(&skill))
    });
    let s = shared.clone();
    engine.register_fn("learn_skill", move |skill: i64| {
        with(&s, |c| {
            if c.player.skills.contains(&skill) {
                return false;
            }
            c.player.skills.push(skill);
            c.effects.push(Effect::LearnedSkill(skill));
            true
        })
    });
    let s = shared.clone();
    engine.register_fn("flag", move |name: &str| with(&s, |c| c.player.flag(name)));
    let s = shared.clone();
    engine.register_fn("set_flag", move |name: &str, value: i64| {
        with(&s, |c| c.player.set_flag(name.to_owned(), value))
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
    let s = shared.clone();
    engine.register_fn("start_timer", move |name: &str, seconds: f64| {
        with(&s, |c| {
            c.effects.push(Effect::StartTimer {
                name: name.to_owned(),
                seconds,
            })
        })
    });
    let s = shared.clone();
    engine.register_fn("start_timer", move |name: &str, seconds: i64| {
        with(&s, |c| {
            c.effects.push(Effect::StartTimer {
                name: name.to_owned(),
                seconds: seconds as f64,
            })
        })
    });
}

/// Life, statuses, world variables, chance, where the player is, effects and sounds, the
/// game UI, monsters and walking.
fn more(engine: &mut Engine, shared: &Shared) {
    let effect = |shared: &Shared, make: fn(i64) -> Effect| {
        let s = shared.clone();
        move |n: i64| with(&s, |c| c.effects.push(make(n)))
    };
    engine.register_fn("give_exp", effect(shared, Effect::Exp));
    engine.register_fn("add_pk", effect(shared, Effect::Pk));
    engine.register_fn("add_mana", effect(shared, Effect::Mana));
    engine.register_fn("drop_money", effect(shared, Effect::DropMoney));
    let s = shared.clone();
    engine.register_fn("move_npc", move |npc: i64, map: i64, x: i64, y: i64| {
        with(&s, |c| {
            c.effects.push(Effect::MoveNpc {
                npc,
                map,
                cell: (x, y),
            })
        })
    });
    engine.register_fn("drop_bag_items", effect(shared, Effect::DropBagItems));
    engine.register_fn("lose_exp", effect(shared, Effect::LoseExp));
    let s = shared.clone();
    engine.register_fn("exp", move || with(&s, |c| c.player.exp));
    let s = shared.clone();
    engine.register_fn("map_pk_free", move || with(&s, |c| c.player.map_pk_free));
    let s = shared.clone();
    engine.register_fn("killed_by_monster", move || {
        with(&s, |c| c.player.killed_by_monster)
    });
    let s = shared.clone();
    engine.register_fn("body", move || with(&s, |c| c.player.body));
    let s = shared.clone();
    engine.register_fn("hair", move || with(&s, |c| c.player.hair));
    let s = shared.clone();
    engine.register_fn("set_hair", move |hair: i64| {
        with(&s, |c| c.player.hair = hair.max(0))
    });
    let s = shared.clone();
    engine.register_fn("mana", move || with(&s, |c| c.player.mana));
    let s = shared.clone();
    engine.register_fn("max_mana", move || with(&s, |c| c.player.max_mana));
    let s = shared.clone();
    engine.register_fn("pk", move || with(&s, |c| c.player.pk));
    let s = shared.clone();
    engine.register_fn("worn", move |slot: i64| {
        with(&s, |c| c.player.worn.get(&slot).copied().unwrap_or(0))
    });
    let s = shared.clone();
    engine.register_fn("fill", move |text: &str| {
        with(&s, |c| {
            let values = [
                ("{player}", c.player.name.clone()),
                ("{npc}", c.npc.name.clone()),
                ("{level}", c.player.level.to_string()),
                ("{money}", c.player.money.to_string()),
            ];
            (values.iter()).fold(text.to_owned(), |t, (key, value)| t.replace(key, value))
        })
    });
    attributes(engine, shared);
    held_items(engine, shared);
    let s = shared.clone();
    engine.register_fn("picture", move |id: i64| {
        with(&s, |c| c.page.picture = Some(id))
    });
    engine.register_fn("time_within", |mode: i64, spec: &str| {
        crate::clock::within(mode, spec, &crate::clock::now())
    });
    engine.register_fn("heal", effect(shared, |n| Effect::Life(n.max(0))));
    engine.register_fn("hurt", effect(shared, |n| Effect::Life(-n.max(0))));
    engine.register_fn("add_status", effect(shared, Effect::AddStatus));
    engine.register_fn("open_window", effect(shared, Effect::OpenWindow));
    let s = shared.clone();
    engine.register_fn("has_status", move |id: i64| {
        with(&s, |c| c.player.statuses.contains(&id))
    });
    let s = shared.clone();
    engine.register_fn("life", move || with(&s, |c| c.player.life));
    let s = shared.clone();
    engine.register_fn("max_life", move || with(&s, |c| c.player.max_life));
    let s = shared.clone();
    engine.register_fn("map_id", move || with(&s, |c| c.player.map));
    let s = shared.clone();
    engine.register_fn("player_x", move || with(&s, |c| c.player.cell.0));
    let s = shared.clone();
    engine.register_fn("player_y", move || with(&s, |c| c.player.cell.1));
    let s = shared.clone();
    engine.register_fn("world_flag", move |name: &str| {
        with(&s, |c| c.player.world.get(name).copied().unwrap_or(0))
    });
    let s = shared.clone();
    engine.register_fn("set_world_flag", move |name: &str, value: i64| {
        with(&s, |c| {
            c.player.world.insert(name.to_owned(), value);
        })
    });
    let text = |shared: &Shared, make: fn(String) -> Effect| {
        let s = shared.clone();
        move |t: &str| with(&s, |c| c.effects.push(make(t.to_owned())))
    };
    engine.register_fn("play_effect", text(shared, Effect::PlayEffect));
    engine.register_fn("play_sound", text(shared, Effect::PlaySound));
    engine.register_fn("announce", text(shared, Effect::Announce));
    engine.register_fn("team_message", text(shared, Effect::TeamMessage));
    let s = shared.clone();
    engine.register_fn("team_size", move || with(&s, |c| c.player.team_size));
    let s = shared.clone();
    engine.register_fn("profession", move || with(&s, |c| c.player.profession));
    let s = shared.clone();
    engine.register_fn("in_team", move || with(&s, |c| c.player.team_size > 0));
    let s = shared.clone();
    engine.register_fn("team_leader", move || {
        with(&s, |c| c.player.team_leader.clone())
    });
    let s = shared.clone();
    engine.register_fn("spawn_monster", move |kind: i64, x: i64, y: i64| {
        with(&s, |c| c.effects.push(Effect::Spawn { kind, cell: (x, y) }))
    });
    let s = shared.clone();
    engine.register_fn("walk_to", move |x: i64, y: i64, tag: &str| {
        with(&s, |c| {
            c.effects.push(Effect::WalkTo {
                cell: (x, y),
                tag: tag.to_owned(),
            })
        })
    });
    // Chance: a whole number from `low` to `high`, both included.
    let seed = Rc::new(std::cell::Cell::new(seed()));
    engine.register_fn("random", move |low: i64, high: i64| {
        let (low, high) = (low.min(high), low.max(high));
        let mut x = seed.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        seed.set(x);
        let span = (high - low) as u64 + 1;
        low + (x % span) as i64
    });
}

/// A starting point for `random` (it differs from run to run).
fn seed() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    nanos | 1
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

/// Quest `id` of the project's quest table.
fn find_quest(tables: &Rc<RefCell<Tables>>, id: i64) -> Fallible<Quest> {
    let mut tables = tables.borrow_mut();
    let table = tables.table(quest::QUEST_TABLE)?;
    table
        .find(id)
        .and_then(|row| Quest::from_row(table, row))
        .ok_or_else(|| format!("there is no quest {id} in the quests table").into())
}

impl PlayerState {
    fn flag(&self, name: &str) -> i64 {
        self.flags.get(name).copied().unwrap_or(0)
    }

    fn set_flag(&mut self, name: String, value: i64) {
        if value == 0 {
            self.flags.remove(&name);
        } else {
            self.flags.insert(name, value);
        }
    }

    fn quest_progress(&self, q: &Quest) -> (i64, i64) {
        q.progress(|f| self.flag(f), |i| self.item_count(i))
    }

    fn quest_ready(&self, q: &Quest) -> bool {
        let (done, needed) = self.quest_progress(q);
        self.flag(&quest::state_flag(q.id)) == TAKEN && done >= needed
    }
}

fn quests(engine: &mut Engine, shared: &Shared, tables: &Rc<RefCell<Tables>>) {
    let s = shared.clone();
    engine.register_fn("quest_state", move |id: i64| {
        with(&s, |c| c.player.flag(&quest::state_flag(id)))
    });
    let t = tables.clone();
    engine.register_fn("quest_name", move |id: i64| -> Fallible<String> {
        Ok(find_quest(&t, id)?.name)
    });
    let (s, t) = (shared.clone(), tables.clone());
    engine.register_fn("quest_progress", move |id: i64| -> Fallible<i64> {
        let q = find_quest(&t, id)?;
        with(&s, |c| c.player.quest_progress(&q).0)
    });
    let (s, t) = (shared.clone(), tables.clone());
    engine.register_fn("quest_ready", move |id: i64| -> Fallible<bool> {
        let q = find_quest(&t, id)?;
        with(&s, |c| c.player.quest_ready(&q))
    });
    let (s, t) = (shared.clone(), tables.clone());
    engine.register_fn("start_quest", move |id: i64| -> Fallible<bool> {
        let q = find_quest(&t, id)?;
        with(&s, |c| {
            let p = &mut c.player;
            if p.flag(&quest::state_flag(id)) != NOT_TAKEN || p.level < q.min_level {
                return false;
            }
            p.set_flag(quest::state_flag(id), TAKEN);
            p.set_flag(quest::kills_flag(id), 0);
            c.effects.push(Effect::QuestTaken { id, name: q.name });
            true
        })
    });
    let (s, t) = (shared.clone(), tables.clone());
    engine.register_fn("complete_quest", move |id: i64| -> Fallible<bool> {
        let q = find_quest(&t, id)?;
        with(&s, |c| {
            if !c.player.quest_ready(&q) {
                return false;
            }
            if let quest::Goal::Collect { item, count } = q.goal {
                c.player.take_item(item, count);
                c.effects.push(Effect::TookItem { item, count });
            }
            c.player.set_flag(quest::state_flag(id), DONE);
            c.player.set_flag(quest::kills_flag(id), 0);
            if q.reward_money > 0 {
                c.player.money += q.reward_money;
                c.effects.push(Effect::Money(q.reward_money));
            }
            if q.reward_item > 0 {
                c.player.give_item(q.reward_item, 1);
                c.effects.push(Effect::GaveItem {
                    item: q.reward_item,
                    count: 1,
                });
            }
            if q.reward_exp > 0 {
                c.effects.push(Effect::Exp(q.reward_exp));
            }
            c.effects.push(Effect::QuestDone { id, name: q.name });
            true
        })
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

/// The player's attributes, record point, worn items' attributes, bag space and items by
/// a range of types.
fn attributes(engine: &mut Engine, shared: &Shared) {
    let s = shared.clone();
    engine.register_fn("attr", move |name: &str| {
        with(&s, |c| c.player.attributes.get(name).copied().unwrap_or(0))
    });
    let s = shared.clone();
    engine.register_fn("add_attr", move |name: &str, amount: i64| {
        with(&s, |c| {
            let (min, max) = c
                .player
                .attribute_limits
                .get(name)
                .copied()
                .unwrap_or_default();
            let value = c
                .player
                .attributes
                .get(name)
                .copied()
                .unwrap_or(0)
                .saturating_add(amount);
            // A cost the player can't pay fails; more than the most is cut off.
            if min.is_some_and(|min| value < min) {
                return false;
            }
            c.player
                .attributes
                .insert(name.to_owned(), max.map_or(value, |max| value.min(max)));
            true
        })
    });
    let s = shared.clone();
    engine.register_fn("set_attr", move |name: &str, value: i64| {
        with(&s, |c| {
            let (min, max) = c
                .player
                .attribute_limits
                .get(name)
                .copied()
                .unwrap_or_default();
            let value = value
                .max(min.unwrap_or(i64::MIN))
                .min(max.unwrap_or(i64::MAX));
            c.player.attributes.insert(name.to_owned(), value);
        })
    });
    let s = shared.clone();
    engine.register_fn("set_record", move |map: i64, x: i64, y: i64| {
        with(&s, |c| c.player.record = Some((map, (x, y))))
    });
    let s = shared.clone();
    engine.register_fn("go_record", move || {
        with(&s, |c| match c.player.record {
            Some((map, cell)) => {
                c.effects.push(Effect::Teleport { map, cell });
                true
            }
            None => false,
        })
    });
    let s = shared.clone();
    engine.register_fn("worn_attr", move |slot: i64, name: &str| {
        with(&s, |c| {
            (c.player.worn_attrs.get(&slot))
                .and_then(|a| a.get(name))
                .copied()
                .unwrap_or(0)
        })
    });
    let s = shared.clone();
    engine.register_fn("set_worn_attr", move |slot: i64, name: &str, value: i64| {
        with(&s, |c| match c.player.worn_attrs.get_mut(&slot) {
            Some(attrs) if attrs.contains_key(name) => {
                attrs.insert(name.to_owned(), value);
                true
            }
            _ => false,
        })
    });
    let s = shared.clone();
    engine.register_fn("bag_space", move || with(&s, |c| c.player.bag_space));
    let s = shared.clone();
    engine.register_fn("count_items_between", move |low: i64, high: i64| {
        with(&s, |c| {
            c.player
                .items
                .range(low..=high)
                .map(|(_, n)| n)
                .sum::<i64>()
        })
    });
    let s = shared.clone();
    engine.register_fn(
        "take_items_between",
        move |low: i64, high: i64, count: i64| {
            with(&s, |c| {
                let have: i64 = c.player.items.range(low..=high).map(|(_, n)| n).sum();
                if count <= 0 || have < count {
                    return false;
                }
                let mut left = count;
                for (&item, n) in c.player.items.range_mut(low..=high) {
                    let take = left.min(*n);
                    *n -= take;
                    left -= take;
                    if take > 0 {
                        c.effects.push(Effect::TookItem { item, count: take });
                    }
                }
                c.player.items.retain(|_, n| *n > 0);
                true
            })
        },
    );
    let s = shared.clone();
    engine.register_fn("forget_skill", move |skill: i64| {
        with(&s, |c| {
            let known = c.player.skills.contains(&skill);
            c.player.skills.retain(|&k| k != skill);
            known
        })
    });
    engine.register_fn("clock", || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64)
    });
}

/// The player's own items by their ids: their type, numbers and durability.
fn held_items(engine: &mut Engine, shared: &Shared) {
    let s = shared.clone();
    engine.register_fn("item_type_of", move |id: i64| {
        with(&s, |c| c.player.held.get(&id).map_or(0, |i| i.kind))
    });
    let s = shared.clone();
    engine.register_fn("set_item_type", move |id: i64, kind: i64| {
        with(&s, |c| match c.player.held.get_mut(&id) {
            Some(item) if kind > 0 => {
                item.kind = kind;
                true
            }
            _ => false,
        })
    });
    let s = shared.clone();
    engine.register_fn("item_attr", move |id: i64, name: &str| {
        with(&s, |c| {
            (c.player.held.get(&id))
                .and_then(|i| i.attrs.get(name))
                .copied()
                .unwrap_or(0)
        })
    });
    let s = shared.clone();
    engine.register_fn("set_item_attr", move |id: i64, name: &str, value: i64| {
        with(&s, |c| match c.player.held.get_mut(&id) {
            Some(item) if item.attrs.contains_key(name) => {
                item.attrs.insert(name.to_owned(), value);
                true
            }
            _ => false,
        })
    });
    let s = shared.clone();
    engine.register_fn("item_durability", move |id: i64| {
        with(&s, |c| c.player.held.get(&id).map_or(0, |i| i.durability))
    });
    let s = shared.clone();
    engine.register_fn("set_item_durability", move |id: i64, value: i64| {
        with(&s, |c| {
            if let Some(item) = c.player.held.get_mut(&id) {
                item.durability = value.max(0);
            }
        })
    });
    let s = shared.clone();
    engine.register_fn("item_worn", move |id: i64| {
        with(&s, |c| c.player.held.get(&id).is_some_and(|i| i.worn))
    });
}
