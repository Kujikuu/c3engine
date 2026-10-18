//! c3-audio: the game's sound. [`Audio`] plays one-shot sounds (actions, effects, UI) softer
//! with distance from the listener, and one music track at a time; sound files come from the
//! game through a loader, decoded as WAV or MP3. [`MusicDirector`] picks music by where the
//! player stands (music regions). Without a sound device everything stays silent.

mod music;

pub use music::{MusicDirector, MusicRegion, Track};

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

/// Cells from the listener at which a sound fades out completely.
pub const HEARING_RANGE: f32 = 18.0;

/// Reads a sound file of the game by its path.
pub type Loader = Box<dyn Fn(&str) -> Option<Vec<u8>>>;

pub struct Audio {
    /// The sound device; `None` when there is none (silent).
    sink: Option<MixerDeviceSink>,
    music: Option<Player>,
    /// The music track playing.
    music_path: Option<String>,
    load: Loader,
    /// Loaded files (`None`: missing or unreadable, not tried again).
    files: HashMap<String, Option<Arc<[u8]>>>,
    muted: bool,
    pub sound_volume: f32,
    pub music_volume: f32,
}

impl Audio {
    /// Sound on the default device, files read through `load`.
    pub fn new(load: Loader) -> Self {
        let sink = DeviceSinkBuilder::open_default_sink().ok().map(|mut s| {
            s.log_on_drop(false);
            s
        });
        Self {
            sink,
            music: None,
            music_path: None,
            load,
            files: HashMap::new(),
            muted: false,
            sound_volume: 0.8,
            music_volume: 0.5,
        }
    }

    /// Whether a sound device is open.
    pub fn has_device(&self) -> bool {
        self.sink.is_some()
    }

    pub fn muted(&self) -> bool {
        self.muted
    }

    /// Mutes or unmutes everything (music pauses while muted).
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        if let Some(music) = &self.music {
            if muted {
                music.pause();
            } else {
                music.play();
            }
        }
    }

    fn file(&mut self, path: &str) -> Option<Arc<[u8]>> {
        let load = &self.load;
        self.files
            .entry(path.to_owned())
            .or_insert_with(|| load(path).map(Arc::from))
            .clone()
    }

    fn decode(&mut self, path: &str) -> Option<Decoder<Cursor<Arc<[u8]>>>> {
        let bytes = self.file(path)?;
        Decoder::new(Cursor::new(bytes)).ok()
    }

    /// Whether sound `path` can be read and decoded (for checking a game's files).
    pub fn can_play(&mut self, path: &str) -> bool {
        self.decode(path).is_some()
    }

    /// Plays sound `path` once at `volume` (0–1).
    pub fn play(&mut self, path: &str, volume: f32) {
        if self.muted || self.sink.is_none() || volume <= 0.0 {
            return;
        }
        let Some(source) = self.decode(path) else {
            return;
        };
        let gain = volume * self.sound_volume;
        if let Some(sink) = &self.sink {
            sink.mixer().add(source.amplify(gain));
        }
    }

    /// Plays sound `path` once, softer the farther (cells) it is from the listener.
    pub fn play_at(&mut self, path: &str, distance: f32) {
        self.play(path, 1.0 - (distance / HEARING_RANGE).clamp(0.0, 1.0));
    }

    /// Starts music `path` (from its start) unless it is already playing; `None` stops it.
    pub fn set_music(&mut self, path: Option<&str>) {
        if self.music_path.as_deref() == path {
            return;
        }
        if let Some(old) = self.music.take() {
            old.stop();
        }
        self.music_path = path.map(str::to_owned);
        let (Some(path), Some(sink)) = (path, &self.sink) else {
            return;
        };
        let player = Player::connect_new(sink.mixer());
        player.set_volume(self.music_volume);
        let Some(source) = self.decode(path) else {
            return;
        };
        player.append(source);
        if self.muted {
            player.pause();
        }
        self.music = Some(player);
    }
}
