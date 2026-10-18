use super::*;
use c3_scene::{Grid, Npc, Spawn};

const SLIME: u32 = 1;
const WOLF: u32 = 2;
const POTION: u32 = 100;
const SHOP_NPC: u32 = 7;
const ARMOR: u32 = 200;
/// Stacks by 50.
const ARROW: u32 = 300;
/// Worn, priced in gems.
const CHARM: u32 = 400;

struct TestRules {
    monsters: Vec<MonsterType>,
    potion: ItemType,
    armor: ItemType,
    others: Vec<ItemType>,
}

fn monster(id: u32, life: u32, attack: u32, aggressive: bool) -> MonsterType {
    MonsterType {
        id,
        name: format!("monster {id}"),
        look: 100 + id,
        level: 1,
        stats: Stats {
            max_life: life,
            attack_min: attack,
            attack_max: attack,
            attack_range: 1,
            attack_ms: 1000,
            step_ms: 500,
            ..Default::default()
        },
        view_range: 6,
        aggressive,
    }
}

impl Rules for TestRules {
    fn monster(&self, kind: u32) -> Option<&MonsterType> {
        self.monsters.iter().find(|m| m.id == kind)
    }
    fn item(&self, kind: u32) -> Option<&ItemType> {
        match kind {
            POTION => Some(&self.potion),
            ARMOR => Some(&self.armor),
            _ => self.others.iter().find(|t| t.id == kind),
        }
    }
    fn player_stats(&self, level: u32) -> Stats {
        Stats {
            max_life: 100 + 10 * level,
            attack_min: 10,
            attack_max: 10,
            attack_range: 1,
            attack_ms: 800,
            step_ms: 400,
            ..Default::default()
        }
    }
    fn level_exp(&self, level: u32) -> Option<u64> {
        (level < 5).then_some(25)
    }
    fn hit(&self, attacker: &Fighter, target: &Fighter, rng: &mut Rng) -> u32 {
        let a = rng.range(
            attacker.stats.attack_min.into(),
            attacker.stats.attack_max.into(),
        );
        (a as u32).saturating_sub(target.stats.defence).max(1)
    }
    fn hit_exp(&self, _level: u32, _target: &Fighter, damage: u32) -> u64 {
        damage.into()
    }
    fn kill_exp(&self, _level: u32, _target: &Fighter) -> u64 {
        10
    }
    fn drops(&self, _kind: u32, _rng: &mut Rng) -> Vec<Drop> {
        vec![Drop::Money(5), Drop::Item(POTION)]
    }
    fn shop(&self, npc: u32) -> &[u32] {
        if npc == SHOP_NPC {
            &[POTION, ARROW, CHARM]
        } else {
            &[]
        }
    }
}

fn rules() -> Box<dyn Rules> {
    Box::new(TestRules {
        monsters: vec![monster(SLIME, 30, 5, false), monster(WOLF, 500, 40, true)],
        potion: ItemType {
            id: POTION,
            name: "Potion".into(),
            price: 30,
            restores_life: 50,
            restores_mana: 0,
            slot: None,
            bonus: Stats::default(),
            ..ItemType::default()
        },
        armor: ItemType {
            id: ARMOR,
            name: "Armor".into(),
            price: 100,
            restores_life: 0,
            restores_mana: 0,
            slot: Some(3),
            bonus: Stats {
                defence: 4,
                max_life: 20,
                ..Default::default()
            },
            durability: 3,
            ..ItemType::default()
        },
        others: vec![
            ItemType {
                id: ARROW,
                name: "Arrow".into(),
                price: 1,
                stack: 50,
                ..ItemType::default()
            },
            ItemType {
                id: CHARM,
                name: "Charm".into(),
                price: 6,
                slot: Some(5),
                bonus: Stats {
                    defence: 10,
                    ..Default::default()
                },
                currency: "Gems".into(),
                ..ItemType::default()
            },
        ],
    })
}

fn spawn(id: u32, monster: u32, min: (i32, i32), max: (i32, i32), alive: u32) -> Spawn {
    Spawn {
        id,
        monster,
        min,
        max,
        max_alive: alive,
        respawn_secs: 5,
        per_wave: 1,
        extra: Default::default(),
    }
}

fn shopkeeper() -> Npc {
    Npc {
        id: SHOP_NPC,
        name: "Shopkeeper".into(),
        cell: (2, 2),
        direction: 0,
        look: 1,
        kind: 1,
        script: String::new(),
        extra: Default::default(),
    }
}

fn run_until(sim: &mut Sim, max_ms: u64, done: impl Fn(&Sim) -> bool) -> bool {
    let end = sim.now_ms() + max_ms;
    while sim.now_ms() < end {
        sim.advance(sim.settings().tick_ms);
        if done(sim) {
            return true;
        }
    }
    false
}

#[test]
fn spawns_fill_and_monsters_stay_near_home() {
    let mut sim = Sim::new(
        Grid::new(60, 60),
        vec![],
        vec![
            spawn(1, SLIME, (10, 10), (14, 14), 4),
            spawn(2, 999, (0, 0), (1, 1), 1),
        ],
        rules(),
        1,
    );
    assert_eq!(sim.monsters().count(), 4);
    let events = sim.take_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Warning(w) if w.contains("999")))
    );
    sim.advance(60_000);
    for m in sim.monsters() {
        assert!(distance(m.cell, m.home) <= 4, "{m:?} wandered off");
        assert!((10..=14).contains(&m.home.0));
    }
}

#[test]
fn players_kill_monsters_for_exp_drops_and_respawns() {
    let mut sim = Sim::new(
        Grid::new(60, 60),
        vec![],
        vec![spawn(1, SLIME, (20, 20), (20, 20), 1)],
        rules(),
        2,
    );
    let hero = sim.add_player("Hero", (10, 10), 1);
    let slime = sim.monsters().next().unwrap().id;
    sim.attack(hero, slime);
    assert!(run_until(&mut sim, 30_000, |s| !s
        .monster(slime)
        .unwrap()
        .is_alive()));
    let p = sim.player(hero).unwrap();
    assert!(distance(p.cell, (20, 20)) <= 1, "walked up to it");
    // 30 life = 30 exp from hits + 10 for the kill: 40, one level (25) up.
    assert_eq!((p.level, p.exp), (2, 15));
    assert_eq!(p.life, p.stats.max_life, "a level up refills life");
    let events = sim.take_events();
    assert!(events.contains(&Event::LeveledUp {
        player: hero,
        level: 2
    }));
    assert!(events.contains(&Event::Died {
        id: slime,
        killer: Some(hero)
    }));
    assert_eq!(sim.ground_items().count(), 2, "money and a potion");

    for item in sim.ground_items().map(|i| i.id).collect::<Vec<_>>() {
        sim.pick_up(hero, item);
        assert!(run_until(&mut sim, 10_000, |s| s
            .ground_item(item)
            .is_none()));
    }
    let p = sim.player(hero).unwrap();
    assert_eq!((p.money, p.bag.len()), (5, 1));

    // The body fades, then the area grows a new slime.
    assert!(run_until(&mut sim, 10_000, |s| s.monster(slime).is_none()));
    assert!(run_until(&mut sim, 10_000, |s| s.monsters().count() == 1));
}

#[test]
fn aggressive_monsters_kill_players_who_come_back() {
    let mut sim = Sim::new(
        Grid::new(60, 60),
        vec![],
        vec![spawn(1, WOLF, (12, 12), (12, 12), 1)],
        rules(),
        3,
    );
    let hero = sim.add_player("Hero", (10, 10), 1);
    assert!(run_until(&mut sim, 30_000, |s| !s
        .player(hero)
        .unwrap()
        .is_alive()));
    assert!(
        sim.take_events()
            .iter()
            .any(|e| matches!(e, Event::Died { id, .. } if *id == hero))
    );
    // Monsters leave the dead alone; the player revives with full life.
    let revive = ms(sim.settings().revive_seconds);
    assert!(run_until(&mut sim, revive + 1000, |s| s
        .player(hero)
        .unwrap()
        .is_alive()));
    let p = sim.player(hero).unwrap();
    assert_eq!(p.life, p.stats.max_life);
}

#[test]
fn shops_bags_and_potions() {
    let mut sim = Sim::new(Grid::new(60, 60), vec![shopkeeper()], vec![], rules(), 4);
    let hero = sim.add_player("Hero", (3, 3), 1);
    sim.set_money(hero, 70);
    sim.buy(hero, SHOP_NPC, POTION);
    sim.buy(hero, SHOP_NPC, POTION);
    sim.buy(hero, SHOP_NPC, POTION);
    let p = sim.player(hero).unwrap();
    assert_eq!((p.money, p.bag.len()), (10, 2), "the third was too dear");
    assert!(sim.take_events().iter().any(
        |e| matches!(e, Event::Message { text, .. } if text == "You don't have enough Gold.")
    ));

    let first = sim.player(hero).unwrap().bag[0].id;
    sim.sell(hero, SHOP_NPC, first);
    assert_eq!(sim.player(hero).unwrap().money, 20);

    // A potion heals up to the most life.
    sim.players.get_mut(&hero).unwrap().life = 10;
    let potion = sim.player(hero).unwrap().bag[0].id;
    sim.use_item(hero, potion);
    let p = sim.player(hero).unwrap();
    assert_eq!((p.life, p.bag.len()), (60, 0));

    for _ in 0..sim.bag_size() {
        assert!(sim.give_item(hero, POTION));
    }
    assert!(!sim.give_item(hero, POTION), "the bag is full");
    assert!(sim.take_items(hero, POTION, 3));
    assert_eq!(sim.player(hero).unwrap().bag.len(), sim.bag_size() - 3);

    // Out of reach, the shop doesn't trade.
    sim.place(hero, (40, 40));
    sim.set_money(hero, 1000);
    sim.take_items(hero, POTION, 10);
    sim.buy(hero, SHOP_NPC, POTION);
    assert_eq!(sim.player(hero).unwrap().money, 1000);
}

#[test]
fn equipment_adds_to_stats_and_swaps() {
    let mut sim = Sim::new(Grid::new(20, 20), vec![], vec![], rules(), 5);
    let hero = sim.add_player("Hero", (3, 3), 1);
    let base = sim.player(hero).unwrap().stats;
    assert!(!sim.equip(hero, POTION), "potions aren't worn");
    assert!(sim.equip(hero, ARMOR));
    let p = sim.player(hero).unwrap();
    assert_eq!(
        (p.stats.defence, p.stats.max_life),
        (base.defence + 4, base.max_life + 20)
    );
    // Wearing a second armor from the bag puts the first one in the bag.
    sim.give_item(hero, ARMOR);
    let spare = sim.player(hero).unwrap().bag[0].id;
    sim.use_item(hero, spare);
    let p = sim.player(hero).unwrap();
    assert_eq!((p.equipment.len(), p.bag.len()), (1, 1));
    assert_eq!(p.equipment[0].id, spare);
    let worn = p.equipment[0].id;
    sim.unequip(hero, worn);
    let p = sim.player(hero).unwrap();
    assert_eq!((p.equipment.len(), p.stats.defence), (0, base.defence));
    assert!(p.life <= p.stats.max_life);
}

#[test]
fn unreachable_targets_are_given_up() {
    // A wall of blocked cells between the hero and the slime.
    let mut grid = Grid::new(30, 30);
    for y in 0..30 {
        grid.get_mut((15, y)).unwrap().flags = 1;
    }
    let mut sim = Sim::new(
        grid,
        vec![],
        vec![spawn(1, SLIME, (20, 10), (20, 10), 1)],
        rules(),
        6,
    );
    let hero = sim.add_player("Hero", (14, 10), 1);
    let slime = sim.monsters().next().unwrap().id;
    sim.attack(hero, slime);
    sim.advance(1000);
    assert_eq!(sim.player(hero).unwrap().task, Task::Idle);
    assert!(
        sim.take_events()
            .iter()
            .any(|e| matches!(e, Event::Message { text, .. } if text.contains("reach")))
    );
}

const SHOP_SCRIPT: &str = r#"
fn talk() {
    text("Hello " + player_name());
    option("Show me your goods", "shop");
    option("A gift?", "gift");
    option("Bye");
}
fn shop() { open_shop(); }
fn gift() {
    give_item(100);
    give_money(5);
    set_flag("gifted", 1);
    message("Enjoy!");
}
"#;

#[test]
fn players_talk_to_npcs_and_scripts_change_them() {
    let dir = std::env::temp_dir().join(format!("c3-game-talk-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("npc")).unwrap();
    std::fs::write(dir.join("npc/shop.rhai"), SHOP_SCRIPT).unwrap();
    let mut npc = shopkeeper();
    npc.script = "npc/shop".into();
    let mut sim = Sim::new(Grid::new(30, 30), vec![npc], vec![], rules(), 7);
    sim.set_scripts(c3_script::ScriptHost::new(&dir, None), vec![]);
    let hero = sim.add_player("Hero", (12, 12), 1);

    sim.talk(hero, SHOP_NPC);
    assert!(run_until(&mut sim, 20_000, |s| s
        .player(hero)
        .unwrap()
        .dialogue
        .is_some()));
    let p = sim.player(hero).unwrap();
    assert!(distance(p.cell, (2, 2)) <= 1, "walked up to the NPC");
    assert_eq!(p.dialogue.as_ref().unwrap().page.lines, ["Hello Hero"]);

    sim.answer(hero, 1);
    let p = sim.player(hero).unwrap();
    assert!(
        p.dialogue.is_none(),
        "the gift page is empty, so the dialogue ends"
    );
    assert_eq!((p.money, p.bag.len(), p.flags["gifted"]), (5, 1, 1));
    let events = sim.take_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Notice { text, .. } if text == "Enjoy!"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Notice { text, .. } if text.contains("Potion")))
    );

    sim.talk(hero, SHOP_NPC);
    sim.advance(sim.settings().tick_ms);
    sim.answer(hero, 0);
    assert_eq!(sim.player(hero).unwrap().shop, Some(SHOP_NPC));
    assert!(sim.take_events().contains(&Event::ShopOpened {
        player: hero,
        npc: SHOP_NPC
    }));
    // Walking off closes the shop.
    sim.walk(hero, (20, 20));
    assert_eq!(sim.player(hero).unwrap().shop, None);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn chat_commands() {
    let mut sim = Sim::new(Grid::new(20, 20), vec![], vec![], rules(), 8);
    let hero = sim.add_player("Hero", (3, 3), 1);
    assert!(sim.command(hero, "/money 77").is_ok());
    assert!(
        sim.command(hero, "/item 100 3")
            .unwrap()
            .contains("3 × Potion")
    );
    assert!(sim.command(hero, "/level 4").is_ok());
    assert!(sim.command(hero, "/goto 10 11").is_ok());
    let p = sim.player(hero).unwrap();
    assert_eq!(
        (p.money, p.bag.len(), p.level, p.cell),
        (77, 3, 4, (10, 11))
    );
    assert!(sim.command(hero, "/item 999").is_err());
    assert!(sim.command(hero, "/money lots").is_err());
    assert!(sim.command(hero, "/fly").is_err());
}

#[test]
fn settings_decide_game_masters_and_the_bag() {
    let mut sim = Sim::new(Grid::new(20, 20), vec![], vec![], rules(), 8);
    let hero = sim.add_player("Alice", (3, 3), 1);
    sim.set_settings(GameplaySettings {
        gm_everyone: false,
        gm_accounts: vec!["admin".into()],
        bag_size: 2,
        ..GameplaySettings::default()
    });
    assert!(sim.command(hero, "/money 5").is_err());
    let admin = sim.add_player("Admin", (4, 4), 1);
    assert!(
        sim.command(admin, "/item 100 9")
            .unwrap()
            .contains("2 × Potion")
    );
    assert_eq!(sim.player(admin).unwrap().bag.len(), 2);
}

#[test]
fn kills_count_towards_taken_kill_quests() {
    let mut sim = Sim::new(
        Grid::new(60, 60),
        vec![],
        vec![spawn(1, SLIME, (14, 10), (14, 10), 1)],
        rules(),
        4,
    );
    let quest = |id, monster| c3_script::Quest {
        id,
        name: format!("quest {id}"),
        description: String::new(),
        giver: 0,
        min_level: 0,
        goal: c3_script::Goal::Kill { monster, count: 1 },
        reward_money: 0,
        reward_exp: 0,
        reward_item: 0,
    };
    sim.set_quests(vec![
        quest(1, SLIME.into()),
        quest(2, WOLF.into()),
        quest(3, SLIME.into()),
    ]);
    let hero = sim.add_player("Hero", (10, 10), 1);
    for id in [1, 2] {
        let p = sim.players.get_mut(&hero).unwrap();
        p.flags
            .insert(c3_script::quest::state_flag(id), c3_script::quest::TAKEN);
    }
    let slime = sim.monsters().next().unwrap().id;
    sim.attack(hero, slime);
    assert!(run_until(&mut sim, 30_000, |s| !s
        .monster(slime)
        .unwrap()
        .is_alive()));
    let p = sim.player(hero).unwrap();
    let progress = |id| p.quest_progress(&sim.quests()[id]);
    assert_eq!(progress(0), (1, 1), "taken, right monster");
    assert_eq!(progress(1), (0, 1), "another monster");
    assert_eq!(progress(2), (0, 1), "not taken");
    assert!(
        sim.take_events()
            .iter()
            .any(|e| matches!(e, Event::Notice { text, .. } if text.starts_with("quest 1: done")))
    );
}

#[test]
fn portals_place_here_and_send_travellers_away() {
    use c3_scene::{Portal, PortalTarget};
    let mut sim = Sim::new(Grid::new(30, 30), vec![], vec![], rules(), 8);
    sim.set_map_ids(vec![1]);
    sim.set_portals(&[
        // To a cell of this map.
        Portal {
            cell: (5, 5),
            id: 0,
            to: Some(PortalTarget {
                map: 1,
                cell: (20, 20),
            }),
        },
        // To another map.
        Portal {
            cell: (10, 3),
            id: 1,
            to: Some(PortalTarget {
                map: 2,
                cell: (7, 8),
            }),
        },
        // Leads nowhere: not a portal to the game.
        Portal {
            cell: (12, 12),
            id: 2,
            to: None,
        },
    ]);
    assert!(sim.portal_at((12, 12)).is_none());
    let hero = sim.add_player("Hero", (2, 2), 1);
    sim.input(hero, Input::Walk((5, 5)));
    assert!(run_until(&mut sim, 10_000, |s| s
        .player(hero)
        .unwrap()
        .cell
        == (20, 20)));
    sim.take_events();
    sim.input(hero, Input::EnterPortal((10, 3)));
    let travelled = run_until(&mut sim, 20_000, |s| {
        s.events.iter().any(|e| {
            matches!(
                e,
                Event::Travel {
                    map: 2,
                    cell: (7, 8),
                    ..
                }
            )
        })
    });
    assert!(travelled);
    // The host moves the traveller; until then it stands by the portal.
    assert!(distance(sim.player(hero).unwrap().cell, (10, 3)) <= portals::PORTAL_REACH);
}

#[test]
fn items_stack_up_to_their_type() {
    let mut sim = Sim::new(Grid::new(20, 20), vec![], vec![], rules(), 6);
    let hero = sim.add_player("Hero", (3, 3), 1);
    assert_eq!(sim.give_items(hero, ARROW, 120), 120);
    let counts = |s: &Sim| -> Vec<u32> {
        s.player(hero)
            .unwrap()
            .bag
            .iter()
            .map(|i| i.count)
            .collect()
    };
    assert_eq!(counts(&sim), [50, 50, 20]);
    assert!(sim.take_items(hero, ARROW, 60));
    assert_eq!(counts(&sim), [40, 20]);
    assert!(!sim.take_items(hero, ARROW, 61), "only 60 left");
    assert_eq!(sim.give_items(hero, ARROW, 15), 15);
    assert_eq!(counts(&sim), [50, 25], "onto the stacks first");
    // Sold one at a time; a save keeps the stacks.
    let first = sim.player(hero).unwrap().bag[0].id;
    sim.place(hero, (3, 3));
    let save = sim.save_player(hero).unwrap();
    assert_eq!(
        save.bag.iter().map(|i| i.count).collect::<Vec<_>>(),
        [50, 25]
    );
    let mut other = Sim::new(Grid::new(20, 20), vec![shopkeeper()], vec![], rules(), 7);
    let back = other.add_saved_player(&save);
    assert_eq!(other.player(back).unwrap().bag.len(), 2);
    let first_back = other.player(back).unwrap().bag[0].id;
    other.sell(back, SHOP_NPC, first_back);
    assert_eq!(other.player(back).unwrap().bag[0].count, 49);
    let _ = first;
}

#[test]
fn worn_items_wear_out_and_quality_adds() {
    let mut sim = Sim::new(
        Grid::new(20, 20),
        vec![],
        vec![spawn(1, WOLF, (6, 3), (6, 3), 1)],
        rules(),
        8,
    );
    let mut settings = sim.settings().clone();
    settings.wear_percent = 100;
    sim.set_settings(settings);
    let hero = sim.add_player("Hero", (3, 3), 4);
    let base = sim.player(hero).unwrap().stats.defence;
    assert!(sim.equip(hero, ARMOR));
    assert_eq!(sim.player(hero).unwrap().stats.defence, base + 4);
    // The wolf's bites wear the armour (3 points) until it breaks and gives nothing.
    assert!(run_until(&mut sim, 30_000, |s| s
        .player(hero)
        .unwrap()
        .equipment[0]
        .durability
        == 0));
    assert_eq!(sim.player(hero).unwrap().stats.defence, base);
    assert!(
        sim.take_events()
            .iter()
            .any(|e| matches!(e, Event::Message { text, .. } if text == "Your Armor broke."))
    );
    // Each quality step adds 10 % of what an item gives.
    let mut save = sim.save_player(hero).unwrap();
    save.equipment = vec![SavedItem {
        quality: 2,
        ..CHARM.into()
    }];
    let mut other = Sim::new(Grid::new(20, 20), vec![], vec![], rules(), 9);
    let back = other.add_saved_player(&save);
    assert_eq!(other.player(back).unwrap().stats.defence, base + 12);
}

#[test]
fn shops_take_other_currencies() {
    let mut sim = Sim::new(Grid::new(20, 20), vec![shopkeeper()], vec![], rules(), 10);
    let hero = sim.add_player("Hero", (3, 3), 1);
    sim.set_money(hero, 1000);
    sim.buy(hero, SHOP_NPC, CHARM);
    assert!(
        sim.player(hero).unwrap().bag.is_empty(),
        "money doesn't buy it"
    );
    assert!(
        sim.take_events()
            .iter()
            .any(|e| matches!(e, Event::Message { text, .. } if text.contains("Gems")))
    );
    sim.players
        .get_mut(&hero)
        .unwrap()
        .wallet
        .insert("Gems".into(), 7);
    sim.buy(hero, SHOP_NPC, CHARM);
    let p = sim.player(hero).unwrap();
    assert_eq!((p.bag.len(), p.wallet["Gems"], p.money), (1, 1, 1000));
    let charm = p.bag[0].id;
    sim.sell(hero, SHOP_NPC, charm);
    assert_eq!(
        sim.player(hero).unwrap().wallet["Gems"],
        3,
        "a third back, in gems"
    );
}

#[test]
fn older_saves_name_items_by_type() {
    let old = r#"(name: "Old", cell: (3, 3), level: 2, exp: 0, money: 5, life: 0, mana: 0,
        bag: [100, 300], equipment: [200])"#;
    let save: PlayerSave = ron::from_str(old).expect("an old save reads");
    assert_eq!(save.bag, [SavedItem::from(POTION), SavedItem::from(ARROW)]);
    let mut sim = Sim::new(Grid::new(20, 20), vec![], vec![], rules(), 11);
    let hero = sim.add_saved_player(&save);
    let armor = sim.player(hero).unwrap().equipment[0];
    assert_eq!(armor.durability, 3, "a whole one");
    let again: PlayerSave =
        ron::from_str(&ron::to_string(&sim.save_player(hero).unwrap()).unwrap()).unwrap();
    assert_eq!(again.equipment[0].durability, Some(3));
}
