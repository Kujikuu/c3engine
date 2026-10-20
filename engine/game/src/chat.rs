//! Chat on channels: what is said near the player (`Local`), to one player anywhere
//! (`Whisper`), to its team or its guild. Lines for players on other maps wait as
//! [`Sim::take_chat_out`] for the host, which gives them to the map they are on
//! ([`Sim::hear`]).

use crate::sim::Sim;
use crate::{EntityId, Event, MAX_CHAT};
use serde::{Deserialize, Serialize};

/// Who a chat line is for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatChannel {
    /// Everyone near the speaker.
    #[default]
    Local,
    /// One player, named, on any map.
    Whisper,
    /// The speaker's team.
    Team,
    /// The speaker's guild.
    Guild,
    /// The speaker's friends, on any map.
    Friend,
    /// Words left for a friend who is offline, given when it next comes (TQ's leave words).
    LeaveWord,
    /// A post on message board `0` (the settings' `boards`), one per author.
    Board(u8),
}

/// How long a player waits between leaving words (TQ's 60 s, social spec §5).
const WORD_WAIT_MS: u64 = 60_000;

/// A chat line on its way to a player on another map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatLine {
    pub channel: ChatChannel,
    pub from: EntityId,
    pub name: String,
    pub text: String,
    /// Whom a whisper was said to (empty on other channels).
    pub hearer: String,
}

impl Sim {
    /// Player `id` says `text` on `channel` (to player `to` when whispering).
    pub fn chat(&mut self, id: EntityId, channel: ChatChannel, to: &str, text: &str) {
        let text: String = text.trim().chars().take(MAX_CHAT).collect();
        let Some(name) = self.players.get(&id).map(|p| p.name.clone()) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        if channel == ChatChannel::LeaveWord {
            return self.leave_word(id, &name, to.trim(), &text);
        }
        if let ChatChannel::Board(board) = channel {
            return self.post(id, board, &text);
        }
        let hearer = match channel {
            ChatChannel::Whisper => to.trim().to_owned(),
            _ => String::new(),
        };
        let line = ChatLine {
            channel,
            from: id,
            name: name.clone(),
            text,
            hearer,
        };
        let hearers: Vec<String> = match channel {
            ChatChannel::Local => {
                self.events.push(Event::Chat {
                    player: id,
                    name,
                    text: line.text,
                    channel,
                    to: None,
                    hearer: String::new(),
                });
                return;
            }
            ChatChannel::Whisper => {
                if !self.world.online.contains_key(to.trim()) && self.player_by_name(to).is_none() {
                    self.message(id, format!("{} is not online.", to.trim()));
                    return;
                }
                vec![to.trim().to_owned()]
            }
            ChatChannel::Team => {
                let members = self.team_members(id);
                if members.len() < 2 {
                    self.message(id, "You are in no team.");
                    return;
                }
                (members.into_iter())
                    .filter(|&m| m != id)
                    .filter_map(|m| self.name_of_any(m))
                    .collect()
            }
            ChatChannel::Friend => {
                let friends: Vec<String> = (self.world.friends_of(&name).into_iter())
                    .filter(|f| {
                        self.world.online.contains_key(f) || self.player_by_name(f).is_some()
                    })
                    .collect();
                if friends.is_empty() {
                    self.message(id, "None of your friends is online.");
                    return;
                }
                friends
            }
            ChatChannel::LeaveWord | ChatChannel::Board(_) => return,
            ChatChannel::Guild => {
                let Some((guild, _)) = self.world.guild_of(&name) else {
                    self.message(id, "You are in no guild.");
                    return;
                };
                let g = &self.world.guilds[&guild];
                if g.fund < self.settings.guild_chat_min_fund {
                    self.message(id, "Your guild's fund is too low to talk on its channel.");
                    return;
                }
                (g.members.keys())
                    .filter(|m| **m != name)
                    .filter(|m| {
                        self.world.online.contains_key(*m) || self.player_by_name(m).is_some()
                    })
                    .cloned()
                    .collect()
            }
        };
        // The speaker sees its own line on its channel.
        self.deliver_chat(id, &line);
        for hearer in hearers {
            match self.player_by_name(&hearer) {
                Some(here) => self.deliver_chat(here, &line),
                None => self.chat_out.push((hearer, line.clone())),
            }
        }
    }

    /// Player `id` (`name`) leaves `text` for friend `to`, who must be offline; it waits on
    /// the world state for it (social spec §5: one a minute).
    fn leave_word(&mut self, id: EntityId, name: &str, to: &str, text: &str) {
        let online = self.world.online.contains_key(to) || self.player_by_name(to).is_some();
        let known = self.world.are_friends(name, to) || self.world.are_partners(name, to);
        if online || !known {
            return;
        }
        let now = self.now_ms;
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        if p.word_left_ms.is_some_and(|t| now < t + WORD_WAIT_MS) {
            let text = "You`ve left too many messages. Please try again later.";
            return self.message(id, text);
        }
        p.word_left_ms = Some(now);
        let delivery = c3_script::Delivery::Word {
            from: name.to_owned(),
            text: text.to_owned(),
        };
        let to = to.to_owned();
        self.change_world(c3_script::WorldChange::Deliver { to, delivery });
        self.message(id, "The message has been sent.");
    }

    /// The world state's key of message board `board` for player `name`: its guild's own when
    /// it is the guild board (none outside a guild), else the board's number.
    pub fn board_key(&self, board: u8, name: &str) -> Option<String> {
        if usize::from(board) >= self.settings.boards.len() {
            return None;
        }
        match self.settings.guild_board == Some(board) {
            true => Some(format!("{board}:{}", self.world.guild_of(name)?.0)),
            false => Some(board.to_string()),
        }
    }

    /// Player `id` posts `text` on message board `board` (in place of its post there): from
    /// the board level up (social spec, MsgMessageBoard 1111).
    fn post(&mut self, id: EntityId, board: u8, text: &str) {
        let Some(p) = self.players.get(&id) else {
            return;
        };
        if p.level < self.settings.board_min_level {
            let level = self.settings.board_min_level;
            return self.message(id, format!("You must be level {level} to post."));
        }
        let author = p.name.clone();
        let Some(board) = self.board_key(board, &author) else {
            return self.message(id, "You are in no guild.");
        };
        let time = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
        let text = text.to_owned();
        self.change_world(c3_script::WorldChange::Post {
            board,
            author,
            text,
            time,
        });
    }

    /// Player `id` takes `author`'s post off message board `board`: its own, or anyone's for
    /// a game master (TQ's GM/PM delete).
    pub(crate) fn take_down(&mut self, id: EntityId, board: u8, author: &str) {
        let Some(name) = self.players.get(&id).map(|p| p.name.clone()) else {
            return;
        };
        let author = match self.settings.is_gm(&name) && !author.is_empty() {
            true => author.to_owned(),
            false => name.clone(),
        };
        if let Some(board) = self.board_key(board, &name) {
            let (text, time) = (String::new(), String::new());
            self.change_world(c3_script::WorldChange::Post {
                board,
                author,
                text,
                time,
            });
        }
    }

    /// Chat lines for players on other maps, by their names (the host carries them).
    pub fn take_chat_out(&mut self) -> Vec<(String, ChatLine)> {
        std::mem::take(&mut self.chat_out)
    }

    /// Player `name` (if on this map) hears `line` from another map.
    pub fn hear(&mut self, name: &str, line: &ChatLine) -> bool {
        let Some(id) = self.player_by_name(name) else {
            return false;
        };
        self.deliver_chat(id, line);
        true
    }

    fn deliver_chat(&mut self, to: EntityId, line: &ChatLine) {
        self.events.push(Event::Chat {
            player: line.from,
            name: line.name.clone(),
            text: line.text.clone(),
            channel: line.channel,
            to: Some(to),
            hearer: line.hearer.clone(),
        });
    }

    fn player_by_name(&self, name: &str) -> Option<EntityId> {
        let name = name.trim();
        (self.players.values())
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .map(|p| p.id)
    }

    /// The name of player `id`, here or a teammate on another map.
    fn name_of_any(&self, id: EntityId) -> Option<String> {
        (self.players.get(&id).map(|p| p.name.clone()))
            .or_else(|| self.elsewhere.get(&id).map(|m| m.name.clone()))
    }
}
