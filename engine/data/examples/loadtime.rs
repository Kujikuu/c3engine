//! Times saving and loading a large table (a check for the file format's speed).
fn main() {
    let path = std::env::args().nth(1).expect("an SQL dump");
    let sql = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
    let table = c3_data::sql::import_tables(&sql, &["cq_itemtype"]).remove(0);
    let file = std::env::temp_dir().join("loadtime.table.ron");
    let t = std::time::Instant::now();
    c3_data::save_table(&table, &file).unwrap();
    println!("save {:?}", t.elapsed());
    let t = std::time::Instant::now();
    let loaded = c3_data::load_table(&file).unwrap();
    println!(
        "load {:?}: {} rows, same = {}",
        t.elapsed(),
        loaded.rows.len(),
        loaded == table
    );
}
