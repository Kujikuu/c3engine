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
}

/// A chat line on its way to a player on another map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatLine {
    pub channel: ChatChannel,
    pub from: EntityId,
    pub name: String,
    pub text: String,
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
        let line = ChatLine {
            channel,
            from: id,
            name: name.clone(),
            text,
        };
        let hearers: Vec<String> = match channel {
            ChatChannel::Local => {
                self.events.push(Event::Chat {
                    player: id,
                    name,
                    text: line.text,
                    channel,
                    to: None,
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
