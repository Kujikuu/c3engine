//! Sound for a game being played: what roles do, items used, and the music of where the hero
//! stands, as the game ([`GameModule`]) says they sound.

use crate::module::{Act, GameModule};
use c3_audio::{Audio, MusicDirector, SoundFile, Volumes};
use c3_core::asset::Importer;
use c3_core::settings::{AudioSettings, PlayerPrefs};
use c3_game::{EntityId, Event, Sim, distance};
use std::rc::Rc;

pub struct GameAudio {
    audio: Audio,
    music: MusicDirector,
    map_ids: Vec<u32>,
    game: Rc<dyn GameModule>,
    /// The music of the region the hero stands in (the game's music regions).
    region_music: Option<String>,
    /// The map's own music (its entry in the map list), when no region has any.
    map_music: Option<String>,
    /// The project's volumes and the player's, which scale them.
    project: Volumes,
    player: PlayerPrefs,
}

impl GameAudio {
    pub fn new(game: Rc<dyn GameModule>, map_ids: Vec<u32>) -> Self {
        let files = Rc::clone(&game);
        let audio = Audio::new(Box::new(move |path| sound_file(&*files, path)));
        Self {
            audio,
            music: MusicDirector::new(game.music_regions()),
            map_ids,
            game,
            region_music: None,
            map_music: None,
            project: Volumes::default(),
            player: PlayerPrefs::default(),
        }
    }

    /// Plays on map `map_ids` from now on, `music` being the map's own.
    pub fn set_map(&mut self, map_ids: Vec<u32>, music: Option<String>) {
        self.map_ids = map_ids;
        self.map_music = music;
        self.music = MusicDirector::new(self.game.music_regions());
        self.region_music = None;
    }

    /// `None` without a sound device, else whether sound is muted.
    pub fn muted(&self) -> Option<bool> {
        self.audio.has_device().then(|| self.audio.muted())
    }

    /// Plays by `settings` from now on.
    pub fn apply(&mut self, settings: &AudioSettings) {
        self.project = Volumes {
            master: settings.master_volume as f32,
            music: settings.music_volume as f32,
            sound: settings.sound_volume as f32,
        };
        self.audio.hearing_range = settings.hearing_range as f32;
        self.set_volumes();
    }

    /// Plays at the player's volumes (over the project's) from now on.
    pub fn set_player(&mut self, prefs: PlayerPrefs) {
        self.player = prefs;
        self.set_volumes();
    }

    fn set_volumes(&mut self) {
        let (p, q) = (self.project, self.player);
        self.audio.set_volumes(Volumes {
            master: p.master * q.master,
            music: p.music * q.music,
            sound: p.sound * q.sound,
        });
    }

    /// Reads sound `path` again (it changed on disk), or every sound.
    pub fn reload(&mut self, path: Option<&str>) {
        self.audio.forget(path);
    }

    /// Plays a sound of the game's UI (a click), as loud as any sound beside the hero.
    pub fn play_ui(&mut self, path: &str) {
        self.audio.play(path, 1.0);
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
                Event::Cast { caster, skill, .. } => {
                    let sound = (sim.rules().skill(skill)).and_then(|t| self.game.skill_media(t).1);
                    if let (Some(sound), Some(cell)) = (sound, sim.cell_of(caster)) {
                        self.audio.play_at(&sound, distance(here, cell) as f32);
                    }
                    continue;
                }
                Event::Sound { entity, ref sound } => {
                    if let Some(cell) = sim.cell_of(entity) {
                        self.audio.play_at(sound, distance(here, cell) as f32);
                    }
                    continue;
                }
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

    /// Music and ambience for where the hero stands (`now` in seconds): a region's own
    /// music first, then the game's music regions, then the map's music.
    pub fn update(&mut self, sim: &Sim, hero: EntityId, now: f64) {
        let Some(cell) = sim.player(hero).map(|p| p.cell) else {
            return;
        };
        if let Some(track) = self.music.update(&self.map_ids, cell, now) {
            self.region_music = track;
        }
        let here = |pick: fn(&c3_scene::Region) -> &String| {
            sim.regions_at(cell)
                .map(pick)
                .find(|s| !s.is_empty())
                .cloned()
        };
        let own = here(|r| &r.music);
        let track = own
            .as_ref()
            .or(self.region_music.as_ref())
            .or(self.map_music.as_ref());
        self.audio.set_music(track.map(String::as_str));
        self.audio.set_ambience(here(|r| &r.sound).as_deref());
    }
}

/// Sound `path` of `game`, played as its import settings say (the project's own sounds).
pub fn sound_file(game: &dyn GameModule, path: &str) -> Option<SoundFile> {
    let mut file = SoundFile::plain(game.read_file(path)?);
    if let Some(Importer::Sound(s)) = game.native().and_then(|n| n.importer(path)) {
        file.volume = s.volume;
        file.looped = s.looped;
    }
    Some(file)
}
