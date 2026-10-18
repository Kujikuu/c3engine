//! Conquer's sound: action sounds (`ActionSound.ini`: `look.weapon.action=file`, 999 for
//! any weapon), item-use sounds and effects (`MediaEffect.ini`), music regions
//! (`MusicRegion.ini`), and [`GameAudio`], which plays them for what happens in a game.

use crate::Content;
use crate::character::{action, weapon_family};
use crate::sim_view::player_look;
use c3_audio::{Audio, MusicDirector, MusicRegion, Track};
use c3_formats_tq::ini::Ini;
use c3_game::{EntityId, Event, Sim, distance};
use std::collections::HashMap;
use std::rc::Rc;

/// Weapon family meaning "any weapon" in `ActionSound.ini`.
const ANY_WEAPON: u32 = 999;
/// Action of a dying role (`_ACTION_DIE`).
const DIE: u32 = 330;

/// Key of an action sound.
fn key(look: u32, weapon: u32, act: u32) -> u64 {
    u64::from(look) * 1_000_000 + u64::from(weapon) * 1000 + u64::from(act)
}

/// Sound tables of the client's `ini/`.
#[derive(Debug, Default)]
pub(crate) struct SoundTables {
    actions: HashMap<u64, String>,
    /// Item type → (effect, sound) when used.
    items: HashMap<u32, (Option<String>, Option<String>)>,
    music: Vec<MusicRegion>,
}

/// A file value of the ini files, `None` for "none"/"NULL"/empty.
fn file(value: &str) -> Option<String> {
    let v = value.trim();
    (!v.is_empty() && !v.eq_ignore_ascii_case("none") && !v.eq_ignore_ascii_case("null"))
        .then(|| v.to_owned())
}

impl SoundTables {
    pub fn parse(action_sound: &[u8], media_effect: &[u8], music_region: &[u8]) -> Self {
        let actions = String::from_utf8_lossy(action_sound)
            .lines()
            .filter_map(|line| {
                let (k, v) = line.split_once('=')?;
                let mut n = k.trim().split('.').map(|p| p.parse::<u32>().ok());
                let (look, weapon, act) = (n.next()??, n.next()??, n.next()??);
                Some((key(look, weapon, act), file(v)?))
            })
            .collect();
        let items = Ini::parse(media_effect)
            .sections
            .iter()
            .filter_map(|s| {
                let id = s.name.trim().parse().ok()?;
                let get = |k: &str| s.get(k).and_then(file);
                Some((id, (get("Effect"), get("Sound"))))
            })
            .collect();
        let music = Ini::parse(music_region)
            .sections
            .iter()
            .filter_map(|s| {
                let num = |k: &str| s.get_parsed::<i64>(k);
                let (x, y) = (num("Bound_X")? as i32, num("Bound_Y")? as i32);
                let (w, h) = (num("Bound_CX")? as i32, num("Bound_CY")? as i32);
                let track = |path: &str, secs: &str| {
                    Some(Track {
                        path: file(s.get(path)?)?,
                        seconds: s.get_parsed(secs).unwrap_or(60),
                    })
                };
                let amount = s.get_parsed::<usize>("Amount").unwrap_or(0);
                Some(MusicRegion {
                    map: num("Mapid")? as u32,
                    min: (x, y),
                    max: (x + (w - 1).max(0), y + (h - 1).max(0)),
                    title: track("TitleMusic", "TitleMusicTime"),
                    tracks: (0..amount)
                        .filter_map(|i| track(&format!("Music{i}"), &format!("MusicTime{i}")))
                        .collect(),
                    pause: s.get_parsed("DelayTime").unwrap_or(0),
                })
            })
            .collect();
        Self {
            actions,
            items,
            music,
        }
    }
}

impl Content {
    /// The sound of `act` for a role of `look` holding `weapon` family (or any weapon).
    pub fn action_sound(&self, look: u32, weapon: u32, act: u32) -> Option<&str> {
        let sounds = &self.tables.sounds.actions;
        sounds
            .get(&key(look, weapon, act))
            .or_else(|| sounds.get(&key(look, ANY_WEAPON, act)))
            .map(String::as_str)
    }

    /// The effect and sound of using item type `kind`.
    pub fn item_use_media(&self, kind: u32) -> (Option<&str>, Option<&str>) {
        self.tables
            .sounds
            .items
            .get(&kind)
            .map_or((None, None), |(e, s)| (e.as_deref(), s.as_deref()))
    }

    pub fn music_regions(&self) -> &[MusicRegion] {
        &self.tables.sounds.music
    }
}

/// Sound for a game: what entities do, items used, and the music of where the hero is.
pub struct GameAudio {
    audio: Audio,
    music: MusicDirector,
    map_ids: Vec<u32>,
    content: Rc<Content>,
}

impl GameAudio {
    pub fn new(content: Rc<Content>, map_ids: Vec<u32>) -> Self {
        let files = content.clone();
        let audio = Audio::new(Box::new(move |path| {
            files.files().read(path).map(|b| b.into_owned())
        }));
        Self {
            audio,
            music: MusicDirector::new(content.music_regions().to_vec()),
            map_ids,
            content,
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
                Event::Moved { id, running, .. } if id == hero => (
                    id,
                    if running {
                        action::RUN_L
                    } else {
                        action::WALK_L
                    },
                ),
                Event::Attacked { attacker, .. } => (attacker, action::ATTACK_0),
                Event::Died { id, .. } => (id, DIE),
                Event::Used { player, kind } if player == hero => {
                    if let (_, Some(sound)) = self.content.item_use_media(kind) {
                        let sound = sound.to_owned();
                        self.audio.play(&sound, 1.0);
                    }
                    continue;
                }
                _ => continue,
            };
            let Some((look, weapon, cell)) = role(sim, who) else {
                continue;
            };
            if let Some(sound) = self.content.action_sound(look, weapon, act) {
                let sound = sound.to_owned();
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

/// The look, weapon family and cell of a player or monster.
fn role(sim: &Sim, id: EntityId) -> Option<(u32, u32, c3_scene::CellPos)> {
    if let Some(p) = sim.player(id) {
        let look = player_look(p);
        return Some((
            look.look,
            weapon_family(look.l_weapon, look.r_weapon),
            p.cell,
        ));
    }
    let m = sim.monster(id)?;
    let look = sim.rules().monster(m.kind)?.look % 1000;
    Some((look, ANY_WEAPON, m.cell))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_action_sounds_media_and_music() {
        let t = SoundTables::parse(
            b"3.410.401=sound/falchion_m01.wav\n104.999.330=sound/d_monster01.wav\n104.999.320=none\n",
            b"[1000000]\nEffect=heal1-1\nSound=sound/gethp.wav\n",
            b"[MusicRegion001]\nMapid=1002\nBound_X=10\nBound_Y=20\nBound_CX=5\nBound_CY=5\n\
              TitleMusic=NULL\nTitleMusicTime=0\nAmount=1\nMusic0=sound/music07.mp3\nMusicTime0=67\nDelayTime=5\n",
        );
        assert_eq!(
            t.actions.get(&key(3, 410, 401)).map(String::as_str),
            Some("sound/falchion_m01.wav")
        );
        assert!(
            !t.actions.contains_key(&key(104, 999, 320)),
            "none is no sound"
        );
        assert_eq!(t.items[&1000000].1.as_deref(), Some("sound/gethp.wav"));
        let r = &t.music[0];
        assert_eq!(
            (r.map, r.min, r.max, r.title.is_none()),
            (1002, (10, 20), (14, 24), true)
        );
        assert_eq!((r.tracks[0].seconds, r.pause), (67, 5));
    }
}
