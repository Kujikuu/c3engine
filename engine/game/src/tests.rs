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

const FIRE: u32 = 10;
const MEND: u32 = 11;
const QUAKE: u32 = 12;
const SUPERMAN: u32 = 13;
const RAGE: u32 = 20;
const POISON: u32 = 21;
const STUN: u32 = 22;

struct TestRules {
    monsters: Vec<MonsterType>,
    potion: ItemType,
    armor: ItemType,
    others: Vec<ItemType>,
    skills: Vec<SkillType>,
    statuses: Vec<StatusType>,
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
        ..MonsterType::default()
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
    fn skill(&self, id: u32) -> Option<&SkillType> {
        self.skills.iter().find(|s| s.id == id)
    }
    fn status(&self, id: u32) -> Option<&StatusType> {
        self.statuses.iter().find(|s| s.id == id)
    }
    fn player_stats(&self, who: &crate::StatsFor) -> Stats {
        let level = who.level;
        Stats {
            max_life: 100 + 10 * level,
            max_mana: 100,
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
    fn hit_exp(&self, _attacker: &Fighter, _target: &Fighter, damage: u32) -> u64 {
        damage.into()
    }
    fn kill_exp(&self, _attacker: &Fighter, _target: &Fighter) -> u64 {
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
    Box::new(test_rules())
}

fn test_rules() -> TestRules {
    TestRules {
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
        statuses: vec![
            StatusType {
                id: RAGE,
                name: "Rage".into(),
                duration_ms: 1000,
                attack_percent: 50,
                ..StatusType::default()
            },
            StatusType {
                id: POISON,
                name: "Poison".into(),
                duration_ms: 3000,
                life_per_tick: -5,
                tick_ms: 1000,
                ..StatusType::default()
            },
            StatusType {
                id: STUN,
                name: "Stun".into(),
                duration_ms: 1000,
                stun: true,
                ..StatusType::default()
            },
        ],
        skills: vec![
            SkillType {
                id: FIRE,
                name: "Fire".into(),
                range: 5,
                cooldown_ms: 1000,
                mana: 10,
                power: 20,
                ..SkillType::default()
            },
            SkillType {
                id: MEND,
                name: "Mend".into(),
                target: SkillTarget::Myself,
                kind: SkillKind::Heal,
                mana: 5,
                power: 30,
                ..SkillType::default()
            },
            SkillType {
                id: SUPERMAN,
                name: "Superman".into(),
                target: SkillTarget::Myself,
                kind: SkillKind::Heal,
                cost_attribute: "xp".into(),
                cost: 100,
                ..SkillType::default()
            },
            SkillType {
                id: QUAKE,
                name: "Quake".into(),
                target: SkillTarget::Ground,
                range: 6,
                area: 2,
                power: 5,
                ..SkillType::default()
            },
        ],
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
    }
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

    sim.answer(hero, 1, None);
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
    sim.answer(hero, 0, None);
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

#[test]
fn skills_may_cost_an_attribute() {
    let mut sim = Sim::new(Grid::new(20, 20), vec![], vec![], rules(), 3);
    let hero = sim.add_player("Hero", (5, 5), 5);
    assert!(sim.learn_skill(hero, SUPERMAN));
    sim.cast(hero, SUPERMAN, CastAt::Entity(hero));
    sim.advance(50);
    let refused = sim
        .take_events()
        .iter()
        .any(|e| matches!(e, Event::Message { text, .. } if text == "Not enough xp for Superman."));
    assert!(refused);
    let p = sim.players.get_mut(&hero).unwrap();
    p.attributes.insert("xp".into(), 100);
    sim.cast(hero, SUPERMAN, CastAt::Entity(hero));
    sim.advance(50);
    assert_eq!(
        sim.player(hero).unwrap().attributes["xp"],
        0,
        "the full bar spent"
    );
}

#[test]
fn skills_cost_mana_rest_and_reach_their_area() {
    let mut sim = Sim::new(
        Grid::new(40, 40),
        vec![],
        vec![
            spawn(1, SLIME, (9, 5), (9, 5), 1),
            spawn(2, SLIME, (20, 20), (20, 20), 1),
            spawn(3, SLIME, (21, 21), (21, 21), 1),
        ],
        rules(),
        12,
    );
    let mut settings = sim.settings().clone();
    settings.monster_wander = 0;
    sim.set_settings(settings);
    // At the top level, so no level up refills its mana.
    let hero = sim.add_player("Hero", (5, 5), 5);
    let near = sim.monsters().find(|m| m.home == (9, 5)).unwrap().id;
    sim.cast(hero, FIRE, CastAt::Entity(near));
    let said = |sim: &mut Sim, text: &str| {
        sim.take_events()
            .iter()
            .any(|e| matches!(e, Event::Message { text: t, .. } if t == text))
    };
    assert!(said(&mut sim, "You don't know Fire."));
    assert!(sim.learn_skill(hero, FIRE));
    assert!(!sim.learn_skill(hero, 999), "no such skill");
    // In range: cast at once; 20 power + half of 10 attack = 25 of the slime's 30.
    sim.cast(hero, FIRE, CastAt::Entity(near));
    sim.advance(50);
    assert_eq!(sim.monster(near).unwrap().life, 5);
    assert_eq!(sim.player(hero).unwrap().mana, 90);
    let events = sim.take_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Cast { skill: FIRE, targets, .. } if targets == &[near]))
    );
    // Resting: not again until its cooldown is over.
    sim.cast(hero, FIRE, CastAt::Entity(near));
    sim.advance(50);
    assert!(said(&mut sim, "Fire isn't ready yet."));
    sim.advance(1000);
    sim.cast(hero, FIRE, CastAt::Entity(near));
    sim.advance(50);
    assert!(
        !sim.monster(near).unwrap().is_alive(),
        "the second bolt kills"
    );
    // An area hits all within it; out of range, the caster walks up first.
    sim.learn_skill(hero, QUAKE);
    sim.cast(hero, QUAKE, CastAt::Cell((20, 21)));
    assert!(run_until(&mut sim, 20_000, |s| {
        s.monsters().filter(|m| m.home.0 >= 20).all(|m| m.life < 30)
    }));
    let p = sim.player(hero).unwrap();
    assert!(distance(p.cell, (20, 21)) <= 6, "walked into range");
    // A heal on oneself, whatever was aimed at.
    sim.learn_skill(hero, MEND);
    sim.players.get_mut(&hero).unwrap().life = 50;
    sim.cast(hero, MEND, CastAt::Cell((0, 0)));
    sim.advance(50);
    assert_eq!(sim.player(hero).unwrap().life, 80, "50 + 30");
    // Known skills are kept.
    assert_eq!(sim.save_player(hero).unwrap().skills, [FIRE, QUAKE, MEND]);
}

#[test]
fn statuses_change_stats_tick_life_stun_and_wear_off() {
    let mut sim = Sim::new(
        Grid::new(40, 40),
        vec![],
        vec![spawn(1, SLIME, (20, 20), (20, 20), 1)],
        rules(),
        12,
    );
    let mut settings = sim.settings().clone();
    settings.monster_wander = 0;
    sim.set_settings(settings);
    let hero = sim.add_player("Hero", (5, 5), 1);
    assert!(sim.add_status(hero, RAGE, None));
    assert!(!sim.add_status(hero, 999, None), "no such status");
    assert_eq!(sim.player(hero).unwrap().stats.attack_min, 15, "+50% of 10");
    sim.advance(1050);
    assert_eq!(sim.player(hero).unwrap().stats.attack_min, 10, "worn off");
    assert!(sim.take_events().iter().any(|e| matches!(
        e,
        Event::StatusEnded { target, status: RAGE } if *target == hero
    )));
    // Poison: 5 life a second for 3 seconds, the kill the poisoner's.
    let slime = sim.monsters().next().unwrap().id;
    let life = sim.monster(slime).unwrap().life;
    sim.add_status(slime, POISON, Some(hero));
    sim.advance(3100);
    assert_eq!(sim.monster(slime).unwrap().life, life - 15);
    assert!(sim.monster(slime).unwrap().statuses.is_empty());
    // Stunned, the hero stands still.
    sim.add_status(hero, STUN, None);
    sim.walk(hero, (9, 5));
    sim.advance(900);
    assert_eq!(sim.player(hero).unwrap().cell, (5, 5));
    sim.advance(3000);
    assert_eq!(sim.player(hero).unwrap().cell, (9, 5));
}

#[test]
fn players_fight_players_only_on_pk_maps() {
    let mut sim = Sim::new(Grid::new(20, 20), vec![], vec![], rules(), 3);
    let a = sim.add_player("A", (5, 5), 1);
    let b = sim.add_player("B", (6, 5), 1);
    sim.attack(a, b);
    sim.advance(2000);
    let life = sim.player(b).unwrap().stats.max_life;
    assert_eq!(sim.player(b).unwrap().life, life, "no PK here");
    sim.set_pk(true);
    sim.attack(a, b);
    sim.advance(60_000);
    assert_eq!(sim.player(a).unwrap().pk_kills, 1);
    assert!(
        sim.save_player(a).unwrap().pk_kills == 1,
        "kept in the save"
    );
}

#[test]
fn npcs_move_within_the_map_and_leave_for_others() {
    let guide = Npc {
        id: 7,
        name: "Guide".into(),
        cell: (3, 3),
        direction: 0,
        look: 1,
        kind: 0,
        script: String::new(),
        extra: Default::default(),
    };
    let mut sim = Sim::new(Grid::new(20, 20), vec![guide], vec![], rules(), 3);
    sim.set_map_ids(vec![1002]);
    let viewer = sim.add_player("A", (5, 5), 1);
    let mut mirror = Sim::new(Grid::new(20, 20), sim.npcs().to_vec(), vec![], rules(), 3);
    let mut seen = Viewer::default();
    sim.move_npc(7, 1002, (8, 8));
    let events = sim.take_events();
    mirror.apply_update(sim.update_for(viewer, &mut seen, &events));
    assert_eq!(mirror.npcs()[0].cell, (8, 8), "the mirror follows");
    sim.move_npc(7, 1011, (4, 4));
    let events = sim.take_events();
    mirror.apply_update(sim.update_for(viewer, &mut seen, &events));
    assert!(sim.npcs().is_empty() && mirror.npcs().is_empty());
    let moves = sim.take_npc_moves();
    assert_eq!((moves[0].0, moves[0].1.cell), (1011, (4, 4)));
    let mut other = Sim::new(Grid::new(20, 20), vec![], vec![], rules(), 3);
    other.arrive_npc(moves[0].1.clone());
    assert_eq!(other.npcs()[0].name, "Guide");
}

#[test]
fn the_dead_walk_as_ghosts_and_come_back_when_they_ask() {
    let mut sim = Sim::new(Grid::new(20, 20), vec![], vec![], rules(), 3);
    let mut settings = sim.settings().clone();
    settings.revive_by_request = true;
    settings.ghosts_walk = true;
    settings.revive_seconds = 20.0;
    sim.set_settings(settings);
    sim.set_pk(true);
    sim.set_revive_point(Some(c3_scene::PortalTarget {
        map: 0,
        cell: (2, 2),
    }));
    sim.set_map_ids(vec![0]);
    let a = sim.add_player("A", (5, 5), 1);
    let b = sim.add_player("B", (6, 5), 1);
    sim.attack(a, b);
    sim.advance(30_000);
    assert!(!sim.player(b).unwrap().is_alive());
    sim.input(b, Input::Ghost);
    sim.input(b, Input::Walk((6, 9)));
    sim.advance(10_000);
    let ghost = sim.player(b).unwrap();
    assert!(
        ghost.ghost && !ghost.is_alive() && ghost.cell == (6, 9),
        "{:?}",
        ghost.cell
    );
    sim.input(b, Input::Revive);
    sim.advance(1_000);
    let back = sim.player(b).unwrap();
    assert!(back.is_alive() && !back.ghost);
    assert_eq!(back.cell, (2, 2), "at the map's revive point");
}

#[test]
fn killing_players_of_good_name_brings_crime_and_pk_points() {
    let mut sim = Sim::new(Grid::new(20, 20), vec![], vec![], rules(), 3);
    sim.set_pk(true);
    let a = sim.add_player("A", (5, 5), 1);
    let b = sim.add_player("B", (6, 5), 1);
    sim.attack(a, b);
    sim.advance(60_000);
    let killer = sim.player(a).unwrap();
    assert_eq!((killer.pk, killer.mark), (10, crate::NameMark::Criminal));
    // Two more kills: a red name.
    for _ in 0..2 {
        sim.attack(a, b);
        sim.advance(60_000);
    }
    assert_eq!(sim.player(a).unwrap().mark, crate::NameMark::Red);
    // Killing a red name is fair; points wear off.
    sim.attack(b, a);
    sim.advance(120_000);
    assert_eq!(
        sim.player(b).unwrap().pk,
        0,
        "no points for killing a red name"
    );
    sim.advance(360_000 * 2);
    assert!(sim.player(a).unwrap().pk < 30);
}

#[test]
fn monsters_assist_flee_cast_and_bosses_hold_on() {
    let mut r = test_rules();
    let pack = MonsterType {
        assist: true,
        flee_percent: 80,
        ..monster(SLIME, 100, 0, false)
    };
    r.monsters = vec![pack];
    let mut sim = Sim::new(
        Grid::new(40, 40),
        vec![],
        vec![Spawn {
            per_wave: 3,
            ..spawn(1, SLIME, (10, 5), (12, 5), 3)
        }],
        Box::new(r),
        5,
    );
    let mut settings = sim.settings().clone();
    settings.monster_wander = 0;
    sim.set_settings(settings);
    sim.advance(50);
    let hero = sim.add_player("Hero", (5, 5), 1);
    let first = sim.monsters().next().unwrap().id;
    sim.learn_skill(hero, FIRE);
    let before = distance(sim.monster(first).unwrap().cell, (5, 5));
    sim.cast(hero, FIRE, CastAt::Entity(first));
    sim.advance(1500);
    assert_eq!(sim.monsters().count(), 3);
    // Hit, it calls its kind: every slime is after the hero now.
    assert!(sim.monsters().all(|m| m.target == Some(hero)));
    // Down to 75 of 100 life, under its 80%: it runs.
    sim.advance(3000);
    let hit = sim.monster(first).unwrap();
    assert_eq!(hit.life, 75);
    assert!(distance(hit.cell, (5, 5)) > before, "it ran");
}

#[test]
fn monster_skills_and_bosses() {
    let mut r = test_rules();
    r.monsters = vec![MonsterType {
        boss: true,
        skill: FIRE,
        skill_percent: 100,
        ..monster(WOLF, 500, 5, true)
    }];
    let mut sim = Sim::new(
        Grid::new(40, 40),
        vec![],
        vec![spawn(1, WOLF, (6, 5), (6, 5), 1)],
        Box::new(r),
        5,
    );
    sim.advance(50);
    let hero = sim.add_player("Hero", (5, 5), 1);
    let wolf = sim.monsters().next().unwrap().id;
    assert!(sim.add_status(wolf, STUN, None), "it bears it");
    sim.advance(1100);
    let events = sim.take_events();
    // Stuns don't hold a boss; its skill replaces its hits.
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Cast { caster, skill: FIRE, .. } if *caster == wolf
    )));
    assert!(sim.player(hero).unwrap().life < sim.player(hero).unwrap().stats.max_life);
}

const EVENTS: &str = r#"
fn on_login() {
    message("Welcome back, " + player_name() + "!");
    start_timer("bonus", 2);
}

fn on_enter(map) {
    set_flag("map", map);
}

fn on_kill(monster) {
    set_flag("kills", flag("kills") + 1);
    set_flag("last", monster);
}

fn on_use(item) {
    set_flag("used", item);
}

fn on_timer(name) {
    if name == "bonus" { give_money(7); }
}
"#;

#[test]
fn the_events_script_hears_login_maps_kills_uses_and_timers() {
    let dir = std::env::temp_dir().join(format!("c3-game-events-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("events.rhai"), EVENTS).unwrap();
    let mut sim = Sim::new(
        Grid::new(30, 30),
        vec![],
        vec![spawn(1, SLIME, (6, 5), (6, 5), 1)],
        rules(),
        7,
    );
    sim.set_map_ids(vec![4]);
    sim.set_scripts(c3_script::ScriptHost::new(&dir, None), vec![4]);
    sim.advance(50);
    let hero = sim.add_saved_player(&crate::PlayerSave::new("Hero", (5, 5)));
    sim.logged_in(hero);
    assert_eq!(sim.player(hero).unwrap().flags["map"], 4, "on_enter");
    assert!(
        sim.take_events()
            .iter()
            .any(|e| matches!(e, Event::Notice { text, .. } if text == "Welcome back, Hero!"))
    );
    let slime = sim.monsters().next().unwrap().id;
    sim.attack(hero, slime);
    sim.advance(10_000);
    let p = sim.player(hero).unwrap();
    assert_eq!((p.flags["kills"], p.flags["last"]), (1, i64::from(SLIME)));
    assert_eq!(p.money, 7, "the timer ran out");
    sim.give_items(hero, POTION, 1);
    let item = sim.player(hero).unwrap().bag[0].id;
    sim.use_item(hero, item);
    assert_eq!(sim.player(hero).unwrap().flags["used"], i64::from(POTION));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn safe_and_no_pk_regions_stop_fights_and_scripts_hear_regions() {
    use c3_scene::Region;
    let dir = std::env::temp_dir().join(format!("c3-game-regions-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("events.rhai"),
        "fn on_enter_region(id) { set_flag(\"in\", id); }\n\
         fn on_leave_region(id) { set_flag(\"out\", id); }\n",
    )
    .unwrap();
    let mut sim = Sim::new(
        Grid::new(40, 40),
        vec![],
        vec![spawn(1, WOLF, (6, 6), (6, 6), 1)],
        rules(),
        3,
    );
    sim.set_scripts(c3_script::ScriptHost::new(&dir, None), vec![]);
    sim.set_regions(vec![
        Region {
            id: 1,
            min: (0, 0),
            max: (9, 9),
            safe: true,
            ..Region::default()
        },
        Region {
            id: 2,
            min: (20, 20),
            max: (29, 29),
            no_pk: true,
            ..Region::default()
        },
    ]);
    sim.set_pk(true);
    // A wolf right next to a player in a safe zone leaves it be.
    let hero = sim.add_player("Hero", (5, 5), 1);
    sim.advance(10_000);
    let p = sim.player(hero).unwrap();
    assert_eq!(p.life, p.stats.max_life, "safe from the wolf");
    assert_eq!(p.flags["in"], 1, "on_enter_region");
    // Two players in the no-PK zone can't fight.
    let a = sim.add_player("A", (25, 25), 1);
    let b = sim.add_player("B", (26, 25), 1);
    sim.attack(a, b);
    sim.advance(3000);
    let life = sim.player(b).unwrap().stats.max_life;
    assert_eq!(sim.player(b).unwrap().life, life, "no PK here");
    // Walking out tells the script.
    sim.walk(hero, (15, 15));
    sim.advance(10_000);
    assert_eq!(sim.player(hero).unwrap().flags["out"], 1, "on_leave_region");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn blueprints_hear_events_beside_the_events_script() {
    use c3_script::blueprint::{Blueprint, Graph, GraphNode, Literal, PinRef, Wire};
    let dir = std::env::temp_dir().join(format!("c3-game-blueprints-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("events.rhai"),
        "fn on_login() { give_money(1); }\n",
    )
    .unwrap();
    // On login: give 20, wait a second, give 300.
    let node = |id: u32, def: &str, amount: Option<f64>| GraphNode {
        id,
        def: def.into(),
        pos: [0.0, 0.0],
        values: amount
            .map(|a| {
                (
                    if def == "flow.wait" {
                        "seconds"
                    } else {
                        "amount"
                    },
                    a,
                )
            })
            .into_iter()
            .map(|(k, a)| (k.to_owned(), Literal::Number(a)))
            .collect(),
    };
    let wire = |from: u32, to: u32| Wire {
        from: PinRef {
            node: from,
            pin: "then".into(),
        },
        to: PinRef {
            node: to,
            pin: "exec".into(),
        },
    };
    let blueprint = Blueprint {
        events: Graph {
            nodes: vec![
                node(1, "event.login", None),
                node(2, "player.give_money", Some(20.0)),
                node(3, "flow.wait", Some(1.0)),
                node(4, "player.give_money", Some(300.0)),
            ],
            wires: vec![wire(1, 2), wire(2, 3), wire(3, 4)],
        },
        ..Blueprint::default()
    };
    let code = c3_script::blueprint::compile("gifts", &blueprint).unwrap();
    std::fs::create_dir_all(dir.join("graphs")).unwrap();
    std::fs::write(dir.join("graphs").join("gifts.rhai"), code).unwrap();
    let mut sim = Sim::new(Grid::new(20, 20), vec![], vec![], rules(), 5);
    sim.set_scripts(c3_script::ScriptHost::new(&dir, None), vec![]);
    let hero = sim.add_saved_player(&crate::PlayerSave::new("Hero", (5, 5)));
    sim.logged_in(hero);
    assert_eq!(
        sim.player(hero).unwrap().money,
        21,
        "both scripts heard the login"
    );
    sim.advance(1_500);
    assert_eq!(
        sim.player(hero).unwrap().money,
        321,
        "the wait went on from its timer"
    );
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn scripts_heal_share_world_variables_spawn_open_windows_and_walk() {
    let dir = std::env::temp_dir().join(format!("c3-game-more-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("events.rhai"),
        "fn on_login() {\n\
             hurt(1000); heal(5);\n\
             set_world_flag(\"logins\", world_flag(\"logins\") + 1);\n\
             spawn_monster(1, 10, 10);\n\
             open_window(153); play_effect(\"glow\"); play_sound(\"bell.wav\");\n\
             walk_to(8, 5, \"there\");\n\
         }\n\
         fn on_arrive(tag) { set_flag(\"arrived\", if tag == \"there\" { player_x() } else { -1 }); }\n\
         fn on_level_up(level) { set_flag(\"level\", level); }\n",
    )
    .unwrap();
    let mut sim = Sim::new(Grid::new(30, 30), vec![], vec![], rules(), 9);
    sim.set_scripts(c3_script::ScriptHost::new(&dir, None), vec![]);
    let monsters = sim.monsters().count();
    let hero = sim.add_saved_player(&crate::PlayerSave::new("Hero", (5, 5)));
    sim.logged_in(hero);
    let p = sim.player(hero).unwrap();
    assert_eq!(p.life, 6, "hurt never kills: 1 left, then 5 back");
    assert_eq!(sim.world_flags["logins"], 1);
    assert_eq!(sim.monsters().count(), monsters + 1);
    let events = sim.take_events();
    assert!(events.contains(&Event::OpenWindow {
        player: hero,
        dialog: 153
    }));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Effect { effect, .. } if effect == "glow"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Sound { sound, .. } if sound == "bell.wav"))
    );
    sim.advance(5_000);
    assert_eq!(
        sim.player(hero).unwrap().flags["arrived"],
        8,
        "on_arrive heard the walk end"
    );
    sim.gain_exp(hero, 1_000_000);
    assert!(sim.player(hero).unwrap().flags["level"] > 1, "on_level_up");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn teams_form_by_chat_share_kill_experience_and_scripts_see_them() {
    let dir = std::env::temp_dir().join(format!("c3-game-teams-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("events.rhai"),
        "fn on_kill(m) { set_flag(\"team\", team_size()); team_message(\"Got one!\"); }\n",
    )
    .unwrap();
    let mut sim = Sim::new(
        Grid::new(30, 30),
        vec![],
        vec![spawn(1, SLIME, (6, 5), (6, 5), 1)],
        rules(),
        3,
    );
    sim.set_scripts(c3_script::ScriptHost::new(&dir, None), vec![]);
    let ann = sim.add_saved_player(&crate::PlayerSave::new("Ann", (5, 5)));
    let bob = sim.add_saved_player(&crate::PlayerSave::new("Bob", (5, 6)));
    let say = |sim: &mut Sim, who, line: &str| {
        sim.input(who, Input::Say(line.into()));
        sim.take_events()
            .into_iter()
            .find_map(|e| match e {
                Event::CommandReply { text, .. } => Some(text),
                _ => None,
            })
            .unwrap_or_default()
    };
    assert!(say(&mut sim, bob, "/team join Ann").contains("haven't invited"));
    say(&mut sim, ann, "/team create");
    say(&mut sim, ann, "/team invite Bob");
    assert_eq!(say(&mut sim, bob, "/team join Ann"), "You joined the team.");
    assert_eq!(sim.team_members(bob), [ann, bob]);
    assert!(say(&mut sim, bob, "/team").contains("Ann, Bob"));
    let slime = sim.monsters().next().unwrap().id;
    sim.attack(ann, slime);
    sim.advance(10_000);
    assert!(sim.player(bob).unwrap().exp > 0, "Bob shares the kill");
    assert_eq!(sim.player(ann).unwrap().flags["team"], 2);
    say(&mut sim, ann, "/team leave");
    assert_eq!(
        sim.player(bob).unwrap().team,
        None,
        "the leader leaving ends the team"
    );
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn teams_reach_players_on_other_maps() {
    let mut maps = [
        Sim::new(Grid::new(20, 20), vec![], vec![], rules(), 1),
        Sim::new(Grid::new(20, 20), vec![], vec![], rules(), 2),
    ];
    let (ann, bob) = (EntityId(100), EntityId(200));
    maps[0].add_saved_player_as(&crate::PlayerSave::new("Ann", (5, 5)), ann);
    maps[1].add_saved_player_as(&crate::PlayerSave::new("Bob", (5, 5)), bob);
    // What the server does around each tick: share rosters, carry team ops.
    let relay = |maps: &mut [Sim; 2]| {
        let rosters = [maps[0].mates(), maps[1].mates()];
        maps[0].set_elsewhere(rosters[1].clone());
        maps[1].set_elsewhere(rosters[0].clone());
    };
    let carry = |maps: &mut [Sim; 2]| {
        for from in 0..2 {
            for (player, op) in maps[from].take_team_ops() {
                maps[1 - from].apply_team_op(player, op);
            }
        }
    };
    let say = |maps: &mut [Sim; 2], map: usize, who, line: &str| {
        relay(maps);
        maps[map].input(who, Input::Say(line.into()));
        carry(maps);
        let events: Vec<Event> = maps.iter_mut().flat_map(|m| m.take_events()).collect();
        events
            .into_iter()
            .find_map(|e| match e {
                Event::CommandReply { text, .. } => Some(text),
                _ => None,
            })
            .unwrap_or_default()
    };
    say(&mut maps, 0, ann, "/team create");
    assert_eq!(
        say(&mut maps, 0, ann, "/team invite Bob"),
        "You invited Bob."
    );
    assert_eq!(
        say(&mut maps, 1, bob, "/team join Ann"),
        "You joined the team."
    );
    relay(&mut maps);
    assert_eq!(maps[0].team_members(ann), [ann, bob]);
    let names: Vec<String> = maps[0]
        .team_mates(ann)
        .into_iter()
        .map(|m| m.name)
        .collect();
    assert_eq!(
        names,
        ["Ann", "Bob"],
        "Ann's team list shows Bob on the other map"
    );
    say(&mut maps, 0, ann, "/team kick Bob");
    assert_eq!(
        maps[1].player(bob).unwrap().team,
        None,
        "kicked across maps"
    );
}

#[test]
fn an_event_without_a_page_leaves_the_conversation_open() {
    let dir = std::env::temp_dir().join(format!("c3-game-talk-event-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("npc")).unwrap();
    std::fs::write(
        dir.join("events.rhai"),
        "fn on_timer(name) { give_money(1); }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("npc/talker.rhai"),
        "fn talk() { text(\"Hello.\"); option(\"Bye\"); }\n",
    )
    .unwrap();
    let npc = Npc {
        id: 7,
        name: "Talker".into(),
        cell: (6, 5),
        script: "npc/talker".into(),
        ..shopkeeper()
    };
    let mut sim = Sim::new(Grid::new(30, 30), vec![npc], vec![], rules(), 9);
    sim.set_scripts(c3_script::ScriptHost::new(&dir, None), vec![]);
    let hero = sim.add_saved_player(&crate::PlayerSave::new("Hero", (5, 5)));
    sim.input(hero, Input::Talk(7));
    sim.advance(100);
    assert!(sim.player(hero).unwrap().dialogue.is_some());
    sim.start_timer(hero, "gift".into(), 0.0);
    sim.advance(100);
    assert_eq!(sim.player(hero).unwrap().money, 1);
    assert!(
        sim.player(hero).unwrap().dialogue.is_some(),
        "still talking"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn guilds_and_world_variables_reach_every_map() {
    let dir = std::env::temp_dir().join(format!("c3-game-guilds-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("events.rhai"),
        "fn on_login() {\n\
             if create_guild(\"Dragons\", 5000) { set_world_flag(\"guilds\", 1); }\n\
             add_guild_fund(guild_id(), 250);\n\
         }\n",
    )
    .unwrap();
    let mut here = Sim::new(Grid::new(30, 30), vec![], vec![], rules(), 9);
    here.set_scripts(c3_script::ScriptHost::new(&dir, None), vec![]);
    let hero = here.add_saved_player(&crate::PlayerSave::new("Hero", (5, 5)));
    here.logged_in(hero);
    // The host takes this map's changes and hands the result to another map.
    let mut host = crate::WorldState::default();
    for change in here.take_world_changes() {
        host.apply(&change);
    }
    let mut there = Sim::new(Grid::new(30, 30), vec![], vec![], rules(), 9);
    there.set_world_state(host.clone());
    let (id, rank) = there.world_state().guild_of("Hero").expect("a member");
    assert_eq!(rank, 100, "its leader");
    let guild = &there.world_state().guilds[&id];
    assert_eq!((guild.name.as_str(), guild.fund), ("Dragons", 5250));
    assert_eq!(there.world_state().flags["guilds"], 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_transformation_lasts_its_time() {
    let dir = std::env::temp_dir().join(format!("c3-game-transform-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("events.rhai"),
        "fn on_login() { transform(1, 2, 0); }\n",
    )
    .unwrap();
    let mut sim = Sim::new(Grid::new(30, 30), vec![], vec![], rules(), 9);
    sim.set_scripts(c3_script::ScriptHost::new(&dir, None), vec![]);
    let hero = sim.add_saved_player(&crate::PlayerSave::new("Hero", (5, 5)));
    sim.logged_in(hero);
    let p = sim.player(hero).unwrap();
    assert_eq!(p.transform.map(|(m, _)| m), Some(1));
    let monster = sim.rules().monster(1).unwrap().stats;
    assert_eq!(
        p.stats.attack_max, monster.attack_max,
        "the monster's numbers"
    );
    sim.advance(2_100);
    assert_eq!(
        sim.player(hero).unwrap().transform,
        None,
        "two seconds later"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn world_events_run_every_minute_and_by_timer() {
    let dir = std::env::temp_dir().join(format!("c3-game-world-ev-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("events.rhai"),
        "fn on_minute() {\n\
             set_world_flag(\"minutes\", world_flag(\"minutes\") + 1);\n\
             world_timer(\"later\", 5);\n\
         }\n\
         fn later() { set_world_flag(\"later\", 1); }\n",
    )
    .unwrap();
    let mut sim = Sim::new(Grid::new(30, 30), vec![], vec![], rules(), 9);
    sim.set_scripts(c3_script::ScriptHost::new(&dir, None), vec![]);
    sim.advance(61_000);
    assert_eq!(
        sim.world_state().flags.get("minutes"),
        Some(&1),
        "with no player"
    );
    sim.advance(5_000);
    assert_eq!(sim.world_state().flags.get("later"), Some(&1));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stepping_on_a_trap_sets_it_off() {
    let dir = std::env::temp_dir().join(format!("c3-game-trap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("events.rhai"),
        "fn trap_7() { set_flag(\"stepped\", flag(\"stepped\") + 1); erase_trap(); }\n",
    )
    .unwrap();
    let mut sim = Sim::new(Grid::new(30, 30), vec![], vec![], rules(), 9);
    sim.set_scripts(c3_script::ScriptHost::new(&dir, None), vec![]);
    sim.set_traps(vec![c3_scene::Trap {
        id: 1,
        kind: 7,
        cell: (8, 5),
        look: 3,
    }]);
    let hero = sim.add_saved_player(&crate::PlayerSave::new("Hero", (5, 5)));
    sim.input(hero, Input::Walk((10, 5)));
    assert!(run_until(&mut sim, 10_000, |s| s
        .player(hero)
        .unwrap()
        .cell
        == (10, 5)));
    assert_eq!(sim.player(hero).unwrap().flags["stepped"], 1);
    assert!(sim.traps().is_empty(), "erased by its event");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn storage_keeps_items_and_money_across_saves() {
    let keeper = Npc {
        id: 8,
        name: "Warehouseman".into(),
        cell: (6, 5),
        ..shopkeeper()
    };
    let mut sim = Sim::new(Grid::new(30, 30), vec![keeper], vec![], rules(), 9);
    let mut save = crate::PlayerSave::new("Hero", (5, 5));
    save.money = 500;
    save.bag = vec![ARMOR.into()];
    let hero = sim.add_saved_player(&save);
    let sword = sim.player(hero).unwrap().bag[0].id;
    sim.store(hero, 8, sword);
    sim.move_money(hero, 8, 200, true);
    let p = sim.player(hero).unwrap();
    assert!(p.bag.is_empty() && p.storage[&8].len() == 1);
    assert_eq!((p.money, p.saved_money), (300, 200));
    // Kept in the save, and back.
    let kept = sim.save_player(hero).unwrap();
    let mut other = Sim::new(Grid::new(30, 30), vec![], vec![], rules(), 9);
    let again = other.add_saved_player(&kept);
    let p = other.player(again).unwrap();
    assert_eq!((p.storage[&8].len(), p.saved_money), (1, 200));
    let sword = sim.player(hero).unwrap().storage[&8][0].id;
    sim.retrieve(hero, 8, sword);
    assert_eq!(sim.player(hero).unwrap().bag.len(), 1);
}

#[test]
fn two_players_trade_an_item_for_money() {
    let mut sim = Sim::new(Grid::new(30, 30), vec![], vec![], rules(), 9);
    let mut seller = crate::PlayerSave::new("Seller", (5, 5));
    seller.bag = vec![ARMOR.into()];
    let mut buyer = crate::PlayerSave::new("Buyer", (7, 5));
    buyer.money = 300;
    let (s, b) = (sim.add_saved_player(&seller), sim.add_saved_player(&buyer));
    sim.ask_trade(s, b);
    sim.ask_trade(b, s);
    let armor = sim.player(s).unwrap().bag[0].id;
    sim.offer_item(s, armor);
    sim.offer_money(b, 250);
    sim.accept_trade(s);
    assert!(
        sim.player(b).unwrap().bag.is_empty(),
        "not until both accept"
    );
    sim.accept_trade(b);
    let (sp, bp) = (sim.player(s).unwrap(), sim.player(b).unwrap());
    assert!(sp.bag.is_empty() && sp.money == 250);
    assert_eq!((bp.bag[0].kind, bp.money), (ARMOR, 50));
    let news: Vec<_> = (sim.take_events().into_iter())
        .filter_map(|e| match e {
            Event::Trade { player, news, .. } if player == s => Some(news),
            _ => None,
        })
        .collect();
    assert_eq!(news.first(), Some(&crate::TradeNews::Opened));
    assert_eq!(news.last(), Some(&crate::TradeNews::Done));
}

#[test]
fn dropped_items_and_money_come_back_as_they_were() {
    let mut sim = Sim::new(Grid::new(30, 30), vec![], vec![], rules(), 9);
    let mut save = crate::PlayerSave::new("Hero", (5, 5));
    save.money = 500;
    save.bag = vec![ARMOR.into()];
    let hero = sim.add_saved_player(&save);
    sim.players.get_mut(&hero).unwrap().bag[0].attrs[0] = 7;
    let armor = sim.player(hero).unwrap().bag[0];
    sim.drop_item(hero, armor.id);
    sim.input(hero, crate::Input::DropMoney(120));
    let p = sim.player(hero).unwrap();
    assert!(p.bag.is_empty());
    assert_eq!((p.money, sim.ground_items().count()), (380, 2));

    for item in sim.ground_items().map(|i| i.id).collect::<Vec<_>>() {
        sim.pick_up(hero, item);
        assert!(run_until(&mut sim, 10_000, |s| s
            .ground_item(item)
            .is_none()));
    }
    let p = sim.player(hero).unwrap();
    assert_eq!(p.money, 500);
    assert_eq!(p.bag, vec![armor], "the very item, its attributes kept");
}

#[test]
fn team_actions_invite_join_and_leave() {
    use crate::{Input, TeamAction};
    let mut sim = Sim::new(Grid::new(30, 30), vec![], vec![], rules(), 9);
    let lead = sim.add_player("Lead", (5, 5), 1);
    let mate = sim.add_player("Mate", (6, 5), 1);
    sim.input(lead, Input::Team(TeamAction::Create));
    sim.input(lead, Input::Team(TeamAction::Invite(mate)));
    assert!(sim.take_events().contains(&Event::TeamInvited {
        player: mate,
        leader: lead
    }));
    sim.input(mate, Input::Team(TeamAction::Join(lead)));
    let names: Vec<String> = sim.team_mates(mate).into_iter().map(|m| m.name).collect();
    assert_eq!(names, ["Lead", "Mate"]);
    sim.input(mate, Input::Team(TeamAction::Leave));
    assert_eq!(sim.team_members(lead), [lead]);
    sim.input(lead, Input::Team(TeamAction::Leave));
    assert!(sim.team_members(lead).is_empty(), "the team broke up");
}

#[test]
fn players_apply_to_open_teams() {
    use crate::{Input, TeamAction};
    let mut sim = Sim::new(Grid::new(30, 30), vec![], vec![], rules(), 9);
    let lead = sim.add_player("Lead", (5, 5), 1);
    let comer = sim.add_player("Comer", (6, 5), 1);
    sim.input(lead, Input::Team(TeamAction::Create));
    sim.input(lead, Input::Team(TeamAction::Close));
    sim.input(comer, Input::Team(TeamAction::Apply(lead)));
    sim.input(lead, Input::Team(TeamAction::Accept(comer)));
    assert_eq!(sim.team_members(lead), [lead], "a closed team takes no one");
    sim.input(lead, Input::Team(TeamAction::Open));
    sim.input(comer, Input::Team(TeamAction::Apply(lead)));
    assert!(sim.take_events().contains(&Event::TeamApplied {
        player: lead,
        applicant: comer
    }));
    sim.input(lead, Input::Team(TeamAction::Accept(comer)));
    assert_eq!(sim.team_members(lead), [lead, comer]);
}

#[test]
fn guild_leaders_invite_kick_and_members_give() {
    use crate::{GuildAction, Input};
    use c3_script::WorldChange;
    let mut sim = Sim::new(Grid::new(30, 30), vec![], vec![], rules(), 9);
    let lead = sim.add_player("Lead", (5, 5), 1);
    let comer = sim.add_player("Comer", (6, 5), 1);
    sim.set_money(comer, 500);
    sim.change_world(WorldChange::CreateGuild {
        id: 7,
        name: "Wolves".into(),
        leader: "Lead".into(),
        rank: 100,
        fund: 0,
        master: 0,
    });
    sim.input(comer, Input::Guild(GuildAction::Join(lead)));
    assert_eq!(sim.world_state().guild_of("Comer"), None, "not invited");
    sim.input(lead, Input::Guild(GuildAction::Invite(comer)));
    assert!(sim.take_events().contains(&Event::GuildInvited {
        player: comer,
        inviter: lead
    }));
    sim.input(comer, Input::Guild(GuildAction::Join(lead)));
    let member_rank = i64::from(sim.settings.guild_member_rank);
    assert_eq!(sim.world_state().guild_of("Comer"), Some((7, member_rank)));
    sim.input(comer, Input::Guild(GuildAction::Donate(200)));
    assert_eq!(sim.world_state().guilds[&7].fund, 200);
    assert_eq!(sim.player(comer).unwrap().money, 300);
    sim.input(lead, Input::Guild(GuildAction::Leave));
    assert!(
        sim.world_state().guild_of("Lead").is_some(),
        "a leader stays"
    );
    sim.input(lead, Input::Guild(GuildAction::Kick(comer)));
    assert_eq!(sim.world_state().guild_of("Comer"), None);
    assert_eq!(
        sim.take_world_changes().len(),
        4,
        "each change goes to the host"
    );
}

#[test]
fn the_wanted_list_is_shared_and_raised_and_removed() {
    use c3_script::WorldChange;
    let mut sim = Sim::new(Grid::new(10, 10), vec![], vec![], rules(), 9);
    let wanted = |id, bounty| WorldChange::Wanted {
        id,
        target: "Bad".into(),
        payer: "Hero".into(),
        bounty,
    };
    sim.change_world(wanted(1, 200_000));
    assert_eq!(sim.world_state().wanted[&1].bounty, 200_000);
    sim.change_world(wanted(1, 250_000));
    assert_eq!(sim.world_state().wanted_views()[&1].bounty, 250_000);
    sim.change_world(wanted(1, 0));
    assert!(
        sim.world_state().wanted.is_empty(),
        "a bounty of 0 removes it"
    );
    assert_eq!(
        sim.take_world_changes().len(),
        3,
        "each change goes to the host"
    );
}

#[test]
fn bounties_on_killers_are_shared() {
    use c3_script::{BountyView, WorldChange};
    let mut sim = Sim::new(Grid::new(10, 10), vec![], vec![], rules(), 9);
    let view = BountyView {
        target: "Red".into(),
        hunter: "Hero".into(),
        kind: 2,
        bonus: 500,
    };
    sim.change_world(WorldChange::Bounty {
        id: 1,
        view: Some(view.clone()),
    });
    assert_eq!(sim.world_state().bounty_views()[&1], view);
    sim.change_world(WorldChange::Bounty { id: 1, view: None });
    assert!(sim.world_state().bounties.is_empty());
}

#[test]
fn a_guild_handed_over_swaps_its_leaders_ranks() {
    use c3_script::WorldChange;
    let mut sim = Sim::new(Grid::new(10, 10), vec![], vec![], rules(), 9);
    sim.change_world(WorldChange::CreateGuild {
        id: 3,
        name: "Branch".into(),
        leader: "Old".into(),
        rank: 100,
        fund: 0,
        master: 1,
    });
    sim.change_world(WorldChange::Member {
        guild: 3,
        name: "New".into(),
        rank: 50,
    });
    sim.change_world(WorldChange::GuildLeader {
        guild: 3,
        name: "New".into(),
    });
    let g = &sim.world_state().guilds[&3];
    assert_eq!(g.leader, "New");
    assert_eq!((g.members["New"], g.members["Old"]), (100, 50));
}
