//! Character assembly against the real 5065 client. Skipped when the client is absent.

use game_conquer::Content;
use game_conquer::character::{Look, action};

fn content() -> Option<Content> {
    let dir = std::env::var("C3_CLIENT_DIR")
        .unwrap_or_else(|_| r"D:\Conquer2\Tools\eosource\client5065".into());
    Content::open(&dir).ok()
}

#[test]
fn newbie_with_hair_uses_base_body_and_stand_motion() {
    let Some(c) = content() else { return };
    let look = Look {
        look: 3,
        hair: 310,
        ..Default::default()
    };
    let ch = c.assemble_character(&look, action::STANDBY).unwrap();
    assert_eq!(ch.motion_id, Some(3_000_100));
    assert!(
        ch.body
            .texture
            .as_deref()
            .is_some_and(|t| t.contains("003000000")),
        "{:?}",
        ch.body.texture
    );
    // Hair hangs on the helmet socket, which the body motion animates.
    assert_eq!(
        ch.attachments.iter().map(|a| a.socket).collect::<Vec<_>>(),
        ["v_armet"]
    );
    assert!(ch.body.model.socket_transform("v_armet", 0.0).is_some());
    assert!(
        ch.body.model.parts.iter().any(|p| !p.visible),
        "socket parts must be hidden"
    );
}

#[test]
fn armor_and_blade_use_weapon_family_motion() {
    let Some(c) = content() else { return };
    let look = Look {
        look: 3,
        hair: 310,
        armor: 130_003,
        r_weapon: 410_003,
        ..Default::default()
    };
    let ch = c.assemble_character(&look, action::STANDBY).unwrap();
    assert_eq!(ch.motion_id, Some(3_410_100));
    assert!(
        ch.body
            .texture
            .as_deref()
            .is_some_and(|t| t.contains("003130300")),
        "{:?}",
        ch.body.texture
    );
    let sockets: Vec<_> = ch.attachments.iter().map(|a| a.socket).collect();
    assert_eq!(sockets, ["v_armet", "v_r_weapon"]);
    assert!(ch.body.model.socket_transform("v_r_weapon", 3.0).is_some());
}

#[test]
fn bow_is_held_in_left_socket_with_weapon_motion() {
    let Some(c) = content() else { return };
    let look = Look {
        look: 3,
        r_weapon: 500_003,
        ..Default::default()
    };
    let ch = c.assemble_character(&look, action::STANDBY).unwrap();
    assert_eq!(ch.motion_id, Some(3_500_100));
    assert_eq!(
        ch.attachments.iter().map(|a| a.socket).collect::<Vec<_>>(),
        ["v_l_weapon"]
    );
}

#[test]
fn walk_and_run_have_motions_and_timing() {
    let Some(c) = content() else { return };
    let look = Look {
        look: 3,
        hair: 310,
        armor: 130003,
        r_weapon: 410003,
        ..Default::default()
    };
    let mut ch = c.assemble_character(&look, action::STANDBY).unwrap();
    for act in [action::WALK_L, action::RUN_L, action::RUN_R] {
        let motions = c.character_motions(&ch, &look, act);
        assert_eq!(motions.models.len(), 1 + ch.attachments.len());
        assert!(
            motions.body_id.is_some_and(|id| id % 1000 == act as u64),
            "{act}: {:?}",
            motions.body_id
        );
        ch.apply_motions(&motions);
        let frames = ch.body.model.frame_count();
        let ms = c.frame_interval(&look, act);
        assert!(
            frames > 0 && (20..200).contains(&ms),
            "{act}: {frames} frames × {ms} ms"
        );
        println!(
            "action {act}: {frames} frames × {ms} ms = {} ms per step",
            frames * ms
        );
    }
}
