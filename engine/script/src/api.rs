//! The functions scripts call: dialogue, the player, the world and the game's tables.
//! They act on the [`Call`] being run; outside a call they fail with a clear error.

use crate::dialog::{
    BountyView, DialogOption, DialogPage, Effect, GuildView, MapCopyView, NpcInfo, PlayerState,
    PoliceView, WantedView, WorldChange,
};
use crate::quest::{self, DONE, NOT_TAKEN, Quest, TAKEN};
use crate::tables::Tables;
use rhai::{Dynamic, Engine, EvalAltResult};
use std::cell::RefCell;
use std::collections::BTreeMap;
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
    levels(engine, shared, tables);
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
                input: None,
            })
        })
    });
    let s = shared.clone();
    engine.register_fn(
        "input_option",
        move |label: &str, length: i64, next: &str| {
            with(&s, |c| {
                c.page.options.push(DialogOption {
                    label: label.to_owned(),
                    next: Some(next.to_owned()),
                    input: Some(length.clamp(1, 255) as u32),
                })
            })
        },
    );
    let s = shared.clone();
    engine.register_fn("accept", move || with(&s, |c| c.player.accept.clone()));
    let s = shared.clone();
    engine.register_fn("accept_number", move || {
        with(&s, |c| c.player.accept.trim().parse::<i64>().unwrap_or(0))
    });
    let s = shared.clone();
    engine.register_fn("option", move |label: &str| {
        with(&s, |c| {
            c.page.options.push(DialogOption {
                label: label.to_owned(),
                next: None,
                input: None,
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
    let s = shared.clone();
    engine.register_fn("text_var", move |name: &str| {
        with(&s, |c| {
            c.player.texts.get(name).cloned().unwrap_or_default()
        })
    });
    let s = shared.clone();
    engine.register_fn("set_text_var", move |name: &str, value: &str| {
        with(&s, |c| {
            if value.is_empty() {
                c.player.texts.remove(name);
            } else {
                c.player.texts.insert(name.to_owned(), value.to_owned());
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
    // Given when the bag has room (a free place, or a stack of it to join); whether it was.
    let give = |s: Shared| {
        move |item: i64, count: i64| {
            positive(count, "count")?;
            with(&s, |c| {
                let joins = c.player.item_count(item) > 0;
                if !joins && c.player.bag_space <= 0 {
                    c.effects.push(Effect::Message("Your bag is full.".into()));
                    return false;
                }
                if !joins {
                    c.player.bag_space -= 1;
                }
                c.player.give_item(item, count);
                c.effects.push(Effect::GaveItem { item, count });
                true
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
    // An empty line says nothing.
    engine.register_fn("message", move |text: &str| {
        with(&s, |c| {
            if !text.is_empty() {
                c.effects.push(Effect::Message(text.to_owned()))
            }
        })
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
    engine.register_fn("raise", move |name: &str, a: i64, b: i64, c: i64| {
        with(&s, |call| {
            call.effects.push(Effect::Signal {
                name: name.to_owned(),
                args: [a, b, c],
            })
        })
    });
    let s = shared.clone();
    engine.register_fn("worn_id", move |slot: i64| {
        with(&s, |c| {
            let worn = c.player.worn.get(&slot).copied();
            (c.player.held.iter())
                .find(|(_, i)| i.worn && Some(i.kind) == worn)
                .map_or(0, |(id, _)| *id)
        })
    });
    let s = shared.clone();
    engine.register_fn("set_level", move |level: i64| {
        with(&s, |c| c.player.level = level.max(1))
    });
    let s = shared.clone();
    engine.register_fn("set_exp", move |exp: i64| {
        with(&s, |c| c.player.exp = exp.max(0))
    });
    let s = shared.clone();
    engine.register_fn("set_profession", move |p: i64| {
        with(&s, |c| c.player.profession = p.max(0))
    });
    let s = shared.clone();
    engine.register_fn("set_body", move |body: i64| {
        with(&s, |c| c.player.body = body.max(0))
    });
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
    engine.register_fn("monsters_named", move |map: i64, name: &str| {
        with(&s, |c| {
            if map == c.player.map {
                c.player.monsters.get(name).copied().unwrap_or(0)
            } else {
                0
            }
        })
    });
    let s = shared.clone();
    engine.register_fn("map_players", move |map: i64, alive: bool| {
        with(&s, |c| {
            (c.player.map_players.get(&map))
                .map_or(0, |&(all, living)| if alive { living } else { all })
        })
    });
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
                ("{accept}", c.player.accept.clone()),
                ("{member}", guild_member_at(c, c.player.flag("iterator"))),
                ("{wanted}", wanted_line(c, c.player.flag("iterator"))),
                ("{police}", police_line(c, c.player.flag("iterator"))),
            ];
            (values.iter()).fold(text.to_owned(), |t, (key, value)| t.replace(key, value))
        })
    });
    attributes(engine, shared);
    held_items(engine, shared);
    guilds(engine, shared);
    wanted(engine, shared);
    bounties(engine, shared);
    police(engine, shared);
    map_copies(engine, shared);
    let s = shared.clone();
    engine.register_fn("npc_count", move |field: &str, value: &str| {
        with(&s, |c| {
            let npcs = c.player.map_npcs.iter();
            let kind: i64 = value.trim().parse().unwrap_or(-1);
            (match field {
                "name" => npcs.filter(|(n, _, _)| n == value).count(),
                "kind" => npcs.filter(|(_, k, _)| *k == kind).count(),
                _ => npcs.count(),
            }) as i64
        })
    });
    let s = shared.clone();
    engine.register_fn("ask_placement", move |look: i64, kind: i64, event: &str| {
        with(&s, |c| {
            c.effects.push(Effect::AskPlacement {
                look,
                kind,
                event: event.to_owned(),
            })
        })
    });
    let s = shared.clone();
    engine.register_fn("placed", move |field: &str| {
        with(&s, |c| match field {
            "x" => c.player.placed.0,
            "y" => c.player.placed.1,
            "direction" => c.player.placed.2,
            "action" => c.player.placed_pose.0,
            "frame" => c.player.placed_pose.1,
            _ => 0,
        })
    });
    // A family of world variables `<prefix><number>`: the number of the highest above 0,
    // and all of them back to 0.
    let s = shared.clone();
    engine.register_fn("flag_top", move |prefix: &str| {
        with(&s, |c| {
            let mut best = (0, 0);
            for (name, &value) in &c.player.world {
                let Some(n) = name
                    .strip_prefix(prefix)
                    .and_then(|n| n.parse::<i64>().ok())
                else {
                    continue;
                };
                if value > best.1 {
                    best = (n, value);
                }
            }
            best.0
        })
    });
    let s = shared.clone();
    engine.register_fn("clear_flags", move |prefix: &str| {
        with(&s, |c| {
            for (name, value) in c.player.world.iter_mut() {
                if name.starts_with(prefix) {
                    *value = 0;
                }
            }
        })
    });
    let s = shared.clone();
    engine.register_fn("combine_items", move || {
        with(&s, |c| {
            (c.player.combining.iter())
                .map(|&i| Dynamic::from(i))
                .collect::<rhai::Array>()
        })
    });
    let s = shared.clone();
    engine.register_fn("npc_near", move |kind: i64, range: i64| {
        with(&s, |c| {
            (c.player.map_npcs.iter()).any(|(_, k, d)| *k == kind && *d <= range)
        })
    });
    let s = shared.clone();
    engine.register_fn(
        "create_npc",
        move |map: i64,
              x: i64,
              y: i64,
              name: &str,
              look: i64,
              kind: i64,
              script: &str,
              extra: &str| {
            with(&s, |c| {
                c.effects.push(Effect::CreateNpc {
                    map,
                    cell: (x, y),
                    name: name.to_owned(),
                    look,
                    kind,
                    script: script.to_owned(),
                    extra: extra.to_owned(),
                })
            })
        },
    );
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
    engine.register_fn("players_near_with", move |status: i64, range: i64| {
        with(&s, |c| {
            (c.player.nearby.iter())
                .filter(|(d, statuses)| *d <= range && statuses.contains(&status))
                .count() as i64
        })
    });
    let s = shared.clone();
    engine.register_fn("add_status_near", move |status: i64, range: i64| {
        with(&s, |c| {
            c.effects.push(Effect::AddStatusNear { status, range })
        })
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
    let s = shared.clone();
    engine.register_fn("room_cue", move |name: &str, a: i64, b: i64| {
        with(&s, |c| {
            c.effects.push(Effect::RoomCue {
                name: name.to_owned(),
                args: [a, b],
            })
        })
    });
    let s = shared.clone();
    engine.register_fn("leads_team", move || {
        with(&s, |c| {
            c.player.team_size > 0 && c.player.team_leader == c.player.name
        })
    });
    let s = shared.clone();
    engine.register_fn("loot_item", move |item: i64| {
        with(&s, |c| c.effects.push(Effect::Loot { item, money: 0 }))
    });
    let s = shared.clone();
    engine.register_fn("loot_money", move |money: i64| {
        with(&s, |c| c.effects.push(Effect::Loot { item: 0, money }))
    });
    let s = shared.clone();
    engine.register_fn(
        "teleport_team",
        move |map: i64, x: i64, y: i64, range: i64| {
            with(&s, |c| {
                c.effects.push(Effect::TeleportTeam {
                    map,
                    cell: (x, y),
                    range,
                })
            })
        },
    );
    let s = shared.clone();
    engine.register_fn("deliver_text", move |to: &str, name: &str, value: &str| {
        with(&s, |c| {
            let delivery = crate::dialog::Delivery::Text {
                name: name.to_owned(),
                value: value.to_owned(),
            };
            let to = to.to_owned();
            c.effects
                .push(Effect::World(WorldChange::Deliver { to, delivery }));
        })
    });
    let s = shared.clone();
    engine.register_fn("deliver_item", move |to: &str, kind: i64| {
        with(&s, |c| {
            let delivery = crate::dialog::Delivery::Item { kind };
            let to = to.to_owned();
            c.effects
                .push(Effect::World(WorldChange::Deliver { to, delivery }));
        })
    });
    let s = shared.clone();
    engine.register_fn("player_where", move |name: &str| {
        with(&s, |c| {
            c.player.online.get(name).cloned().unwrap_or_default()
        })
    });
    engine.register_fn(
        "record_put",
        |mut record: rhai::Map, field: &str, value: i64| {
            record.insert(field.into(), value.into());
            record
        },
    );
    engine.register_fn(
        "around",
        |text: &str, before: &str, after: &str| match text {
            "" => String::new(),
            _ => format!("{before}{text}{after}"),
        },
    );
    let s = shared.clone();
    engine.register_fn("client_cue_text", move |name: &str, a: i64, text: &str| {
        with(&s, |c| {
            c.effects.push(Effect::ClientCueText {
                name: name.to_owned(),
                a,
                text: text.to_owned(),
            })
        })
    });
    let s = shared.clone();
    engine.register_fn("room_cue_text", move |name: &str, a: i64, text: &str| {
        with(&s, |c| {
            c.effects.push(Effect::RoomCueText {
                name: name.to_owned(),
                a,
                text: text.to_owned(),
            })
        })
    });
    let s = shared.clone();
    engine.register_fn("team_effect", move |effect: &str| {
        with(&s, |c| {
            c.effects.push(Effect::TeamEffect(effect.to_owned()))
        })
    });
    let s = shared.clone();
    engine.register_fn("map_timer", move |map: i64, name: &str, count: i64| {
        with(&s, |c| {
            c.effects.push(Effect::MapTimer {
                map,
                name: name.to_owned(),
                count,
            })
        })
    });
    let s = shared.clone();
    engine.register_fn("erase_npcs", move |kind: i64| {
        with(&s, |c| c.effects.push(Effect::EraseNpcs(kind)))
    });
    let s = shared.clone();
    engine.register_fn("transform", move |monster: i64, secs: i64, skill: i64| {
        with(&s, |c| {
            c.effects.push(Effect::Transform {
                monster,
                secs,
                skill,
            })
        })
    });
    let s = shared.clone();
    engine.register_fn(
        "create_trap",
        move |kind: i64, look: i64, map: i64, x: i64, y: i64| {
            with(&s, |c| {
                c.effects.push(Effect::CreateTrap {
                    kind,
                    look,
                    map,
                    cell: (x, y),
                })
            })
        },
    );
    let s = shared.clone();
    engine.register_fn("erase_trap", move || {
        with(&s, |c| c.effects.push(Effect::EraseTrap))
    });
    let s = shared.clone();
    engine.register_fn("drop_at", move |item: i64, map: i64, x: i64, y: i64| {
        with(&s, |c| {
            c.effects.push(Effect::DropAt {
                item,
                map,
                cell: (x, y),
            })
        })
    });
    let s = shared.clone();
    engine.register_fn("remove_monsters", move |map: i64, kind: i64| {
        with(&s, |c| c.effects.push(Effect::RemoveMonsters { map, kind }))
    });
    let s = shared.clone();
    engine.register_fn("map_travel", move |from: i64, to: i64, x: i64, y: i64| {
        with(&s, |c| {
            c.effects.push(Effect::MapTravel {
                from,
                to,
                cell: (x, y),
            })
        })
    });
    let s = shared.clone();
    engine.register_fn("add_status_for", move |status: i64, secs: i64| {
        with(&s, |c| {
            c.effects.push(Effect::AddStatusFor { status, secs })
        })
    });
    engine.register_fn("remove_status", effect(shared, Effect::RemoveStatus));
    let s = shared.clone();
    engine.register_fn("open_storage", move || {
        with(&s, |c| c.effects.push(Effect::OpenStorage))
    });
    let s = shared.clone();
    engine.register_fn("saved_money", move || with(&s, |c| c.player.saved_money));
    let s = shared.clone();
    engine.register_fn("add_saved_money", move |n: i64| {
        with(&s, |c| {
            let after = c.player.saved_money + n;
            if !(0..=1_000_000_000).contains(&after) {
                return false;
            }
            c.player.saved_money = after;
            c.effects.push(Effect::SavedMoney(n));
            true
        })
    });
    let s = shared.clone();
    engine.register_fn("world_timer", move |name: &str, secs: i64| {
        with(&s, |c| {
            c.effects.push(Effect::WorldTimer {
                name: name.to_owned(),
                secs,
            })
        })
    });
    let s = shared.clone();
    engine.register_fn("used_item", move || with(&s, |c| c.player.used_item));
    let s = shared.clone();
    engine.register_fn("npc_look", move |npc: i64| {
        with(&s, |c| c.player.npc_looks.get(&npc).copied().unwrap_or(0))
    });
    let s = shared.clone();
    engine.register_fn("set_npc_look", move |npc: i64, look: i64| {
        with(&s, |c| {
            c.player.npc_looks.insert(npc, look);
            c.effects.push(Effect::NpcLook { npc, look })
        })
    });
    let s = shared.clone();
    engine.register_fn("die", move || with(&s, |c| c.effects.push(Effect::Die)));
    // A number for the player that lasts (its name's), for scripts to keep.
    let s = shared.clone();
    engine.register_fn("player_uid", move || {
        with(&s, |c| {
            let hash = (c.player.name.bytes()).fold(2_166_136_261u32, |h, b| {
                (h ^ u32::from(b)).wrapping_mul(16_777_619)
            });
            i64::from(hash & 0x7FFF_FFFF)
        })
    });
    let s = shared.clone();
    engine.register_fn("clear_attrs", move |prefix: &str| {
        with(&s, |c| {
            c.player
                .attributes
                .retain(|name, _| !name.starts_with(prefix))
        })
    });
    let s = shared.clone();
    engine.register_fn("team_here", move || with(&s, |c| c.player.team_here));
    let s = shared.clone();
    engine.register_fn("team_all_named", move |name: &str| {
        with(&s, |c| {
            let names = &c.player.team_names;
            !names.is_empty() && names.iter().all(|n| n == name)
        })
    });
    let s = shared.clone();
    engine.register_fn("team_holding", move |item: i64| {
        with(&s, |c| c.player.team_items.get(&item).copied().unwrap_or(0))
    });
    let s = shared.clone();
    engine.register_fn("team_give", move |item: i64| {
        with(&s, |c| c.effects.push(Effect::TeamItem { item, count: 1 }))
    });
    let s = shared.clone();
    engine.register_fn("team_take", move |item: i64| {
        with(&s, |c| c.effects.push(Effect::TeamItem { item, count: -1 }))
    });
    let s = shared.clone();
    engine.register_fn("team_say", move |text: &str| {
        with(&s, |c| c.effects.push(Effect::TeamSay(text.to_owned())))
    });
    let s = shared.clone();
    engine.register_fn("team_travel", move |map: i64, x: i64, y: i64| {
        with(&s, |c| {
            c.effects.push(Effect::TeamTravel { map, cell: (x, y) })
        })
    });
    let s = shared.clone();
    engine.register_fn("client_cue", move |name: &str, a: i64, b: i64| {
        with(&s, |c| {
            c.effects.push(Effect::ClientCue {
                name: name.to_owned(),
                args: [a, b],
            })
        })
    });
    let s = shared.clone();
    engine.register_fn(
        "effect_at",
        move |map: i64, x: i64, y: i64, effect: &str| {
            with(&s, |c| {
                c.effects.push(Effect::EffectAt {
                    map,
                    cell: (x, y),
                    effect: effect.to_owned(),
                })
            })
        },
    );
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
    let roll = move |low: i64, high: i64| {
        let (low, high) = (low.min(high), low.max(high));
        let mut x = seed.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        seed.set(x);
        let span = (high - low) as u64 + 1;
        low + (x % span) as i64
    };
    let random = roll.clone();
    engine.register_fn("random", move |low: i64, high: i64| random(low, high));
    let s = shared.clone();
    engine.register_fn("pick_worn", move |name: &str, op: &str, value: i64| {
        with(&s, |c| {
            let slots: Vec<i64> = (c.player.worn.iter())
                .filter(|(slot, kind)| {
                    let have = match name {
                        "type" => **kind,
                        _ => match c.player.worn_attrs.get(slot).and_then(|a| a.get(name)) {
                            Some(v) => *v,
                            None => return false,
                        },
                    };
                    match op {
                        ">" => have > value,
                        ">=" => have >= value,
                        "<" => have < value,
                        "<=" => have <= value,
                        _ => have == value,
                    }
                })
                .map(|(slot, _)| *slot)
                .collect();
            match slots.len() {
                0 => 0,
                n => slots[roll(0, n as i64 - 1) as usize],
            }
        })
    });
}

/// A starting point for `random` (it differs from run to run).
fn seed() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    nanos | 1
}

pub(crate) fn data(engine: &mut Engine, tables: &Rc<RefCell<Tables>>) {
    let t = tables.clone();
    engine.register_fn("lookup", move |table: &str, key: i64, column: &str| {
        t.borrow_mut().lookup(table, key, column)
    });
    let t = tables.clone();
    engine.register_fn("has_row", move |table: &str, key: i64| -> Fallible<bool> {
        Ok(t.borrow_mut().table(table)?.find(key).is_some())
    });
    let t = tables.clone();
    let draw = std::cell::Cell::new(seed());
    engine.register_fn(
        "pick_row",
        move |table: &str, column: &str, value: i64, weight: &str| -> Fallible<i64> {
            let mut tables = t.borrow_mut();
            let table = tables.table(table)?;
            let col = |name: &str| {
                (table.column(name))
                    .ok_or_else(|| format!("table \"{}\" has no column \"{name}\"", table.name))
            };
            let (c, w) = (col(column)?, col(weight)?);
            let rows: Vec<(i64, i64)> = (table.rows.iter())
                .filter(|r| r.get(c).and_then(|v| v.as_int()) == Some(value))
                .filter_map(|r| Some((r.first()?.as_int()?, r.get(w)?.as_int()?.max(0))))
                .collect();
            let total: i64 = rows.iter().map(|(_, w)| w).sum();
            if total <= 0 {
                return Ok(0);
            }
            let mut x = draw.get();
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            draw.set(x);
            let mut roll = (x % total as u64) as i64;
            for (key, w) in rows {
                if roll < w {
                    return Ok(key);
                }
                roll -= w;
            }
            Ok(0)
        },
    );
}

/// The levels table's name (its rows: a level's experience to the next, and its time).
const LEVELS: &str = "levels";

/// Column `column` of level `level` in the levels table; `None` past the last level.
fn level_value(tables: &Rc<RefCell<Tables>>, level: i64, column: &str) -> Fallible<Option<i64>> {
    let value = tables.borrow_mut().lookup(LEVELS, level, column)?;
    Ok(value.as_int().ok())
}

/// The level and the whole percent into it the player would be at with `exp` more
/// experience: the levels table walked from the player's level (nothing changes).
fn level_after(
    tables: &Rc<RefCell<Tables>>,
    level: i64,
    exp: i64,
    more: i64,
) -> Fallible<(i64, i64)> {
    let mut e = exp.saturating_add(more);
    let (mut l, mut last) = (level, 0);
    while let Some(need) = level_value(tables, l, "exp")? {
        last = need;
        if need <= 0 || e < need {
            break;
        }
        e -= need;
        l += 1;
    }
    Ok((l, e.saturating_mul(100) / last.max(1)))
}

/// Registers the functions that walk the levels table for the player.
fn levels(engine: &mut Engine, shared: &Shared, tables: &Rc<RefCell<Tables>>) {
    let (s, t) = (shared.clone(), tables.clone());
    engine.register_fn("level_after", move |more: i64| -> Fallible<i64> {
        let (level, exp) = with(&s, |c| (c.player.level, c.player.exp))?;
        Ok(level_after(&t, level, exp, more)?.0)
    });
    let (s, t) = (shared.clone(), tables.clone());
    engine.register_fn("percent_after", move |more: i64| -> Fallible<i64> {
        let (level, exp) = with(&s, |c| (c.player.level, c.player.exp))?;
        Ok(level_after(&t, level, exp, more)?.1)
    });
    // The experience that takes the player `units` of the levels' `time` further.
    let (s, t) = (shared.clone(), tables.clone());
    engine.register_fn("exp_for_time", move |units: i64| -> Fallible<i64> {
        let (mut l, cur) = with(&s, |c| (c.player.level, c.player.exp))?;
        let (Some(exp), Some(time)) = (level_value(&t, l, "exp")?, level_value(&t, l, "time")?)
        else {
            return Ok(0);
        };
        let mut left = units + (time as f64 * (cur as f64 / exp.max(1) as f64)) as i64;
        let mut total = 0i64;
        while let (Some(exp), Some(time)) =
            (level_value(&t, l, "exp")?, level_value(&t, l, "time")?)
        {
            if exp <= 0 {
                break;
            }
            if left < time {
                total += (exp as f64 * (left as f64 / time.max(1) as f64)) as i64;
                break;
            }
            left -= time;
            total += exp;
            l += 1;
        }
        Ok(total.max(cur) - cur)
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

/// The player's guild and the others: reading them, and changes for every map.
/// Maps made while playing (players' homes), shared by every map: made, their fields read
/// and set, an owner's found.
fn map_copies(engine: &mut Engine, shared: &Shared) {
    let change = |c: &mut Call, id: i64, copy: MapCopyView| {
        c.effects.push(Effect::World(WorldChange::MapCopy {
            id,
            copy: copy.clone(),
        }));
        c.player.map_copies.insert(id, copy);
    };
    let s = shared.clone();
    engine.register_fn(
        "make_map_copy",
        move |base: i64, x: i64, y: i64, level: i64| {
            with(&s, |c| {
                let id = c
                    .player
                    .map_copies
                    .keys()
                    .next_back()
                    .map_or(FIRST_MAP_COPY, |l| l + 1);
                let copy = MapCopyView {
                    base,
                    owner: c.player.name.clone(),
                    x,
                    y,
                    level,
                };
                change(c, id, copy);
                id
            })
        },
    );
    let s = shared.clone();
    engine.register_fn("map_copy", move |id: i64, field: &str| {
        with(&s, |c| {
            c.player.map_copies.get(&id).map_or(0, |m| match field {
                "base" => m.base,
                "x" => m.x,
                "y" => m.y,
                "level" => m.level,
                _ => 0,
            })
        })
    });
    let s = shared.clone();
    engine.register_fn("set_map_copy", move |id: i64, field: &str, value: i64| {
        with(&s, |c| {
            let Some(mut copy) = c.player.map_copies.get(&id).cloned() else {
                return;
            };
            match field {
                "base" => copy.base = value,
                "x" => copy.x = value,
                "y" => copy.y = value,
                "level" => copy.level = value,
                _ => return,
            }
            change(c, id, copy);
        })
    });
    let s = shared.clone();
    engine.register_fn("map_copy_of", move |owner: &str| {
        with(&s, |c| {
            (c.player.map_copies.iter())
                .rev()
                .find(|(_, m)| m.owner == owner)
                .map_or(0, |(id, _)| *id)
        })
    });
}

/// The first id of maps made while playing (TQ's dynamic maps are over 1,000,000).
const FIRST_MAP_COPY: i64 = 1_000_001;

/// The police wanted list every map shares (TQ's PoliceWanted): players put on and taken off,
/// walked by index, looked for.
fn police(engine: &mut Engine, shared: &Shared) {
    let s = shared.clone();
    engine.register_fn(
        "police_list",
        move |name: &str, level: i64, listed: bool| {
            with(&s, |c| {
                // The player's own guild and PK go with its name.
                let (guild, pk) = match name == c.player.name {
                    true => {
                        let guild = (c.player.guilds.values())
                            .find(|g| g.members.contains_key(&c.player.name))
                            .map(|g| g.name.clone());
                        (guild.unwrap_or_default(), c.player.pk)
                    }
                    false => (String::new(), 0),
                };
                c.player.police.retain(|p| p.name != name);
                if listed {
                    c.player.police.push(PoliceView {
                        name: name.to_owned(),
                        guild: guild.clone(),
                        level,
                        pk,
                    });
                }
                let change = WorldChange::Police {
                    name: name.to_owned(),
                    level,
                    listed,
                    guild,
                    pk,
                };
                c.effects.push(Effect::World(change));
            })
        },
    );
    let s = shared.clone();
    engine.register_fn("police_at", move |index: i64| {
        with(&s, |c| {
            let n = usize::try_from(index).unwrap_or(usize::MAX);
            c.player
                .police
                .get(n)
                .map(|p| p.name.clone())
                .unwrap_or_default()
        })
    });
    let s = shared.clone();
    engine.register_fn("police_has", move |name: &str| {
        with(&s, |c| c.player.police.iter().any(|p| p.name == name))
    });
}

/// The wanted entry at `index` as a board line: its id, target and bounty (`--`: none).
fn wanted_line(c: &Call, index: i64) -> String {
    let n = usize::try_from(index).unwrap_or(usize::MAX);
    c.player.wanted.iter().nth(n).map_or_else(
        || "--".to_owned(),
        |(id, w)| format!("{id:06} {:<15} {}", w.target, w.bounty),
    )
}

/// The police list's name at `index` as a board line: its place, name, guild, level and PK
/// (`--`: none).
fn police_line(c: &Call, index: i64) -> String {
    let n = usize::try_from(index).unwrap_or(usize::MAX);
    c.player.police.get(n).map_or_else(
        || "--".to_owned(),
        |p| {
            format!(
                "{n:03} {:<15} {:<15} {:03} {}",
                p.name, p.guild, p.level, p.pk
            )
        },
    )
}

/// Bounties on killers every map shares (TQ's cq_pk_bonus): recorded on a kill, counted,
/// a reward set by the one killed, claimed by the killer.
fn bounties(engine: &mut Engine, shared: &Shared) {
    let change = |c: &mut Call, id: i64, view: Option<BountyView>| {
        c.effects.push(Effect::World(WorldChange::Bounty {
            id,
            view: view.clone(),
        }));
        match view {
            Some(view) => c.player.bounties.insert(id, view),
            None => c.player.bounties.remove(&id),
        };
    };
    // The player's rows as the one killed (`target`) or the killer (`hunter`).
    let mine = |c: &Call, role: &str| -> Vec<(i64, BountyView)> {
        let name = &c.player.name;
        (c.player.bounties.iter())
            .filter(|(_, b)| {
                if role == "hunter" {
                    &b.hunter == name
                } else {
                    &b.target == name
                }
            })
            .map(|(id, b)| (*id, b.clone()))
            .collect()
    };
    let s = shared.clone();
    engine.register_fn("record_bounty", move |hunter: &str| {
        with(&s, |c| {
            let id = c
                .player
                .bounties
                .keys()
                .next_back()
                .map_or(1, |last| last + 1);
            let view = BountyView {
                target: c.player.name.clone(),
                hunter: hunter.to_owned(),
                kind: 0,
                bonus: 0,
            };
            change(c, id, Some(view));
        })
    });
    let s = shared.clone();
    engine.register_fn("bounty_count", move |role: &str, unset: bool| {
        with(&s, |c| {
            let rows = mine(c, role);
            rows.iter().filter(|(_, b)| (b.bonus == 0) == unset).count() as i64
        })
    });
    let s = shared.clone();
    engine.register_fn("set_bounty", move |kind: i64, bonus: i64| {
        with(&s, |c| {
            let oldest = mine(c, "target").into_iter().find(|(_, b)| b.bonus == 0);
            let Some((id, mut view)) = oldest else {
                return false;
            };
            view.kind = kind;
            view.bonus = bonus;
            change(c, id, Some(view));
            true
        })
    });
    let s = shared.clone();
    engine.register_fn("clear_bounties", move || {
        with(&s, |c| {
            for (id, _) in mine(c, "target").into_iter().filter(|(_, b)| b.bonus == 0) {
                change(c, id, None);
            }
        })
    });
    // The oldest reward the player may claim: its kind and amount (0: none); taken.
    let s = shared.clone();
    engine.register_fn("claimable_bounty", move |kind: bool| {
        with(&s, |c| {
            let oldest = mine(c, "hunter").into_iter().find(|(_, b)| b.bonus > 0);
            oldest.map_or(0, |(_, b)| if kind { b.kind } else { b.bonus })
        })
    });
    let s = shared.clone();
    engine.register_fn("take_bounty", move || {
        with(&s, |c| {
            if let Some((id, _)) = mine(c, "hunter").into_iter().find(|(_, b)| b.bonus > 0) {
                change(c, id, None);
            }
        })
    });
}

/// The name of member `index` (from 0) of the player's guild, empty past the last.
fn guild_member_at(c: &Call, index: i64) -> String {
    let name = &c.player.name;
    let guild = c
        .player
        .guilds
        .values()
        .find(|g| g.members.contains_key(name));
    let n = usize::try_from(index).unwrap_or(usize::MAX);
    guild
        .and_then(|g| g.members.keys().nth(n))
        .cloned()
        .unwrap_or_default()
}

/// The wanted list every map shares: reading it, entries added, raised and removed.
fn wanted(engine: &mut Engine, shared: &Shared) {
    // A change seen at once by the script, and sent on for every map.
    let change = |c: &mut Call, id: i64, view: WantedView| {
        c.effects.push(Effect::World(WorldChange::Wanted {
            id,
            target: view.target.clone(),
            payer: view.payer.clone(),
            bounty: view.bounty,
        }));
        if view.bounty > 0 {
            c.player.wanted.insert(id, view);
        } else {
            c.player.wanted.remove(&id);
        }
    };
    let s = shared.clone();
    engine.register_fn("wanted_at", move |index: i64| {
        with(&s, |c| {
            let n = usize::try_from(index).unwrap_or(usize::MAX);
            c.player.wanted.keys().nth(n).copied().unwrap_or(0)
        })
    });
    let s = shared.clone();
    engine.register_fn("wanted_bounty", move |id: i64| {
        with(&s, |c| c.player.wanted.get(&id).map_or(0, |w| w.bounty))
    });
    let s = shared.clone();
    engine.register_fn("wanted_target", move |id: i64| {
        with(&s, |c| {
            c.player
                .wanted
                .get(&id)
                .map(|w| w.target.clone())
                .unwrap_or_default()
        })
    });
    let s = shared.clone();
    engine.register_fn("add_wanted", move |target: &str, bounty: i64| {
        with(&s, |c| {
            let id = c
                .player
                .wanted
                .keys()
                .next_back()
                .map_or(1, |last| last + 1);
            let view = WantedView {
                target: target.to_owned(),
                payer: c.player.name.clone(),
                bounty,
            };
            change(c, id, view);
            id
        })
    });
    let s = shared.clone();
    engine.register_fn("set_wanted_bounty", move |id: i64, bounty: i64| {
        with(&s, |c| {
            if let Some(mut view) = c.player.wanted.get(&id).cloned() {
                view.bounty = bounty;
                change(c, id, view);
            }
        })
    });
}

fn guilds(engine: &mut Engine, shared: &Shared) {
    // The player's guild and rank there (0: none).
    let membership = |c: &Call| {
        (c.player.guilds.iter())
            .find_map(|(id, g)| g.members.get(&c.player.name).map(|r| (*id, *r)))
            .unwrap_or((0, 0))
    };
    let s = shared.clone();
    engine.register_fn("guild_id", move || with(&s, |c| membership(c).0));
    let s = shared.clone();
    engine.register_fn("guild_member", move |index: i64| {
        with(&s, |c| guild_member_at(c, index))
    });
    let s = shared.clone();
    engine.register_fn("guild_rank", move || with(&s, |c| membership(c).1));
    let read = |s: Shared, f: fn(&GuildView) -> i64| {
        move |id: i64| with(&s, |c| c.player.guilds.get(&id).map_or(0, f))
    };
    engine.register_fn("guild_fund", read(shared.clone(), |g| g.fund));
    engine.register_fn("guild_master", read(shared.clone(), |g| g.master));
    engine.register_fn(
        "guild_members",
        read(shared.clone(), |g| g.members.len() as i64),
    );
    let s = shared.clone();
    engine.register_fn("guild_name", move |id: i64| {
        with(&s, |c| {
            c.player
                .guilds
                .get(&id)
                .map(|g| g.name.clone())
                .unwrap_or_default()
        })
    });
    let s = shared.clone();
    engine.register_fn("guild_named", move |name: &str| {
        with(&s, |c| {
            (c.player.guilds.iter())
                .find(|(_, g)| g.name.eq_ignore_ascii_case(name))
                .map_or(0, |(id, _)| *id)
        })
    });
    let s = shared.clone();
    engine.register_fn("next_guild", move |after: i64| {
        with(&s, |c| {
            (c.player.guilds.iter())
                .find(|(id, g)| **id > after && !g.members.is_empty())
                .map_or(0, |(id, _)| *id)
        })
    });
    let s = shared.clone();
    engine.register_fn("map_owner", move |map: i64| {
        with(&s, |c| c.player.map_owners.get(&map).copied().unwrap_or(0))
    });
    let s = shared.clone();
    engine.register_fn(
        "guild_relation",
        move |guild: i64, other: i64, ally: bool| {
            with(&s, |c| {
                (c.player.guilds.get(&guild))
                    .is_some_and(|g| if ally { &g.allies } else { &g.enemies }.contains(&other))
            })
        },
    );
    // Changes: made here at once, and for every map.
    let change = |c: &mut Call, change: WorldChange| {
        apply_guild_change(&mut c.player, &change);
        c.effects.push(Effect::World(change));
    };
    let s = shared.clone();
    engine.register_fn("create_guild", move |name: &str, fund: i64| {
        with(&s, |c| {
            let taken = c
                .player
                .guilds
                .values()
                .any(|g| g.name.eq_ignore_ascii_case(name));
            let len = name.chars().count();
            if membership(c).0 != 0 || taken || !(2..16).contains(&len) {
                return false;
            }
            let id = c.player.guilds.keys().max().copied().unwrap_or(0) + 1;
            let leader = c.player.name.clone();
            let create = WorldChange::CreateGuild {
                id,
                name: name.to_owned(),
                leader,
                rank: GUILD_LEADER,
                fund: fund.max(0),
                master: 0,
            };
            change(c, create);
            true
        })
    });
    // A sub-guild under the player's guild (which it leads), led by its first teammate here:
    // one of its guild, of `level` at least. The parent pays `money`; the branch starts with
    // `fund`.
    let s = shared.clone();
    engine.register_fn(
        "create_sub_guild",
        move |name: &str, level: i64, money: i64, fund: i64| {
            with(&s, |c| {
                let (parent, rank) = membership(c);
                let Some((mate, mate_level)) = first_teammate(c) else {
                    return false;
                };
                let parent_fund = c.player.guilds.get(&parent).map_or(0, |g| g.fund);
                let mates_guild = (c.player.guilds.iter())
                    .find(|(_, g)| g.members.contains_key(&mate))
                    .map(|(id, _)| *id);
                let taken = (c.player.guilds.values()).any(|g| g.name.eq_ignore_ascii_case(name));
                let len = name.chars().count();
                if parent == 0
                    || rank != GUILD_LEADER
                    || parent_fund < money
                    || mates_guild != Some(parent)
                    || mate_level < level
                    || taken
                    || !(2..16).contains(&len)
                {
                    return false;
                }
                let id = c.player.guilds.keys().max().copied().unwrap_or(0) + 1;
                let create = WorldChange::CreateGuild {
                    id,
                    name: name.to_owned(),
                    leader: mate,
                    rank: GUILD_LEADER,
                    fund: fund.max(0),
                    master: parent,
                };
                change(c, create);
                change(
                    c,
                    WorldChange::GuildFund {
                        guild: parent,
                        amount: -money,
                    },
                );
                true
            })
        },
    );
    // Sub-guild `name` of the player's guild handed to its first teammate here (of `level`
    // at least).
    let s = shared.clone();
    engine.register_fn("change_sub_leader", move |name: &str, level: i64| {
        with(&s, |c| {
            let (parent, rank) = membership(c);
            let Some((mate, mate_level)) = first_teammate(c) else {
                return false;
            };
            let sub = (c.player.guilds.iter())
                .find(|(_, g)| g.name.eq_ignore_ascii_case(name) && g.master == parent)
                .map(|(id, _)| *id);
            let Some(sub) = sub.filter(|_| parent != 0 && rank == GUILD_LEADER) else {
                return false;
            };
            if mate_level < level {
                return false;
            }
            change(
                c,
                WorldChange::GuildLeader {
                    guild: sub,
                    name: mate,
                },
            );
            true
        })
    });
    let s = shared.clone();
    engine.register_fn("destroy_guild", move || {
        with(&s, |c| {
            let (id, rank) = membership(c);
            let has_branch = c.player.guilds.values().any(|g| g.master == id);
            if id == 0 || rank != GUILD_LEADER || has_branch {
                return false;
            }
            change(c, WorldChange::DestroyGuild(id));
            true
        })
    });
    let s = shared.clone();
    engine.register_fn("add_guild_fund", move |guild: i64, amount: i64| {
        with(&s, |c| {
            if !c.player.guilds.contains_key(&guild) {
                return false;
            }
            change(c, WorldChange::GuildFund { guild, amount });
            true
        })
    });
    let s = shared.clone();
    engine.register_fn("set_guild_rank", move |name: &str, rank: i64| {
        with(&s, |c| {
            let id = membership(c).0;
            let member = (c.player.guilds.get(&id)).is_some_and(|g| g.members.contains_key(name));
            if !member {
                return false;
            }
            let name = name.to_owned();
            change(
                c,
                WorldChange::Member {
                    guild: id,
                    name,
                    rank,
                },
            );
            true
        })
    });
    let s = shared.clone();
    engine.register_fn("join_guild", move |guild: i64, rank: i64| {
        with(&s, |c| {
            if !c.player.guilds.contains_key(&guild) || rank <= 0 {
                return false;
            }
            let name = c.player.name.clone();
            change(c, WorldChange::Member { guild, name, rank });
            true
        })
    });
    let s = shared.clone();
    engine.register_fn(
        "set_guild_relation",
        move |other: i64, ally: bool, on: bool| {
            with(&s, |c| {
                let guild = membership(c).0;
                if guild == 0 || guild == other || !c.player.guilds.contains_key(&other) {
                    return false;
                }
                change(
                    c,
                    WorldChange::Relation {
                        guild,
                        other,
                        ally,
                        on,
                    },
                );
                true
            })
        },
    );
    let s = shared.clone();
    engine.register_fn("set_map_owner", move |map: i64, guild: i64| {
        with(&s, |c| change(c, WorldChange::MapOwner { map, guild }))
    });
}

/// The player's first teammate here: its name and level.
fn first_teammate(c: &Call) -> Option<(String, i64)> {
    let name = c.player.team_names.first()?.clone();
    Some((name, c.player.team_levels.first().copied().unwrap_or(0)))
}

/// A guild's leader's rank.
pub const GUILD_LEADER: i64 = 100;

/// `change` as the player's own view of the guilds sees it at once.
fn apply_guild_change(p: &mut PlayerState, change: &WorldChange) {
    match change {
        WorldChange::CreateGuild {
            id,
            name,
            leader,
            rank,
            fund,
            master,
        } => {
            for g in p.guilds.values_mut() {
                g.members.remove(leader);
            }
            let view = GuildView {
                name: name.clone(),
                leader: leader.clone(),
                fund: *fund,
                master: *master,
                members: BTreeMap::from([(leader.clone(), *rank)]),
                ..GuildView::default()
            };
            p.guilds.insert(*id, view);
        }
        WorldChange::DestroyGuild(id) => {
            p.guilds.remove(id);
        }
        WorldChange::GuildFund { guild, amount } => {
            if let Some(g) = p.guilds.get_mut(guild) {
                g.fund = (g.fund + amount).max(0);
            }
        }
        WorldChange::Member { guild, name, rank } => {
            if *rank > 0 {
                for g in p.guilds.values_mut() {
                    g.members.remove(name);
                }
            }
            if let Some(g) = p.guilds.get_mut(guild) {
                if *rank > 0 {
                    g.members.insert(name.clone(), *rank);
                } else {
                    g.members.remove(name);
                }
            }
        }
        WorldChange::MapOwner { map, guild } => {
            p.map_owners.insert(*map, *guild);
        }
        WorldChange::Relation { .. }
        | WorldChange::Flag { .. }
        | WorldChange::Wanted { .. }
        | WorldChange::Bounty { .. }
        | WorldChange::Police { .. }
        | WorldChange::GuildLeader { .. }
        | WorldChange::GuildNotice { .. }
        | WorldChange::Contribution { .. }
        | WorldChange::Friendship { .. }
        | WorldChange::Enemy { .. }
        | WorldChange::MapCopy { .. }
        | WorldChange::Online { .. }
        | WorldChange::Deliver { .. }
        | WorldChange::Post { .. }
        | WorldChange::Partnership { .. }
        | WorldChange::Mentorship { .. }
        | WorldChange::Broadcast { .. }
        | WorldChange::Donation { .. }
        | WorldChange::Deposit { .. }
        | WorldChange::MentorGift { .. }
        | WorldChange::Delivered { .. } => {}
    }
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
    engine.register_fn("held_of", move |kind: i64| {
        with(&s, |c| {
            (c.player.held.iter())
                .filter(|(_, i)| i.kind == kind && !i.worn)
                .map(|(id, _)| Dynamic::from(*id))
                .collect::<rhai::Array>()
        })
    });
    let s = shared.clone();
    engine.register_fn("erase_item", move |id: i64| {
        with(&s, |c| match c.player.held.get(&id) {
            Some(item) if !item.worn => {
                let (kind, count) = (item.kind, item.count.max(1));
                c.player.held.remove(&id);
                c.player.take_item(kind, count);
                c.effects.push(Effect::TookItem { item: kind, count });
                true
            }
            _ => false,
        })
    });
    let s = shared.clone();
    engine.register_fn(
        "set_given_attr",
        move |item: i64, name: &str, value: i64| {
            with(&s, |c| {
                c.effects.push(Effect::GivenAttr {
                    item,
                    name: name.to_owned(),
                    value,
                })
            })
        },
    );
    let s = shared.clone();
    engine.register_fn("item_worn", move |id: i64| {
        with(&s, |c| c.player.held.get(&id).is_some_and(|i| i.worn))
    });
}
