//! Remakes a Conquer project's rule tables (items, shops, levels...) from the TQ tables it
//! keeps (`cq_*`), as importing does: for a project imported before the conversion changed.
//! Usage: cargo run -p game-conquer --example rebuild_rules -- <project folder>

use std::path::Path;

fn main() {
    let project = std::env::args().nth(1).expect("project folder");
    let dir = Path::new(&project).join("tables");
    let entries = std::fs::read_dir(&dir).expect("the project's tables folder");
    let tq: Vec<c3_data::Table> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            name.starts_with("cq_") && name.ends_with(".table.ron")
        })
        .filter_map(|p| c3_data::load_table(&p).ok())
        .collect();
    let (tables, problems) = game_conquer::rules::engine_tables(&tq);
    for problem in problems {
        eprintln!("{problem}");
    }
    for table in &tables {
        let file = dir.join(format!("{}.table.ron", table.name));
        c3_data::save_table(table, &file).expect("the table written");
        println!("{} ({} rows)", table.name, table.rows.len());
    }
}
