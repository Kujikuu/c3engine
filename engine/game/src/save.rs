//! Players as they are kept between sessions: what a server stores for a character and
//! brings back when it logs in.

use crate::EntityId;
use crate::entity::Item;
use crate::event::Event;
use crate::sim::Sim;
use c3_scene::CellPos;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A character as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerSave {
    pub name: String,
    /// The map it is on, as the server names maps (empty: the first).
    #[serde(default)]
    pub map: String,
    pub cell: CellPos,
    pub level: u32,
    pub exp: u64,
    pub money: u64,
    pub life: u32,
    pub mana: u32,
    /// Items carried.
    pub bag: Vec<SavedItem>,
    /// Items worn.
    pub equipment: Vec<SavedItem>,
    /// Other currencies, by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub wallet: BTreeMap<String, u64>,
    /// Skills known.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<u32>,
    /// Players killed on PK maps.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub pk_kills: u32,
    #[serde(default)]
    pub flags: BTreeMap<String, i64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub texts: BTreeMap<String, String>,
    /// Body and hair chosen when it was made.
    #[serde(default, skip_serializing_if = "is_default_look")]
    pub look: crate::PlayerLook,
    /// The profession chosen when it was made (0: none).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub profession: u32,
    /// PK points.
    #[serde(default, skip_serializing_if = "is_zero_signed")]
    pub pk: i32,
    /// The game's numbers that differ from their default.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, i64>,
    /// Where it comes back to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<c3_scene::PortalTarget>,
    /// Items kept with storage NPCs (by NPC id), and the money kept.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub storage: BTreeMap<u32, Vec<SavedItem>>,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub saved_money: u64,
}

fn is_zero_u64(n: &u64) -> bool {
    *n == 0
}

fn is_default_look(look: &crate::PlayerLook) -> bool {
    *look == crate::PlayerLook::default()
}

/// An item as stored. Older saves name just the type (`410003`), which reads as one whole
/// item of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "SavedItemForm")]
pub struct SavedItem {
    pub kind: u32,
    pub count: u32,
    /// Durability left; `None`: a whole one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub durability: Option<u32>,
    pub quality: u8,
    /// The game's numbers for it.
    #[serde(skip_serializing_if = "no_attrs")]
    pub attrs: [i32; c3_core::settings::ITEM_ATTRIBUTES],
    /// The slot it is worn in when not its type's own.
    #[serde(skip_serializing_if = "is_zero_u8")]
    pub worn_at: u8,
}

fn is_zero_u8(n: &u8) -> bool {
    *n == 0
}

fn no_attrs(attrs: &[i32; c3_core::settings::ITEM_ATTRIBUTES]) -> bool {
    attrs.iter().all(|&a| a == 0)
}

impl From<u32> for SavedItem {
    fn from(kind: u32) -> Self {
        Self {
            kind,
            count: 1,
            durability: None,
            quality: 0,
            attrs: [0; c3_core::settings::ITEM_ATTRIBUTES],
            worn_at: 0,
        }
    }
}

/// The ways a saved item is written.
#[derive(Deserialize)]
#[serde(untagged)]
enum SavedItemForm {
    Kind(u32),
    Full {
        kind: u32,
        #[serde(default = "one")]
        count: u32,
        #[serde(default)]
        durability: Option<u32>,
        #[serde(default)]
        quality: u8,
        #[serde(default)]
        attrs: [i32; c3_core::settings::ITEM_ATTRIBUTES],
        #[serde(default)]
        worn_at: u8,
    },
}

fn one() -> u32 {
    1
}

impl From<SavedItemForm> for SavedItem {
    fn from(form: SavedItemForm) -> Self {
        match form {
            SavedItemForm::Kind(kind) => kind.into(),
            SavedItemForm::Full {
                kind,
                count,
                durability,
                quality,
                attrs,
                worn_at,
            } => Self {
                kind,
                count: count.max(1),
                durability,
                quality,
                attrs,
                worn_at,
            },
        }
    }
}

impl PlayerSave {
    /// A new character at `cell`.
    pub fn new(name: impl Into<String>, cell: CellPos) -> Self {
        Self {
            name: name.into(),
            map: String::new(),
            cell,
            level: 1,
            exp: 0,
            money: 0,
            life: 0,
            mana: 0,
            bag: Vec::new(),
            equipment: Vec::new(),
            wallet: BTreeMap::new(),
            skills: Vec::new(),
            pk_kills: 0,
            flags: BTreeMap::new(),
            texts: BTreeMap::new(),
            look: Default::default(),
            profession: 0,
            pk: 0,
            attributes: BTreeMap::new(),
            record: None,
            storage: BTreeMap::new(),
            saved_money: 0,
        }
    }
}

impl Sim {
    /// Brings a stored character into the world.
    pub fn add_saved_player(&mut self, save: &PlayerSave) -> EntityId {
        let id = self.new_entity();
        self.add_saved_player_as(save, id);
        id
    }

    /// Adds the character `save` as player `id`, an id from outside (a server keeps a
    /// player's id from map to map), which must not be one of this simulation's own.
    pub fn add_saved_player_as(&mut self, save: &PlayerSave, id: EntityId) {
        self.insert_player(id, save.name.clone(), save.cell, save.level.max(1));
        let map = self.map_ids.first().copied().map_or(0, i64::from);
        self.change_world(c3_script::WorldChange::Online {
            name: save.name.clone(),
            map,
            level: i64::from(save.level.max(1)),
        });
        let bag: Vec<Item> = save.bag.iter().map(|&s| self.saved_item(s)).collect();
        let worn: Vec<Item> = save.equipment.iter().map(|&s| self.saved_item(s)).collect();
        let storage: BTreeMap<u32, Vec<Item>> = (save.storage.iter())
            .map(|(&npc, items)| (npc, items.iter().map(|&s| self.saved_item(s)).collect()))
            .collect();
        let p = self.players.get_mut(&id).expect("just added");
        p.storage = storage;
        p.saved_money = save.saved_money;
        p.exp = save.exp;
        p.money = save.money;
        p.wallet = save.wallet.clone();
        p.skills = save.skills.clone();
        p.pk_kills = save.pk_kills;
        p.bag = bag;
        p.equipment = worn;
        p.flags = save.flags.clone();
        p.texts = save.texts.clone();
        p.look = save.look;
        p.profession = save.profession;
        p.pk = save.pk;
        p.attributes = save.attributes.clone();
        p.record = save.record;
        self.refresh_stats(id);
        let p = self.players.get_mut(&id).expect("just added");
        // A save without life (a new character) starts full.
        if save.life == 0 {
            p.life = p.stats.max_life;
            p.mana = p.stats.max_mana;
        } else {
            p.life = save.life.min(p.stats.max_life);
            p.mana = save.mana.min(p.stats.max_mana);
        }
        // What waited for it while it was away.
        self.deliver_waiting();
        let map = self.map_ids.first().copied().unwrap_or(0);
        self.script_event(id, "on_enter", vec![i64::from(map).into()]);
    }

    /// Player `id` as stored.
    pub fn save_player(&self, id: EntityId) -> Option<PlayerSave> {
        let p = self.players.get(&id)?;
        Some(PlayerSave {
            name: p.name.clone(),
            map: String::new(),
            cell: p.cell,
            level: p.level,
            exp: p.exp,
            money: p.money,
            // The dead come back with full life.
            life: if p.is_alive() { p.life } else { 0 },
            mana: p.mana,
            bag: p.bag.iter().map(|i| self.stored_item(i)).collect(),
            equipment: p.equipment.iter().map(|i| self.stored_item(i)).collect(),
            wallet: p.wallet.clone(),
            skills: p.skills.clone(),
            pk_kills: p.pk_kills,
            flags: p.flags.clone(),
            texts: p.texts.clone(),
            look: p.look,
            profession: p.profession,
            pk: p.pk,
            attributes: p.attributes.clone(),
            record: p.record,
            storage: (p.storage.iter())
                .filter(|(_, items)| !items.is_empty())
                .map(|(&npc, items)| (npc, items.iter().map(|i| self.stored_item(i)).collect()))
                .collect(),
            saved_money: p.saved_money,
        })
    }

    /// The team player `id` is in (its leader's id), to carry it to another map.
    pub fn team_of(&self, id: EntityId) -> Option<EntityId> {
        self.players.get(&id).and_then(|p| p.team)
    }

    /// Puts player `id` in `leader`'s team (one it was in on another map).
    pub fn set_team(&mut self, id: EntityId, leader: Option<EntityId>) {
        if let Some(p) = self.players.get_mut(&id) {
            p.team = leader;
        }
    }

    /// Takes player `id` out of the world (logged out).
    pub fn remove_player(&mut self, id: EntityId) {
        self.script_event(id, "on_logout", Vec::new());
        if let Some(name) = self.players.get(&id).map(|p| p.name.clone()) {
            self.change_world(c3_script::WorldChange::Online {
                name,
                map: 0,
                level: 0,
            });
        }
        // Logging out leaves the team.
        if let Some(leader) = self.players.get(&id).and_then(|p| p.team) {
            self.leave_team(id, leader);
        }
        self.travel_out(id);
    }

    /// Takes player `id` off this map to another (it keeps its team; see [`Self::team_of`]).
    pub fn travel_out(&mut self, id: EntityId) {
        self.team_invites.remove(&id);
        if let Some(p) = self.players.remove(&id) {
            self.index.remove(id, p.cell);
            self.events.push(Event::Vanished(id));
        }
    }

    /// A new whole item of `kind`.
    pub(crate) fn new_item(&mut self, kind: u32) -> Item {
        let id = self.new_item_id();
        Item::new(id, kind, self.rules.item(kind))
    }

    fn saved_item(&mut self, s: SavedItem) -> Item {
        let mut item = self.new_item(s.kind);
        item.count = s.count.max(1);
        item.quality = s.quality;
        item.attrs = s.attrs;
        item.worn_at = s.worn_at;
        if let Some(durability) = s.durability {
            item.durability = durability;
        }
        item
    }

    fn stored_item(&self, item: &Item) -> SavedItem {
        let has_durability = self.rules.item(item.kind).is_some_and(|t| t.durability > 0);
        SavedItem {
            kind: item.kind,
            count: item.count,
            durability: has_durability.then_some(item.durability),
            quality: item.quality,
            attrs: item.attrs,
            worn_at: item.worn_at,
        }
    }
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

fn is_zero_signed(n: &i32) -> bool {
    *n == 0
}
