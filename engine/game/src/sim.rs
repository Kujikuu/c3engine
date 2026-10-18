//! The simulated world of one map: its players, monsters and ground items, run in fixed
//! ticks. Hosts give commands (walk, attack, buy…), advance time, and read state and events.

use crate::entity::{GroundItem, Monster, Player};
use crate::event::Event;
use crate::index::CellIndex;
use crate::rng::Rng;
use crate::rules::Rules;
use crate::{EntityId, distance};
use c3_scene::{CellPos, Grid, Npc, Spawn};
use std::collections::BTreeMap;

/// Game time per tick.
pub const TICK_MS: u64 = 50;

/// A spawn area and how it is doing.
pub(crate) struct SpawnArea {
    pub spawn: Spawn,
    pub alive: u32,
    pub next_wave_ms: u64,
}

pub struct Sim {
    pub(crate) grid: Grid,
    pub(crate) npcs: Vec<Npc>,
    pub(crate) spawns: Vec<SpawnArea>,
    pub(crate) rules: Box<dyn Rules>,
    pub(crate) rng: Rng,
    pub(crate) now_ms: u64,
    /// Time advanced but not yet ticked.
    pending_ms: u64,
    next_entity: u32,
    next_item: u32,
    pub(crate) players: BTreeMap<EntityId, Player>,
    pub(crate) monsters: BTreeMap<EntityId, Monster>,
    pub(crate) items: BTreeMap<EntityId, GroundItem>,
    pub(crate) index: CellIndex,
    pub(crate) events: Vec<Event>,
}

impl Sim {
    /// A world on `grid` with `npcs`, and monsters born in `spawns` (filled at once).
    pub fn new(
        grid: Grid,
        npcs: Vec<Npc>,
        spawns: Vec<Spawn>,
        rules: Box<dyn Rules>,
        seed: u64,
    ) -> Self {
        let mut sim = Self {
            grid,
            npcs,
            spawns: spawns
                .into_iter()
                .map(|spawn| SpawnArea {
                    spawn,
                    alive: 0,
                    next_wave_ms: 0,
                })
                .collect(),
            rules,
            rng: Rng::new(seed),
            now_ms: 0,
            pending_ms: 0,
            next_entity: 1,
            next_item: 1,
            players: BTreeMap::new(),
            monsters: BTreeMap::new(),
            items: BTreeMap::new(),
            index: CellIndex::default(),
            events: Vec::new(),
        };
        sim.fill_spawns();
        sim
    }

    pub fn rules(&self) -> &dyn Rules {
        &*self.rules
    }

    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    /// Game time in milliseconds.
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    /// Moves game time on by `ms`, running every tick that fits.
    pub fn advance(&mut self, ms: u64) {
        self.pending_ms += ms;
        while self.pending_ms >= TICK_MS {
            self.pending_ms -= TICK_MS;
            self.now_ms += TICK_MS;
            self.tick();
        }
    }

    fn tick(&mut self) {
        let ids: Vec<EntityId> = self.players.keys().copied().collect();
        for id in ids {
            self.tick_player(id);
        }
        let ids: Vec<EntityId> = self.monsters.keys().copied().collect();
        for id in ids {
            self.tick_monster(id);
        }
        self.tick_spawns();
        self.expire_items();
    }

    /// What happened since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub(crate) fn new_entity(&mut self) -> EntityId {
        let id = EntityId(self.next_entity);
        self.next_entity += 1;
        id
    }

    pub(crate) fn new_item_id(&mut self) -> u32 {
        let id = self.next_item;
        self.next_item += 1;
        id
    }

    pub(crate) fn walkable(&self, cell: CellPos) -> bool {
        self.grid.get(cell).is_some_and(|c| !c.is_blocked())
    }

    // ---- reading ----

    pub fn player(&self, id: EntityId) -> Option<&Player> {
        self.players.get(&id)
    }

    pub fn monster(&self, id: EntityId) -> Option<&Monster> {
        self.monsters.get(&id)
    }

    pub fn ground_item(&self, id: EntityId) -> Option<&GroundItem> {
        self.items.get(&id)
    }

    pub fn players(&self) -> impl Iterator<Item = &Player> {
        self.players.values()
    }

    pub fn monsters(&self) -> impl Iterator<Item = &Monster> {
        self.monsters.values()
    }

    pub fn ground_items(&self) -> impl Iterator<Item = &GroundItem> {
        self.items.values()
    }

    pub fn npcs(&self) -> &[Npc] {
        &self.npcs
    }

    /// Where an entity stands.
    pub fn cell_of(&self, id: EntityId) -> Option<CellPos> {
        self.players
            .get(&id)
            .map(|p| p.cell)
            .or_else(|| self.monsters.get(&id).map(|m| m.cell))
            .or_else(|| self.items.get(&id).map(|i| i.cell))
    }

    /// Living monsters and items within `radius` of `cell`.
    pub fn near(&self, cell: CellPos, radius: i32) -> impl Iterator<Item = EntityId> + '_ {
        self.index.near(cell, radius).map(|(id, _)| id)
    }

    pub(crate) fn npc_in_reach(&self, player: CellPos, npc: u32) -> bool {
        self.npcs
            .iter()
            .any(|n| n.id == npc && distance(n.cell, player) <= crate::NPC_REACH)
    }

    pub(crate) fn message(&mut self, player: EntityId, text: impl Into<String>) {
        self.events.push(Event::Message {
            player,
            text: text.into(),
        });
    }
}
