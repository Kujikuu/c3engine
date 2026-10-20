//! What every map shares and the server keeps: guilds, the guilds owning maps, and world
//! variables. Each map holds a copy and records its changes as [`WorldChange`]s; the host
//! applies them to its own copy, keeps it, and hands every map the result.

use crate::sim::Sim;
use c3_script::{GuildView, WorldChange};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Most allies and enemies a guild has.
pub const GUILD_RELATIONS: usize = 5;

/// A guild: its members by name with their rank, its fund, and its friends and foes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Guild {
    pub name: String,
    pub leader: String,
    pub fund: i64,
    /// The guild it belongs to (a branch's main guild); 0: none.
    #[serde(default)]
    pub master: u32,
    #[serde(default)]
    pub allies: Vec<u32>,
    #[serde(default)]
    pub enemies: Vec<u32>,
    /// Members by name, and their rank (the game's numbers).
    pub members: BTreeMap<String, i64>,
}

/// The state all maps share.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldState {
    #[serde(default)]
    pub guilds: BTreeMap<u32, Guild>,
    /// The guild owning each game map that has one.
    #[serde(default)]
    pub map_owners: BTreeMap<u32, u32>,
    #[serde(default)]
    pub flags: BTreeMap<String, i64>,
    /// The wanted list, by entry id.
    #[serde(default)]
    pub wanted: BTreeMap<u32, Wanted>,
    /// Bounties on killers, by id.
    #[serde(default)]
    pub bounties: BTreeMap<u32, Bounty>,
    /// The police wanted list: names and levels, the highest level first.
    #[serde(default)]
    pub police: Vec<(String, i64)>,
    /// The players online on every map, by name: the game map each is on.
    #[serde(default, skip_serializing)]
    pub online: BTreeMap<String, u32>,
    /// The maps' names by game id (the host's list).
    #[serde(default, skip_serializing)]
    pub map_names: BTreeMap<u32, String>,
    /// The guild and PK points of each name on the police list.
    #[serde(default)]
    pub police_info: BTreeMap<String, (String, i64)>,
    /// Maps made while playing (players' homes), by their new id.
    #[serde(default)]
    pub map_copies: BTreeMap<u32, c3_script::MapCopyView>,
}

/// A bounty on a killer: the one killed, the killer, the reward's kind and amount.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bounty {
    pub target: String,
    pub hunter: String,
    pub kind: i64,
    pub bonus: i64,
}

/// A wanted list entry: who is wanted, who pays, the bounty.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wanted {
    pub target: String,
    pub payer: String,
    pub bounty: i64,
}

impl WorldState {
    /// Makes `change`; a change that no longer fits (a guild gone) does nothing.
    pub fn apply(&mut self, change: &WorldChange) {
        match change {
            WorldChange::Flag { name, value } => {
                self.flags.insert(name.clone(), *value);
            }
            WorldChange::CreateGuild {
                id,
                name,
                leader,
                rank,
                fund,
                master,
            } => {
                let Ok(id) = u32::try_from(*id) else {
                    return;
                };
                self.leave_guilds(leader);
                self.guilds.insert(
                    id,
                    Guild {
                        name: name.clone(),
                        leader: leader.clone(),
                        fund: *fund,
                        master: u32::try_from(*master).unwrap_or(0),
                        members: BTreeMap::from([(leader.clone(), *rank)]),
                        ..Guild::default()
                    },
                );
            }
            WorldChange::DestroyGuild(id) => {
                let Ok(id) = u32::try_from(*id) else {
                    return;
                };
                self.guilds.remove(&id);
                self.map_owners.retain(|_, g| *g != id);
                for g in self.guilds.values_mut() {
                    g.allies.retain(|a| *a != id);
                    g.enemies.retain(|e| *e != id);
                }
            }
            WorldChange::GuildFund { guild, amount } => {
                if let Some(g) = self.guild_mut(*guild) {
                    g.fund = g.fund.saturating_add(*amount).max(0);
                }
            }
            WorldChange::Member { guild, name, rank } => {
                if *rank > 0 {
                    self.leave_guilds(name);
                }
                let Some(g) = self.guild_mut(*guild) else {
                    return;
                };
                if *rank > 0 {
                    g.members.insert(name.clone(), *rank);
                } else {
                    g.members.remove(name);
                }
            }
            WorldChange::Relation {
                guild,
                other,
                ally,
                on,
            } => {
                let (Ok(a), Ok(b)) = (u32::try_from(*guild), u32::try_from(*other)) else {
                    return;
                };
                if a == b || !self.guilds.contains_key(&a) || !self.guilds.contains_key(&b) {
                    return;
                }
                // Alliances are both ways; enmity is one guild's.
                let sides: &[(u32, u32)] = if *ally { &[(a, b), (b, a)] } else { &[(a, b)] };
                for &(from, to) in sides {
                    let g = self.guilds.get_mut(&from).expect("checked above");
                    let list = if *ally { &mut g.allies } else { &mut g.enemies };
                    list.retain(|x| *x != to);
                    if *on && list.len() < GUILD_RELATIONS {
                        list.push(to);
                    }
                }
            }
            WorldChange::Wanted {
                id,
                target,
                payer,
                bounty,
            } => {
                let Ok(id) = u32::try_from(*id) else {
                    return;
                };
                if *bounty <= 0 {
                    self.wanted.remove(&id);
                } else {
                    let entry = Wanted {
                        target: target.clone(),
                        payer: payer.clone(),
                        bounty: *bounty,
                    };
                    self.wanted.insert(id, entry);
                }
            }
            WorldChange::Online { name, map } => match u32::try_from(*map) {
                Ok(map) if map != 0 => {
                    self.online.insert(name.clone(), map);
                }
                _ => {
                    self.online.remove(name);
                }
            },
            WorldChange::MapCopy { id, copy } => {
                if let Ok(id) = u32::try_from(*id) {
                    self.map_copies.insert(id, copy.clone());
                }
            }
            WorldChange::GuildLeader { guild, name } => {
                let Some(g) = self.guild_mut(*guild) else {
                    return;
                };
                // The new leader takes the leader's rank; the old one, the new one's.
                let rank = g.members.get(&g.leader).copied().unwrap_or(0);
                let was = g.members.get(name).copied();
                let old = std::mem::replace(&mut g.leader, name.clone());
                if old != *name
                    && let Some(was) = was
                {
                    g.members.insert(old, was);
                }
                g.members.insert(name.clone(), rank);
            }
            WorldChange::Police {
                name,
                level,
                listed,
                guild,
                pk,
            } => {
                self.police.retain(|(n, _)| n != name);
                self.police_info.remove(name);
                if *listed {
                    self.police_info.insert(name.clone(), (guild.clone(), *pk));
                }
                if *listed {
                    // Before the first of a lower level (TQ's sorted insert).
                    let at = (self.police.iter())
                        .position(|(_, l)| l < level)
                        .unwrap_or(self.police.len());
                    self.police.insert(at, (name.clone(), *level));
                }
            }
            WorldChange::Bounty { id, view } => {
                let Ok(id) = u32::try_from(*id) else {
                    return;
                };
                match view {
                    Some(v) => {
                        let bounty = Bounty {
                            target: v.target.clone(),
                            hunter: v.hunter.clone(),
                            kind: v.kind,
                            bonus: v.bonus,
                        };
                        self.bounties.insert(id, bounty);
                    }
                    None => {
                        self.bounties.remove(&id);
                    }
                }
            }
            WorldChange::MapOwner { map, guild } => {
                let Ok(map) = u32::try_from(*map) else {
                    return;
                };
                match u32::try_from(*guild) {
                    Ok(g) if g > 0 => {
                        self.map_owners.insert(map, g);
                    }
                    _ => {
                        self.map_owners.remove(&map);
                    }
                }
            }
        }
    }

    /// The guild player `name` is in, and its rank there.
    pub fn guild_of(&self, name: &str) -> Option<(u32, i64)> {
        (self.guilds.iter()).find_map(|(id, g)| g.members.get(name).map(|r| (*id, *r)))
    }

    /// The guilds as scripts see them.
    /// The bounties on killers as scripts see them.
    pub fn bounty_views(&self) -> BTreeMap<i64, c3_script::BountyView> {
        (self.bounties.iter())
            .map(|(id, b)| {
                let view = c3_script::BountyView {
                    target: b.target.clone(),
                    hunter: b.hunter.clone(),
                    kind: b.kind,
                    bonus: b.bonus,
                };
                (i64::from(*id), view)
            })
            .collect()
    }

    /// The wanted list as scripts see it.
    pub fn wanted_views(&self) -> BTreeMap<i64, c3_script::WantedView> {
        (self.wanted.iter())
            .map(|(id, w)| {
                let view = c3_script::WantedView {
                    target: w.target.clone(),
                    payer: w.payer.clone(),
                    bounty: w.bounty,
                };
                (i64::from(*id), view)
            })
            .collect()
    }

    pub fn guild_views(&self) -> BTreeMap<i64, GuildView> {
        (self.guilds.iter())
            .map(|(id, g)| {
                let view = GuildView {
                    name: g.name.clone(),
                    leader: g.leader.clone(),
                    fund: g.fund,
                    master: g.master.into(),
                    allies: g.allies.iter().map(|&a| a.into()).collect(),
                    enemies: g.enemies.iter().map(|&e| e.into()).collect(),
                    members: g.members.clone(),
                };
                (i64::from(*id), view)
            })
            .collect()
    }

    fn guild_mut(&mut self, id: i64) -> Option<&mut Guild> {
        self.guilds.get_mut(&u32::try_from(id).ok()?)
    }

    /// Takes player `name` out of every guild.
    fn leave_guilds(&mut self, name: &str) {
        for g in self.guilds.values_mut() {
            g.members.remove(name);
        }
    }
}

impl Sim {
    /// The shared state as this map holds it.
    pub fn world_state(&self) -> &WorldState {
        &self.world
    }

    /// The host's shared state; this map's changes not yet taken stay made.
    pub fn set_world_state(&mut self, world: WorldState) {
        self.world = world;
        for change in &self.world_changes {
            self.world.apply(change);
        }
        self.world_flags = self.world.flags.clone();
    }

    /// The changes this map made to the shared state since the last call, for the host.
    pub fn take_world_changes(&mut self) -> Vec<WorldChange> {
        std::mem::take(&mut self.world_changes)
    }

    /// Makes `change` here and keeps it for the host.
    pub(crate) fn change_world(&mut self, change: WorldChange) {
        self.world.apply(&change);
        self.world_flags = self.world.flags.clone();
        self.world_changes.push(change);
    }
}
