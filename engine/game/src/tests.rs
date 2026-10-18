use super::*;
use c3_scene::{Grid, Npc, Spawn};

const SLIME: u32 = 1;
const WOLF: u32 = 2;
const POTION: u32 = 100;
const SHOP_NPC: u32 = 7;
const ARMOR: u32 = 200;

struct TestRules {
    monsters: Vec<MonsterType>,
    potion: ItemType,
    armor: ItemType,
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
            _ => None,
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
        if npc == SHOP_NPC { &[POTION] } else { &[] }
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
        },
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
        sim.advance(TICK_MS);
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
    assert!(run_until(&mut sim, REVIVE_MS + 1000, |s| s
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
    assert!(
        sim.take_events()
            .iter()
            .any(|e| matches!(e, Event::Message { text, .. } if text.contains("money")))
    );

    let first = sim.player(hero).unwrap().bag[0].id;
    sim.sell(hero, SHOP_NPC, first);
    assert_eq!(sim.player(hero).unwrap().money, 20);

    // A potion heals up to the most life.
    sim.players.get_mut(&hero).unwrap().life = 10;
    let potion = sim.player(hero).unwrap().bag[0].id;
    sim.use_item(hero, potion);
    let p = sim.player(hero).unwrap();
    assert_eq!((p.life, p.bag.len()), (60, 0));

    for _ in 0..BAG_SIZE {
        assert!(sim.give_item(hero, POTION));
    }
    assert!(!sim.give_item(hero, POTION), "the bag is full");
    assert!(sim.take_items(hero, POTION, 3));
    assert_eq!(sim.player(hero).unwrap().bag.len(), BAG_SIZE - 3);

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
    sim.advance(TICK_MS);
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
