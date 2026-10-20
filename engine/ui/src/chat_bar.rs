//! The game UI's chat bar, as TQ's CDlgTalk keeps it: the lines said (Down steps back
//! through them, Up forward), the names listed to talk to, the recent channel-and-name
//! pairs Ctrl+Left/Right cycle, the channels Ctrl+Up/Down cycle and the flood guard.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// The channels in the bar's order (CDlgTalk's talk modes).
pub(crate) const CHANNELS: [&str; 6] = ["talk", "whisper", "team", "guild", "friend", "service"];
/// The lines kept to step through.
const HISTORY: usize = 20;
/// The names a list holds.
const TARGETS: usize = 18;
/// The channel-and-name pairs kept.
const RECENT: usize = 5;
/// The least time between two lines.
const GAP: Duration = Duration::from_millis(500);

/// How long a line to the game masters waits for the next (TQ's 20 s).
const SERVICE_GAP: Duration = Duration::from_millis(20_000);

#[derive(Debug, Default)]
pub(crate) struct ChatBar {
    /// Lines said, the latest first.
    history: VecDeque<String>,
    /// The line Up/Down shows next.
    at: usize,
    /// The names listed to talk to.
    pub targets: Vec<String>,
    /// Channel-and-name pairs talked on, the oldest first, and the one shown.
    recent: VecDeque<(String, String)>,
    recent_at: usize,
    last: Option<Instant>,
    /// When a line last went to the game masters.
    served: Option<Instant>,
}

impl ChatBar {
    /// Whether a line to the game masters now comes within 20 s of the last; else it
    /// counts as sent now (CDlgTalk's W_CLICK).
    pub fn served_too_soon(&mut self, now: Instant) -> bool {
        if self
            .served
            .is_some_and(|t| now.duration_since(t) <= SERVICE_GAP)
        {
            return true;
        }
        self.served = Some(now);
        false
    }

    /// Whether a line now comes too soon after the last; else it counts as said now.
    pub fn too_soon(&mut self, now: Instant) -> bool {
        if self.last.is_some_and(|t| now.duration_since(t) <= GAP) {
            return true;
        }
        self.last = Some(now);
        false
    }

    /// `line` was said on `channel` (to `name`): kept to step through, the pair kept.
    pub fn said(&mut self, line: &str, channel: &str, name: &str) {
        // Only talk, whisper, friend and service keep the name (SetTalkTargetList).
        let name = match channel {
            "talk" | "whisper" | "friend" | "service" => name,
            _ => "",
        };
        let pair = (channel.to_owned(), name.to_owned());
        if !self.recent.contains(&pair) {
            if self.recent.len() == RECENT {
                self.recent.pop_front();
            }
            self.recent.push_back(pair);
        }
        self.history.retain(|l| l != line);
        self.history.push_front(line.to_owned());
        self.history.truncate(HISTORY);
        self.at = 0;
    }

    /// The line Down shows (an older one), given what the line reads now.
    pub fn older(&mut self, current: &str) -> Option<String> {
        if self.at >= self.history.len() {
            return None;
        }
        if self.at == 0 && self.history[0] == current && self.history.len() > 1 {
            self.at = 1;
        }
        let line = self.history[self.at].clone();
        if self.at + 1 < self.history.len() {
            self.at += 1;
        }
        Some(line)
    }

    /// The line Up shows (a newer one).
    pub fn newer(&mut self) -> Option<String> {
        if self.at == 0 {
            return None;
        }
        self.at -= 1;
        self.history.get(self.at).cloned()
    }

    /// The pair before (`back`) or after the one shown, which it becomes.
    pub fn cycle_recent(&mut self, back: bool) -> Option<(String, String)> {
        let n = self.recent.len();
        if n == 0 {
            return None;
        }
        self.recent_at = match back {
            true => (self.recent_at + n - 1) % n,
            false => (self.recent_at + 1) % n,
        };
        self.recent.get(self.recent_at).cloned()
    }

    /// Whether `name` is listed.
    pub fn listed(&self, name: &str) -> bool {
        !name.is_empty() && self.targets.iter().any(|t| t == name)
    }

    /// Whether `name` can be listed: typed, not listed, and the list not full.
    pub fn can_list(&self, name: &str) -> bool {
        !name.is_empty() && !self.listed(name) && self.targets.len() < TARGETS
    }

    /// Lists `name`, or takes it off when it is listed.
    pub fn toggle_listed(&mut self, name: &str) {
        if self.listed(name) {
            self.targets.retain(|t| t != name);
        } else if self.can_list(name) {
            self.targets.push(name.to_owned());
        }
    }
}

/// The channel before (`back`) or after `current` in the bar's order, wrapping.
pub(crate) fn next_channel(current: &str, back: bool) -> &'static str {
    let at = CHANNELS.iter().position(|c| *c == current).unwrap_or(0);
    let n = CHANNELS.len();
    CHANNELS[if back { (at + n - 1) % n } else { (at + 1) % n }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn down_steps_back_through_lines_and_up_forward() {
        let mut bar = ChatBar::default();
        for line in ["one", "two", "three"] {
            bar.said(line, "talk", "");
        }
        assert_eq!(bar.older(""), Some("three".into()));
        assert_eq!(bar.older("three"), Some("two".into()));
        assert_eq!(bar.older("two"), Some("one".into()));
        assert_eq!(bar.older("one"), Some("one".into()));
        assert_eq!(bar.newer(), Some("two".into()));
        assert_eq!(bar.newer(), Some("three".into()));
        assert_eq!(bar.newer(), None);
    }

    #[test]
    fn a_line_said_again_moves_to_the_front() {
        let mut bar = ChatBar::default();
        for line in ["a", "b", "a"] {
            bar.said(line, "talk", "");
        }
        assert_eq!(bar.older(""), Some("a".into()));
        assert_eq!(bar.older("a"), Some("b".into()));
    }

    #[test]
    fn recent_pairs_cycle_and_keep_names_only_where_they_count() {
        let mut bar = ChatBar::default();
        bar.said("x", "whisper", "Ann");
        bar.said("y", "team", "Ann");
        assert_eq!(
            bar.cycle_recent(false),
            Some(("team".into(), String::new()))
        );
        assert_eq!(
            bar.cycle_recent(false),
            Some(("whisper".into(), "Ann".into()))
        );
        assert_eq!(bar.cycle_recent(true), Some(("team".into(), String::new())));
    }

    #[test]
    fn lines_close_together_are_held_back() {
        let mut bar = ChatBar::default();
        let now = Instant::now();
        assert!(!bar.too_soon(now));
        assert!(bar.too_soon(now + Duration::from_millis(300)));
        assert!(!bar.too_soon(now + Duration::from_millis(600)));
    }

    #[test]
    fn channels_wrap_both_ways() {
        assert_eq!(next_channel("talk", true), "service");
        assert_eq!(next_channel("service", false), "talk");
        assert_eq!(next_channel("whisper", false), "team");
    }

    #[test]
    fn names_are_listed_and_taken_off() {
        let mut bar = ChatBar::default();
        assert!(!bar.can_list(""));
        bar.toggle_listed("Ann");
        assert!(bar.listed("Ann") && !bar.can_list("Ann"));
        bar.toggle_listed("Ann");
        assert!(!bar.listed("Ann"));
    }
}
