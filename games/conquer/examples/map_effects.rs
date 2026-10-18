//! Lists a map's effect entities with their cells and whether they load; without a map
//! id, counts the effect entities of every map.
//! Usage: map_effects [map_id]
use c3_scene::EntityKind;
use game_conquer::Content;

fn main() {
    let c = Content::open(r"D:\Conquer2\Tools\eosource\client5065").unwrap();
    let Some(id) = std::env::args().nth(1).and_then(|v| v.parse().ok()) else {
        let ids: Vec<u32> = c.map_ids().collect();
        for id in ids {
            if let Ok((path, scene, _)) = c.load_map_id(id) {
                let n = scene
                    .entities
                    .iter()
                    .filter(|e| matches!(e.kind, EntityKind::Effect { .. }))
                    .count();
                if n > 0 {
                    println!("{id:5} {path:40} {n} effects");
                }
            }
        }
        return;
    };
    let (path, scene, _) = c.load_map_id(id).unwrap();
    println!("{path}");
    for e in &scene.entities {
        if let EntityKind::Effect { name, transform } = &e.kind {
            let world = scene.entity_world(e);
            let status = match c.load_effect(name) {
                Ok(fx) => format!(
                    "{} layers, {} frames, offset {:?}",
                    fx.layers.len(),
                    fx.frame_count(),
                    fx.offset
                ),
                Err(err) => err,
            };
            println!(
                "{name:20} cell {:?} {}: {status}",
                scene.world_to_cell(world),
                if transform.is_some() { "new" } else { "" }
            );
        }
    }
}
