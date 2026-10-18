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
    #[serde(default)]
    pub flags: BTreeMap<String, i64>,
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
}

impl From<u32> for SavedItem {
    fn from(kind: u32) -> Self {
        Self {
            kind,
            count: 1,
            durability: None,
            quality: 0,
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
            } => Self {
                kind,
                count: count.max(1),
                durability,
                quality,
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
            flags: BTreeMap::new(),
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
        let bag: Vec<Item> = save.bag.iter().map(|&s| self.saved_item(s)).collect();
        let worn: Vec<Item> = save.equipment.iter().map(|&s| self.saved_item(s)).collect();
        let p = self.players.get_mut(&id).expect("just added");
        p.exp = save.exp;
        p.money = save.money;
        p.wallet = save.wallet.clone();
        p.bag = bag;
        p.equipment = worn;
        p.flags = save.flags.clone();
        self.refresh_stats(id);
        let p = self.players.get_mut(&id).expect("just added");
        // A save without life (a new character) starts full.
        p.life = if save.life == 0 {
            p.stats.max_life
        } else {
            save.life.min(p.stats.max_life)
        };
        p.mana = save.mana.min(p.stats.max_mana);
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
            flags: p.flags.clone(),
        })
    }

    /// Takes player `id` out of the world (logged out).
    pub fn remove_player(&mut self, id: EntityId) {
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
        }
    }
}
