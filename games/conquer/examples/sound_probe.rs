//! Checks that a client's sounds load and decode: action sounds, item sounds and music.
//! Usage: cargo run -p game-conquer --example sound_probe -- <client folder>

use std::rc::Rc;

fn main() {
    let dir = std::env::args()
        .nth(1)
        .expect("usage: sound_probe <client folder>");
    let content = Rc::new(game_conquer::Content::open(dir).unwrap());
    let files = content.clone();
    let mut audio = c3_audio::Audio::new(Box::new(move |p| {
        files
            .files()
            .read(p)
            .map(|b| c3_audio::SoundFile::plain(b.into_owned()))
    }));
    println!("sound device: {}", audio.has_device());
    let mut paths: Vec<String> = [
        (3, 410, 401),
        (3, 999, 110),
        (104, 999, 330),
        (104, 999, 401),
    ]
    .iter()
    .filter_map(|&(l, w, a)| content.action_sound(l, w, a).map(str::to_owned))
    .collect();
    paths.extend(content.item_use_media(1000000).1.map(str::to_owned));
    let regions = content.music_regions();
    println!("{} music regions", regions.len());
    paths.extend(
        regions
            .iter()
            .take(3)
            .flat_map(|r| r.tracks.iter().map(|t| t.path.clone())),
    );
    for p in paths {
        println!(
            "{p}: {}",
            if audio.can_play(&p) {
                "ok"
            } else {
                "CAN'T PLAY"
            }
        );
    }
}
