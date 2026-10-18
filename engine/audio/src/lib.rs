//! c3-audio: the game's sound. [`Audio`] plays one-shot sounds (actions, effects, UI) softer
//! with distance from the listener, and one music track at a time; sound files come from the
//! game through a loader ([`SoundFile`]: the bytes and how loud and whether to loop), decoded
//! as WAV, OGG Vorbis or MP3. [`MusicDirector`] picks music by where the
//! player stands (music regions). Without a sound device everything stays silent.

mod music;

pub use music::{MusicDirector, MusicRegion, Track};

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

/// Reads a sound file of the game by its path.
pub type Loader = Box<dyn Fn(&str) -> Option<SoundFile>>;

/// A sound file and how it plays (its import settings).
#[derive(Debug, Clone)]
pub struct SoundFile {
    pub bytes: Vec<u8>,
    /// Loudness, 0–1, on top of the volumes.
    pub volume: f32,
    /// Music starts again when it ends.
    pub looped: bool,
}

impl SoundFile {
    /// `bytes` played as recorded, once.
    pub fn plain(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            volume: 1.0,
            looped: false,
        }
    }
}

/// A loaded file's samples.
type SoundDecoder = Decoder<Cursor<Arc<[u8]>>>;

/// A loaded file: shared bytes and how it plays.
#[derive(Clone)]
struct Loaded {
    bytes: Arc<[u8]>,
    volume: f32,
    looped: bool,
}

pub struct Audio {
    /// The sound device; `None` when there is none (silent).
    sink: Option<MixerDeviceSink>,
    music: Option<Player>,
    /// The music track playing.
    music_path: Option<String>,
    load: Loader,
    /// Loaded files (`None`: missing or unreadable, not tried again).
    files: HashMap<String, Option<Loaded>>,
    muted: bool,
    volumes: Volumes,
    /// Cells from the listener at which a sound fades out completely.
    pub hearing_range: f32,
}

/// How loud sound plays, each 0–1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Volumes {
    /// Everything.
    pub master: f32,
    pub music: f32,
    /// One-shot sounds.
    pub sound: f32,
}

impl Default for Volumes {
    fn default() -> Self {
        Self {
            master: 1.0,
            music: 0.5,
            sound: 0.8,
        }
    }
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
            volumes: Volumes::default(),
            hearing_range: 18.0,
        }
    }

    pub fn volumes(&self) -> Volumes {
        self.volumes
    }

    /// Plays at `volumes` from now on (the music playing too).
    pub fn set_volumes(&mut self, volumes: Volumes) {
        self.volumes = volumes;
        if let Some(music) = &self.music {
            music.set_volume(volumes.master * volumes.music);
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

    fn file(&mut self, path: &str) -> Option<Loaded> {
        let load = &self.load;
        self.files
            .entry(path.to_owned())
            .or_insert_with(|| {
                load(path).map(|f| Loaded {
                    bytes: Arc::from(f.bytes),
                    volume: f.volume,
                    looped: f.looped,
                })
            })
            .clone()
    }

    fn decode(&mut self, path: &str) -> Option<(SoundDecoder, Loaded)> {
        let file = self.file(path)?;
        let decoder = Decoder::new(Cursor::new(Arc::clone(&file.bytes))).ok()?;
        Some((decoder, file))
    }

    /// Whether sound `path` can be read and decoded (for checking a game's files).
    pub fn can_play(&mut self, path: &str) -> bool {
        self.decode(path).is_some()
    }

    /// Forgets loaded file `path` (it changed on disk), or every file.
    pub fn forget(&mut self, path: Option<&str>) {
        match path {
            Some(p) => {
                self.files.remove(p);
            }
            None => self.files.clear(),
        }
    }

    /// Plays sound `path` once at `volume` (0–1).
    pub fn play(&mut self, path: &str, volume: f32) {
        if self.muted || self.sink.is_none() || volume <= 0.0 {
            return;
        }
        let Some((source, file)) = self.decode(path) else {
            return;
        };
        let gain = volume * file.volume * self.volumes.master * self.volumes.sound;
        if let Some(sink) = &self.sink {
            sink.mixer().add(source.amplify(gain));
        }
    }

    /// Plays sound `path` once, softer the farther (cells) it is from the listener.
    pub fn play_at(&mut self, path: &str, distance: f32) {
        let range = self.hearing_range.max(f32::EPSILON);
        self.play(path, 1.0 - (distance / range).clamp(0.0, 1.0));
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
        let Some((source, file)) = self.decode(path) else {
            return;
        };
        player.set_volume(file.volume * self.volumes.master * self.volumes.music);
        if file.looped {
            player.append(source.repeat_infinite());
        } else {
            player.append(source);
        }
        if self.muted {
            player.pause();
        }
        self.music = Some(player);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ogg_wav_and_missing_files_are_told_apart() {
        let ogg = include_bytes!("../tests/tone.ogg").to_vec();
        // A tenth of a second of silence as 16-bit mono WAV.
        let mut wav = Vec::new();
        let samples = 2205u32;
        wav.extend(b"RIFF");
        wav.extend((36 + samples * 2).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(22050u32.to_le_bytes());
        wav.extend(44100u32.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend((samples * 2).to_le_bytes());
        wav.extend(vec![0u8; (samples * 2) as usize]);
        let mut audio = Audio::new(Box::new(move |path| match path {
            "theme.ogg" => Some(SoundFile::plain(ogg.clone())),
            "hit.wav" => Some(SoundFile::plain(wav.clone())),
            "noise.ogg" => Some(SoundFile::plain(vec![1, 2, 3])),
            _ => None,
        }));
        assert!(audio.can_play("theme.ogg"));
        assert!(audio.can_play("hit.wav"));
        assert!(!audio.can_play("noise.ogg"));
        assert!(!audio.can_play("missing.wav"));
    }
}
