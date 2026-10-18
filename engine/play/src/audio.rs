//! Sound for a game being played: what roles do, items used, and the music of where the hero
//! stands, as the game ([`GameModule`]) says they sound.

use crate::module::{Act, GameModule};
use c3_audio::{Audio, MusicDirector};
use c3_game::{EntityId, Event, Sim, distance};
use std::rc::Rc;

pub struct GameAudio {
    audio: Audio,
    music: MusicDirector,
    map_ids: Vec<u32>,
    game: Rc<dyn GameModule>,
}

impl GameAudio {
    pub fn new(game: Rc<dyn GameModule>, map_ids: Vec<u32>) -> Self {
        let files = Rc::clone(&game);
        let audio = Audio::new(Box::new(move |path| files.read_file(path)));
        Self {
            audio,
            music: MusicDirector::new(game.music_regions()),
            map_ids,
            game,
        }
    }

    /// `None` without a sound device, else whether sound is muted.
    pub fn muted(&self) -> Option<bool> {
        self.audio.has_device().then(|| self.audio.muted())
    }

    pub fn toggle_muted(&mut self) {
        let muted = self.audio.muted();
        self.audio.set_muted(!muted);
    }

    /// Plays what `events` sound like, heard from the hero.
    pub fn events(&mut self, events: &[Event], sim: &Sim, hero: EntityId) {
        let Some(here) = sim.player(hero).map(|p| p.cell) else {
            return;
        };
        for event in events {
            let (who, act) = match *event {
                Event::Moved { id, running, .. } if id == hero => {
                    (id, if running { Act::Run } else { Act::Walk })
                }
                Event::Attacked { attacker, .. } => (attacker, Act::Attack),
                Event::Died { id, .. } => (id, Act::Die),
                Event::Used { player, kind } if player == hero => {
                    if let (_, Some(sound)) = self.game.item_use(kind) {
                        self.audio.play(&sound, 1.0);
                    }
                    continue;
                }
                _ => continue,
            };
            let Some(cell) = sim.cell_of(who) else {
                continue;
            };
            if let Some(sound) = self.game.action_sound(sim, who, act) {
                self.audio.play_at(&sound, distance(here, cell) as f32);
            }
        }
    }

    /// Music for where the hero stands (`now` in seconds).
    pub fn update(&mut self, sim: &Sim, hero: EntityId, now: f64) {
        let Some(cell) = sim.player(hero).map(|p| p.cell) else {
            return;
        };
        if let Some(track) = self.music.update(&self.map_ids, cell, now) {
            self.audio.set_music(track.as_deref());
        }
    }
}
