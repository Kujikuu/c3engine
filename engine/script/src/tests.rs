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
