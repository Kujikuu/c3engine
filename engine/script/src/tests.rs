use super::*;
use c3_data::{Column, ColumnKind, Table, Value};
use std::path::PathBuf;

/// A fresh scripts folder (and tables folder) for one test.
fn project(test: &str, scripts: &[(&str, &str)]) -> (ScriptHost, PathBuf) {
    let root = std::env::temp_dir().join(format!("c3-script-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("scripts");
    for (name, source) in scripts {
        let file = dir.join(format!("{name}{SCRIPT_SUFFIX}"));
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, source).unwrap();
    }
    let tables = root.join("tables");
    std::fs::create_dir_all(&tables).unwrap();
    let mut items = Table::new(
        "items",
        vec![
            Column::new("id", ColumnKind::Int),
            Column::new("name", ColumnKind::Text),
        ],
    );
    items
        .rows
        .push(vec![Value::Int(7), Value::Text("Potion".into())]);
    c3_data::save_table(&items, &tables.join("items.table.ron")).unwrap();
    (ScriptHost::new(dir, Some(tables)), root)
}

fn npc() -> NpcInfo {
    NpcInfo {
        id: 10,
        name: "Shopkeeper".into(),
    }
}

const SHOP: &str = r#"
const PRICE = 100;
fn talk() {
    text("Hello " + player_name() + ", I am " + npc_name() + ".");
    option("Buy a " + lookup("items", 7, "name"), "buy");
    option("Bye");
}
fn buy() {
    if take_money(global::PRICE) {
        give_item(7);
        set_flag("bought", flag("bought") + 1);
        text("Thanks!");
    } else {
        text("Not enough money.");
    }
}
"#;

#[test]
fn dialogue_pages_answers_and_player_changes() {
    let (mut host, root) = project("shop", &[("npc/shop", SHOP)]);
    let mut player = PlayerState {
        name: "Hero".into(),
        money: 150,
        ..Default::default()
    };
    let reply = host.talk("npc/shop", &npc(), &mut player).unwrap();
    assert_eq!(reply.page.lines, ["Hello Hero, I am Shopkeeper."]);
    assert_eq!(reply.page.options[0].label, "Buy a Potion");
    assert_eq!(reply.page.options[0].next.as_deref(), Some("buy"));
    assert_eq!(reply.page.options[1].next, None);

    let reply = host.run("npc/shop", "buy", &npc(), &mut player).unwrap();
    assert_eq!(reply.page.lines, ["Thanks!"]);
    assert_eq!(
        reply.effects,
        [Effect::Money(-100), Effect::GaveItem { item: 7, count: 1 }]
    );
    assert_eq!((player.money, player.item_count(7)), (50, 1));
    assert_eq!(player.flags["bought"], 1);

    let reply = host.run("npc/shop", "buy", &npc(), &mut player).unwrap();
    assert_eq!(reply.page.lines, ["Not enough money."]);
    assert!(reply.effects.is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn errors_have_places_and_the_sandbox_holds() {
    let broken = "fn talk() {\n    text(\"a\"\n}\n";
    let failing = "fn talk() {\n    give_money(5);\n    take_item(1, 0);\n}\n";
    let endless = "fn talk() { loop { } }";
    let (mut host, root) = project(
        "errors",
        &[
            ("broken", broken),
            ("failing", failing),
            ("endless", endless),
        ],
    );
    let check = host.check(broken, Some(TALK));
    assert_eq!((check.len(), check[0].line), (1, 3));
    let warn = host.check("fn other() {}", Some(TALK));
    assert_eq!(warn[0].severity, Severity::Warning);

    let mut player = PlayerState::default();
    let e = host.talk("broken", &npc(), &mut player).unwrap_err();
    assert_eq!(e.diagnostic.line, 3);
    let e = host.talk("failing", &npc(), &mut player).unwrap_err();
    assert_eq!(e.diagnostic.line, 3, "{e}");
    assert!(e.diagnostic.message.contains("more than 0"), "{e}");
    assert_eq!(player.money, 5, "changes before the error stay");
    assert!(host.talk("endless", &npc(), &mut player).is_err());
    assert!(host.talk("../outside", &npc(), &mut player).is_err());
    assert!(host.run("failing", "missing", &npc(), &mut player).is_err());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn edited_scripts_reload_and_names_list() {
    let (mut host, root) = project("reload", &[("a", "fn talk() { text(\"one\"); }")]);
    let mut player = PlayerState::default();
    assert_eq!(
        host.talk("a", &npc(), &mut player).unwrap().page.lines,
        ["one"]
    );
    let file = host.script_file("a");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&file, "fn talk() { text(\"two\"); }").unwrap();
    assert_eq!(
        host.talk("a", &npc(), &mut player).unwrap().page.lines,
        ["two"]
    );
    std::fs::create_dir_all(root.join("scripts/npc")).unwrap();
    std::fs::write(root.join("scripts/npc/b.rhai"), "").unwrap();
    assert_eq!(script_names(&root.join("scripts")), ["a", "npc/b"]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn conversations_follow_answers() {
    let (mut host, root) = project("conversation", &[("npc/shop", SHOP)]);
    let mut player = PlayerState {
        money: 100,
        ..Default::default()
    };
    let (talk, _) = Conversation::start(&mut host, "npc/shop", npc(), &mut player).unwrap();
    let talk = talk.expect("talk() shows a page");
    let (after, effects) = talk.clone().choose(&mut host, 0, &mut player).unwrap();
    assert_eq!(after.unwrap().page.lines, ["Thanks!"]);
    assert_eq!(effects.len(), 2);
    let (closed, _) = talk.choose(&mut host, 1, &mut player).unwrap();
    assert!(closed.is_none(), "an answer without a function ends it");
    let _ = std::fs::remove_dir_all(root);
}

/// A kill quest and a collect quest in the project's quest table.
fn add_quests(root: &std::path::Path) {
    let mut table = Quest::new_table();
    let row = |id: i64, name: &str, goal: &str, target: i64, count: i64| {
        let mut r = table.empty_row(id);
        let set = |r: &mut Vec<Value>, col: &str, v: Value| r[table.column(col).unwrap()] = v;
        set(&mut r, "name", Value::Text(name.into()));
        set(&mut r, "min_level", Value::Int(2));
        set(&mut r, "goal", Value::Text(goal.into()));
        set(&mut r, "target", Value::Int(target));
        set(&mut r, "count", Value::Int(count));
        set(&mut r, "reward_money", Value::Int(500));
        set(&mut r, "reward_exp", Value::Int(80));
        set(&mut r, "reward_item", Value::Int(7));
        r
    };
    let rows = vec![
        row(1, "Pheasants", "kill", 3, 2),
        row(2, "Feathers", "collect", 9, 3),
    ];
    table.rows = rows;
    c3_data::save_table(&table, &root.join("tables/quests.table.ron")).unwrap();
}

/// Talks through `script`, picking the answers labelled `picks` in turn; returns the last
/// page's lines and every effect.
fn walk(
    host: &mut ScriptHost,
    script: &str,
    player: &mut PlayerState,
    picks: &[&str],
) -> (Vec<String>, Vec<Effect>) {
    let (mut talk, mut effects) = Conversation::start(host, script, npc(), player).unwrap();
    for pick in picks {
        let page = talk.expect("the dialogue goes on");
        let index = page
            .page
            .options
            .iter()
            .position(|o| o.label == *pick)
            .unwrap_or_else(|| panic!("no answer {pick:?} in {:?}", page.page));
        let (next, more) = page.choose(host, index, player).unwrap();
        talk = next;
        effects.extend(more);
    }
    (talk.map(|t| t.page.lines).unwrap_or_default(), effects)
}

#[test]
fn quest_giver_dialogues_run_quests() {
    let (mut host, root) = project("quests", &[]);
    add_quests(&root);
    let quests = Quest::all(&c3_data::load_table(&root.join("tables/quests.table.ron")).unwrap());
    let graph = DialogueGraph::quest_giver(&quests[0]);
    assert!(graph.problems().is_empty(), "{:?}", graph.problems());
    let source = graph.to_script("pheasants");
    assert!(host.check(&source, Some(TALK)).is_empty(), "{source}");
    let file = host.script_file(&generated_script("pheasants"));
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, &source).unwrap();
    let script = generated_script("pheasants");

    let mut player = PlayerState {
        name: "Hero".into(),
        level: 1,
        ..Default::default()
    };
    let (lines, _) = walk(&mut host, &script, &mut player, &[]);
    assert_eq!(lines, ["Come back when you are level 2, Hero."]);
    player.level = 2;
    let (_, effects) = walk(&mut host, &script, &mut player, &["I'll do it"]);
    assert_eq!(
        effects,
        [Effect::QuestTaken {
            id: 1,
            name: "Pheasants".into()
        }]
    );
    assert_eq!(player.flags["quest_1"], quest::TAKEN);
    let (lines, _) = walk(&mut host, &script, &mut player, &[]);
    assert_eq!(lines[1], "Come back when it is done.");
    player.flags.insert(quest::kills_flag(1), 2);
    let (_, effects) = walk(&mut host, &script, &mut player, &[]);
    assert!(effects.contains(&Effect::Exp(80)) && effects.contains(&Effect::Money(500)));
    assert_eq!((player.money, player.item_count(7)), (500, 1));
    assert_eq!(player.flags["quest_1"], quest::DONE);
    assert!(!player.flags.contains_key(&quest::kills_flag(1)));
    let (lines, _) = walk(&mut host, &script, &mut player, &[]);
    assert_eq!(lines, ["Thank you again for your help, Hero."]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn collect_quests_take_the_items() {
    let script = r#"
        fn talk() {
            if start_quest(2) { text("taken"); }
            if complete_quest(2) { text("done"); }
            text(quest_name(2) + " " + quest_progress(2) + " " + quest_ready(2));
        }
    "#;
    let (mut host, root) = project("collect", &[("q", script)]);
    add_quests(&root);
    let mut player = PlayerState {
        level: 5,
        ..Default::default()
    };
    player.give_item(9, 2);
    let lines = host.talk("q", &npc(), &mut player).unwrap().page.lines;
    assert_eq!(lines, ["taken", "Feathers 2 false"]);
    player.give_item(9, 2);
    let reply = host.talk("q", &npc(), &mut player).unwrap();
    assert_eq!(reply.page.lines, ["done", "Feathers 1 false"]);
    assert_eq!(player.item_count(9), 1, "three handed over");
    assert!(host.talk("missing", &npc(), &mut player).is_err());
    let e = host
        .check("fn talk() { quest_name(99); }", Some(TALK))
        .is_empty();
    assert!(e, "unknown quests fail when run, not when checked");
    let _ = std::fs::remove_dir_all(root);
}
