//! Effect loading against the real 5065 client. Skipped when the client is absent.

use c3_assets::effect::{Blend, Playback};
use game_conquer::Content;

fn content() -> Option<Content> {
    let dir = std::env::var("C3_CLIENT_DIR")
        .unwrap_or_else(|_| r"D:\Conquer2\Tools\eosource\client5065".into());
    Content::open(&dir).ok()
}

#[test]
fn map_fire_is_an_endless_textured_mesh_effect() {
    let Some(c) = content() else { return };
    let e = c.load_effect("M_Fire").unwrap();
    assert_eq!(e.layers.len(), 1);
    let layer = &e.layers[0];
    assert!(!layer.meshes.is_empty() || !layer.particles.is_empty());
    assert!(layer.texture.is_some());
    assert_eq!(layer.blend, Blend::ALPHA);
    assert!(e.frame_count() > 0);
    assert!(matches!(e.playback(0), Playback::Frame(0)));
}

#[test]
fn nearly_every_effect_loads() {
    let Some(c) = content() else { return };
    let names: Vec<String> = c.effect_names().map(str::to_owned).collect();
    let mut failed = Vec::new();
    let mut empty = 0;
    for name in names.iter().filter(|n| !n.is_empty()) {
        match c.load_effect(name) {
            Ok(e) if e.layers.is_empty() => empty += 1,
            Ok(_) => {}
            Err(err) => failed.push(err),
        }
    }
    assert!(
        failed.is_empty(),
        "{} failed: {:?}",
        failed.len(),
        &failed[..failed.len().min(5)]
    );
    // Some entries point at the 46 effect files missing from the client.
    assert!(empty < names.len() / 20, "{empty} of {} empty", names.len());
}

#[test]
fn jumps_and_run_steps_leave_ground_effects() {
    let Some(c) = content() else { return };
    // ActionMap3DEffect.ini: a jump (130) ends in dust on stone, a splash in water; a run
    // step (120) ends in run dust; nothing marks where they begin.
    assert_eq!(c.map_effect(3, 130, 3, true).as_deref(), Some("Dust"));
    assert_eq!(
        c.map_effect(3, 130, 1, true).as_deref(),
        Some("WaterSplash")
    );
    assert_eq!(c.map_effect(3, 120, 2, true).as_deref(), Some("RunDust"));
    assert_eq!(c.map_effect(3, 130, 3, false), None);
    assert!(c.load_effect("Dust").is_ok());
}
