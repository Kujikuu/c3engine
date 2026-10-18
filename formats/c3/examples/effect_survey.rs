//! Counts chunk kinds across every effect in `3DEffectObj.ini`, to prioritise playback work.
//! Usage: effect_survey [client_dir]
use c3_formats_tq::{c3::C3File, ini::PathTable, vfs::ClientFs};
use std::collections::BTreeMap;

fn main() {
    let root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| r"D:\Conquer2\Tools\eosource\client5065".into());
    let fs = ClientFs::open(&root).expect("client folder");
    let table = PathTable::parse(&fs.read("ini/3DEffectObj.ini").expect("3DEffectObj.ini"));
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut combos: BTreeMap<String, usize> = BTreeMap::new();
    let (mut missing, mut broken) = (0, 0);
    for path in table.paths.values() {
        let Some(bytes) = fs.read(path) else {
            missing += 1;
            continue;
        };
        let Ok(file) = C3File::parse(&bytes) else {
            broken += 1;
            continue;
        };
        let mut ids: Vec<String> = file.chunks.iter().map(|c| c.id_str()).collect();
        for id in &ids {
            *kinds.entry(id.clone()).or_default() += 1;
        }
        ids.dedup();
        *combos.entry(ids.join("+")).or_default() += 1;
    }
    println!(
        "{} effects, {missing} missing, {broken} unparsable",
        table.paths.len()
    );
    println!("chunks: {kinds:?}");
    let mut combos: Vec<_> = combos.into_iter().collect();
    combos.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (combo, n) in combos.iter().take(15) {
        println!("{n:6}  {combo}");
    }
}
