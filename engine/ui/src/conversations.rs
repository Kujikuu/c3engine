//! Private conversations (the 5065 client's Chat.dll windows): a whisper to the hero, or a
//! team line from another, is kept with whoever said it (the team's together), its tray icon
//! flashing until its window is opened; the window shows one conversation and speaks to it.

/// Lines a conversation keeps.
const MOST_LINES: usize = 200;

/// One conversation: whom it is with (a player, or the team), that player's face, its lines
/// (`name: words`), and whether it has lines not yet seen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Conversation {
    pub name: String,
    pub team: bool,
    pub face: u32,
    pub lines: Vec<String>,
    pub flashing: bool,
}

/// The hero's conversations, the newest first; the one its window shows; and whether the
/// windows and tray are hidden (TQ's conceal button).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Conversations {
    pub list: Vec<Conversation>,
    pub open: Option<usize>,
    pub hidden: bool,
}

impl Conversations {
    /// The conversation its window shows.
    pub fn current(&self) -> Option<&Conversation> {
        self.list.get(self.open?)
    }

    /// `line` heard from `name` (or the team): kept with that conversation, a new one coming
    /// first; it flashes unless its window shows it.
    pub fn heard(&mut self, name: &str, team: bool, face: u32, line: String) {
        let found = (self.list.iter()).position(|c| c.team == team && (team || c.name == name));
        let at = match found {
            Some(at) => at,
            None => {
                let name = if team { String::new() } else { name.to_owned() };
                self.list.insert(
                    0,
                    Conversation {
                        name,
                        team,
                        ..Default::default()
                    },
                );
                self.open = self.open.map(|o| o + 1);
                0
            }
        };
        let open = self.open == Some(at) && !self.hidden;
        let c = &mut self.list[at];
        if !team && face != 0 {
            c.face = face;
        }
        c.lines.push(line);
        let over = c.lines.len().saturating_sub(MOST_LINES);
        c.lines.drain(..over);
        c.flashing |= !open;
    }

    /// `line` said in the window's conversation.
    pub fn said(&mut self, line: String) {
        if let Some(c) = self.open.and_then(|o| self.list.get_mut(o)) {
            c.lines.push(line);
        }
    }

    /// Shows conversation `at` in the window; its icon stops flashing.
    pub fn show(&mut self, at: usize) {
        if let Some(c) = self.list.get_mut(at) {
            c.flashing = false;
            self.open = Some(at);
            self.hidden = false;
        }
    }

    /// Closes the window's conversation (its icon goes).
    pub fn close(&mut self) {
        if let Some(at) = self.open.take() {
            self.list.remove(at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_gather_by_speaker_and_flash_until_shown() {
        let mut c = Conversations::default();
        c.heard("Ann", false, 3, "Ann: hi".into());
        c.heard("Bob", false, 0, "Bob: yo".into());
        c.heard("Kim", true, 0, "Kim: go".into());
        c.heard("Ann", false, 0, "Ann: there?".into());
        assert_eq!(
            c.list.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["", "Bob", "Ann"]
        );
        assert_eq!((c.list[2].lines.len(), c.list[2].face), (2, 3));
        c.show(2);
        c.heard("Ann", false, 0, "Ann: well".into());
        assert!(!c.list[2].flashing, "seen while shown");
        c.said("Me: ok".into());
        assert_eq!(c.current().unwrap().lines.last().unwrap(), "Me: ok");
        c.close();
        assert_eq!((c.list.len(), c.open), (2, None));
    }
}
