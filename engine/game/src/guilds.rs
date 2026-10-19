//! What players do about guilds: a guild's leader invites players in, sends them out and
//! names its allies and enemies; members leave and give money to the guild's fund. Each
//! change goes to the shared world state ([`crate::WorldState`]), so every map sees it.

use crate::EntityId;
use crate::event::Event;
use crate::sim::Sim;
use c3_script::WorldChange;
use serde::{Deserialize, Serialize};

/// What a player does about guilds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuildAction {
    /// The leader asks a player (here) into its guild; the player joins the inviter's guild.
    Invite(EntityId),
    Join(EntityId),
    Leave,
    /// The leader sends a member (here) out.
    Kick(EntityId),
    /// Money from the player into the guild's fund.
    Donate(u64),
    /// The leader makes guild `0` an ally or an enemy (`true`), or no longer (`false`).
    Ally(u32, bool),
    Enemy(u32, bool),
}

impl Sim {
    /// Player `id` acts on guilds; told how it went.
    pub fn guild(&mut self, id: EntityId, action: GuildAction) {
        let text = self.guild_action(id, action).unwrap_or_else(|why| why);
        self.events.push(Event::Notice { player: id, text });
    }

    fn guild_action(&mut self, id: EntityId, action: GuildAction) -> Result<String, String> {
        let me = self.players.get(&id).ok_or("No such player")?;
        let my_name = me.name.clone();
        let mine = self.world.guild_of(&my_name).map(|(g, _)| g);
        let leads = mine.filter(|g| self.world.guilds[g].leader == my_name);
        let name_of = |s: &Sim, e: EntityId| s.players.get(&e).map(|p| p.name.clone());
        match action {
            GuildAction::Invite(other) => {
                let guild = leads.ok_or("Only a guild's leader invites.")?;
                let name = name_of(self, other).ok_or("They aren't here.")?;
                if self.world.guild_of(&name).is_some() {
                    return Err(format!("{name} is in a guild already."));
                }
                self.guild_invites.insert(other, guild);
                self.events.push(Event::GuildInvited {
                    player: other,
                    inviter: id,
                });
                Ok(format!("You invited {name} into the guild."))
            }
            GuildAction::Join(inviter) => {
                let inviter = name_of(self, inviter).ok_or("They aren't here.")?;
                let guild = (self.world.guild_of(&inviter))
                    .map(|(g, _)| g)
                    .filter(|g| self.guild_invites.get(&id) == Some(g))
                    .ok_or("They haven't invited you.")?;
                if mine.is_some() {
                    return Err("You are in a guild already.".into());
                }
                self.guild_invites.remove(&id);
                self.change_world(WorldChange::Member {
                    guild: guild.into(),
                    name: my_name,
                    rank: self.settings.guild_member_rank.into(),
                });
                Ok(format!("You joined {}.", self.world.guilds[&guild].name))
            }
            GuildAction::Leave => {
                let guild = mine.ok_or("You are in no guild.")?;
                if leads.is_some() {
                    return Err("A guild's leader can't leave it.".into());
                }
                self.change_world(WorldChange::Member {
                    guild: guild.into(),
                    name: my_name,
                    rank: 0,
                });
                Ok("You left the guild.".into())
            }
            GuildAction::Kick(other) => {
                let guild = leads.ok_or("Only a guild's leader sends members out.")?;
                let name = name_of(self, other).ok_or("They aren't here.")?;
                if other == id || self.world.guild_of(&name).map(|(g, _)| g) != Some(guild) {
                    return Err(format!("{name} isn't in your guild."));
                }
                self.change_world(WorldChange::Member {
                    guild: guild.into(),
                    name: name.clone(),
                    rank: 0,
                });
                self.events.push(Event::Notice {
                    player: other,
                    text: "You were sent from the guild.".into(),
                });
                Ok(format!("{name} was sent from the guild."))
            }
            GuildAction::Donate(amount) => {
                let guild = mine.ok_or("You are in no guild.")?;
                let p = self.players.get_mut(&id).expect("found above");
                if amount == 0 || amount > p.money {
                    return Err("You don't have that much.".into());
                }
                p.money -= amount;
                self.change_world(WorldChange::GuildFund {
                    guild: guild.into(),
                    amount: i64::try_from(amount).unwrap_or(i64::MAX),
                });
                Ok(format!("You gave {amount} to the guild."))
            }
            GuildAction::Ally(other, on) | GuildAction::Enemy(other, on) => {
                let guild = leads.ok_or("Only a guild's leader does that.")?;
                if other == guild || !self.world.guilds.contains_key(&other) {
                    return Err("There is no such guild.".into());
                }
                let ally = matches!(action, GuildAction::Ally(..));
                self.change_world(WorldChange::Relation {
                    guild: guild.into(),
                    other: other.into(),
                    ally,
                    on,
                });
                let them = &self.world.guilds[&other].name;
                Ok(match (ally, on) {
                    (true, true) => format!("{them} is your ally."),
                    (true, false) => format!("{them} is no longer your ally."),
                    (false, true) => format!("{them} is your enemy."),
                    (false, false) => format!("{them} is no longer your enemy."),
                })
            }
        }
    }
}
