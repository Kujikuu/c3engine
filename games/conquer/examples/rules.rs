//! Loads Conquer's rules from a project's tables and prints a summary, to check them.
//! Usage: cargo run -p game-conquer --example rules -- <project>/tables

use c3_game::Rules;
use game_conquer::rules::ConquerRules;

fn main() {
    let dir = std::env::args()
        .nth(1)
        .expect("usage: rules <tables folder>");
    let (rules, problems) = ConquerRules::from_tables(std::path::Path::new(&dir));
    for p in &problems {
        eprintln!("problem: {p}");
    }
    println!("{} monster types", rules.monster_count());
    for level in [1, 2, 10, 50] {
        println!(
            "level {level}: {:?}, exp to next {:?}",
            rules.player_stats(&c3_game::StatsFor::level(level)),
            rules.level_exp(level)
        );
    }
    if let Some(m) = rules.monster(1) {
        println!("{m:?}");
    }
    println!("shop 3 sells {:?}", rules.shop(3));
    println!("item 1000000: {:?}", rules.item(1000000));
}
