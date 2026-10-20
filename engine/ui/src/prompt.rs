//! The game UI's yes/no prompt (TQ's CDlgConfirm): one question at a time, of a kind the
//! game UI words (`prompt.kind.<kind>`), naming who asked; Yes sends its input or runs its
//! action, No drops it. Another player's trade or team ask lapses after 10 seconds.

use c3_game::Input;
use std::time::{Duration, Instant};

/// How long a trade or team ask waits.
const LAPSE: Duration = Duration::from_secs(10);

/// What Yes does.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Yes {
    /// Send the game this input (take up another player's ask).
    Game(Input),
    /// Run these UI actions (`;` between them; leaving a guild once asked).
    Actions(String),
}

#[derive(Debug, Clone)]
pub(crate) struct Prompt {
    /// What is asked: `trade`, `team_invite`, `team_apply`, `guild_invite`, `guild_apply`,
    /// `marriage`, or an action's own kind (`ask:<kind>:<action>`).
    pub kind: String,
    /// Who asked (empty for the player's own actions).
    pub name: String,
    pub yes: Yes,
    until: Option<Instant>,
}

impl Prompt {
    /// Another player's ask of `kind`, which Yes takes up with `input`.
    pub fn asked(kind: &str, name: &str, input: Input, now: Instant) -> Self {
        let lapses = matches!(kind, "trade" | "team_invite" | "team_apply");
        Self {
            kind: kind.to_owned(),
            name: name.to_owned(),
            yes: Yes::Game(input),
            until: lapses.then(|| now + LAPSE),
        }
    }

    /// The player's own action `actions`, asked first as `kind` (`ask:<kind>:<actions>`).
    pub fn confirm(spec: &str) -> Option<Self> {
        let (kind, actions) = spec.split_once(':')?;
        Some(Self {
            kind: kind.to_owned(),
            name: String::new(),
            yes: Yes::Actions(actions.to_owned()),
            until: None,
        })
    }

    /// Whether it has waited too long.
    pub fn lapsed(&self, now: Instant) -> bool {
        self.until.is_some_and(|t| now >= t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trade_and_team_asks_lapse_but_guild_asks_wait() {
        let now = Instant::now();
        let trade = Prompt::asked("trade", "Ann", Input::Revive, now);
        let guild = Prompt::asked("guild_invite", "Ann", Input::Revive, now);
        assert!(!trade.lapsed(now + Duration::from_secs(9)));
        assert!(trade.lapsed(now + Duration::from_secs(10)));
        assert!(!guild.lapsed(now + Duration::from_secs(60)));
    }

    #[test]
    fn an_action_is_asked_as_its_kind() {
        let p = Prompt::confirm("guild_leave:guild_leave;toggle:133").unwrap();
        assert_eq!(p.kind, "guild_leave");
        assert_eq!(p.yes, Yes::Actions("guild_leave;toggle:133".into()));
        assert!(Prompt::confirm("nothing").is_none());
    }
}
