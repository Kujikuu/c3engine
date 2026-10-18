//! Music by place: a map's music regions each play an optional title track when the player
//! walks in, then their tracks in turn with a pause between them.

use c3_scene::CellPos;

/// A music file and how long it plays (seconds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub path: String,
    pub seconds: u32,
}

/// An area of a map with its music.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicRegion {
    /// The game map it belongs to.
    pub map: u32,
    /// First and last cell, inclusive.
    pub min: CellPos,
    pub max: CellPos,
    /// Played once on entering.
    pub title: Option<Track>,
    /// Played in turn, over and over.
    pub tracks: Vec<Track>,
    /// Seconds of quiet between tracks.
    pub pause: u32,
}

impl MusicRegion {
    pub fn contains(&self, map: u32, cell: CellPos) -> bool {
        self.map == map
            && (self.min.0..=self.max.0).contains(&cell.0)
            && (self.min.1..=self.max.1).contains(&cell.1)
    }
}

/// Decides what music plays as the player moves.
#[derive(Debug, Default)]
pub struct MusicDirector {
    regions: Vec<MusicRegion>,
    /// The region the player is in.
    current: Option<usize>,
    /// The next track to start: index into the region's tracks.
    next: usize,
    /// When (seconds) the next track starts.
    next_at: f64,
}

impl MusicDirector {
    pub fn new(regions: Vec<MusicRegion>) -> Self {
        Self {
            regions,
            ..Default::default()
        }
    }

    /// The music to play now for a player on `cell` of one of `maps` (`now` in seconds):
    /// `Some(Some(path))` to start a track, `Some(None)` to stop, `None` to keep what plays.
    pub fn update(&mut self, maps: &[u32], cell: CellPos, now: f64) -> Option<Option<String>> {
        let region = self
            .regions
            .iter()
            .position(|r| maps.iter().any(|&m| r.contains(m, cell)));
        if region != self.current {
            self.current = region;
            self.next = 0;
            let Some(r) = region.map(|i| &self.regions[i]) else {
                return Some(None);
            };
            if let Some(title) = &r.title {
                self.next_at = now + f64::from(title.seconds + r.pause);
                return Some(Some(title.path.clone()));
            }
            self.next_at = now;
        }
        let r = &self.regions[self.current?];
        if now < self.next_at || r.tracks.is_empty() {
            return None;
        }
        let track = &r.tracks[self.next % r.tracks.len()];
        self.next += 1;
        self.next_at = now + f64::from(track.seconds + r.pause);
        Some(Some(track.path.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(path: &str, seconds: u32) -> Track {
        Track {
            path: path.into(),
            seconds,
        }
    }

    #[test]
    fn title_then_tracks_in_turn_and_silence_outside() {
        let mut d = MusicDirector::new(vec![MusicRegion {
            map: 1,
            min: (0, 0),
            max: (10, 10),
            title: Some(track("title", 30)),
            tracks: vec![track("a", 60), track("b", 60)],
            pause: 5,
        }]);
        let play = |p: &str| Some(Some(p.to_owned()));
        assert_eq!(d.update(&[1], (5, 5), 0.0), play("title"));
        assert_eq!(d.update(&[1], (6, 5), 10.0), None);
        assert_eq!(d.update(&[1], (6, 5), 35.0), play("a"));
        assert_eq!(d.update(&[1], (6, 5), 100.0), play("b"));
        assert_eq!(d.update(&[1], (6, 5), 170.0), play("a"));
        assert_eq!(d.update(&[1], (50, 5), 171.0), Some(None), "walked out");
        assert_eq!(d.update(&[2], (5, 5), 172.0), None, "another map");
    }
}
