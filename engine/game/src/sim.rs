//! The simulated world of one map: its players, monsters and ground items, run in fixed
//! ticks. Hosts give commands (walk, attack, buy…), advance time, and read state and events.

use crate::entity::{GroundItem, Monster, Player};
use crate::event::Event;
use crate::index::CellIndex;
use crate::rng::Rng;
use crate::rules::Rules;
use crate::{EntityId, GameplaySettings, distance};
use c3_scene::{CellPos, Grid, Npc, Spawn};
use std::collections::BTreeMap;

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
    pub(crate) settings: GameplaySettings,
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
    /// Runs NPC scripts; `None` when the game has none.
    pub(crate) scripts: Option<c3_script::ScriptHost>,
    /// Game map ids of this map (script teleports within it work).
    pub(crate) map_ids: Vec<u32>,
    /// Players on each game map the host runs (all, alive), for scripts; this map's own
    /// are counted here.
    pub(crate) map_population: std::collections::BTreeMap<u32, (u32, u32)>,
    /// Players may attack each other here.
    pub(crate) pk: bool,
    /// Fights between players here bring no crime and no PK points.
    pub(crate) pk_free: bool,
    /// Where players who die here come back (none: where they fell, or their return point).
    pub(crate) revive_point: Option<c3_scene::PortalTarget>,
    /// Portals that lead somewhere: their cells and destinations.
    pub(crate) portals: Vec<(CellPos, c3_scene::PortalTarget)>,
    pub(crate) quests: Vec<c3_script::Quest>,
    /// The map's regions (safe zones, markets…).
    pub(crate) regions: Vec<c3_scene::Region>,
    /// Script variables of the whole world (`world_flag`); unset reads as 0.
    pub(crate) world_flags: BTreeMap<String, i64>,
    /// What every map shares, as this map holds it, and its changes not yet taken.
    pub(crate) world: crate::WorldState,
    pub(crate) world_changes: Vec<c3_script::WorldChange>,
    /// The item a player is using while its `use_<type>` event runs.
    pub(crate) using: Option<(EntityId, u32)>,
    /// Where the monster being killed fell, while its `killed_<type>` event runs.
    pub(crate) dying_at: Option<CellPos>,
    /// Whether this map runs the world events (one map of a server does); and the world
    /// timers to come: (when, event).
    pub(crate) world_events: bool,
    /// Trades open, and who asked whom to trade.
    pub(crate) trades: Vec<crate::trades::Trade>,
    pub(crate) trade_asks: std::collections::BTreeMap<EntityId, EntityId>,
    /// The map's traps, and the one a player is setting off while its event runs.
    pub(crate) traps: Vec<c3_scene::Trap>,
    pub(crate) trap_now: Option<u32>,
    pub(crate) world_timers: Vec<(u64, String)>,
    /// Team invitations: the invited player → the inviting leader.
    pub(crate) team_invites: BTreeMap<EntityId, EntityId>,
    /// Proposals of marriage: who proposed to whom.
    pub(crate) courting: BTreeMap<EntityId, EntityId>,
    /// Who asked to join which leader's team, and the teams closed to newcomers.
    pub(crate) team_applies: BTreeMap<EntityId, EntityId>,
    /// Who was invited into which guild.
    pub(crate) guild_invites: BTreeMap<EntityId, u32>,
    pub(crate) closed_teams: std::collections::BTreeSet<EntityId>,
    /// Teams (by leader) whose members keep their money from each other, and those sharing
    /// items.
    pub(crate) teams_keeping_money: std::collections::BTreeSet<EntityId>,
    pub(crate) teams_sharing_items: std::collections::BTreeSet<EntityId>,
    /// Players on the host's other maps, for teams across maps.
    pub(crate) elsewhere: BTreeMap<EntityId, crate::teams::Mate>,
    /// What teams asked of players on other maps, for the host to carry.
    pub(crate) team_ops: Vec<(EntityId, crate::teams::TeamOp)>,
    /// NPCs that left for other maps, for the host to bring there.
    pub(crate) npc_moves: Vec<(u32, c3_scene::Npc)>,
    /// NPCs this map made while playing (their ids follow on).
    pub(crate) npcs_made: u32,
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
            settings: GameplaySettings::default(),
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
            scripts: None,
            map_ids: Vec::new(),
            map_population: std::collections::BTreeMap::new(),
            pk: false,
            pk_free: false,
            revive_point: None,
            portals: Vec::new(),
            quests: Vec::new(),
            regions: Vec::new(),
            world_flags: BTreeMap::new(),
            world: crate::WorldState::default(),
            world_changes: Vec::new(),
            using: None,
            dying_at: None,
            world_events: true,
            trades: Vec::new(),
            trade_asks: std::collections::BTreeMap::new(),
            traps: Vec::new(),
            trap_now: None,
            world_timers: Vec::new(),
            team_invites: BTreeMap::new(),
            courting: BTreeMap::new(),
            team_applies: BTreeMap::new(),
            guild_invites: BTreeMap::new(),
            closed_teams: std::collections::BTreeSet::new(),
            teams_keeping_money: std::collections::BTreeSet::new(),
            teams_sharing_items: std::collections::BTreeSet::new(),
            elsewhere: BTreeMap::new(),
            team_ops: Vec::new(),
            npc_moves: Vec::new(),
            npcs_made: 0,
        };
        sim.fill_spawns();
        sim
    }

    /// Lets NPCs run the scripts of `host`; `map_ids` name this map for script teleports.
    /// What the scripts' blueprints report while running (for a debugger), when it has scripts.
    pub fn script_debug(
        &self,
    ) -> Option<std::rc::Rc<std::cell::RefCell<c3_script::debug::DebugLog>>> {
        self.scripts.as_ref().map(c3_script::ScriptHost::debug)
    }

    /// The game map ids this map answers to (its scripts' teleports, population counts).
    pub fn map_ids(&self) -> &[u32] {
        &self.map_ids
    }

    /// How many players (all, alive) are on each game map the host runs, for scripts.
    pub fn set_map_population(&mut self, population: std::collections::BTreeMap<u32, (u32, u32)>) {
        self.map_population = population;
    }

    pub fn set_scripts(&mut self, host: c3_script::ScriptHost, map_ids: Vec<u32>) {
        self.scripts = Some(host);
        self.map_ids = map_ids;
    }

    pub fn rules(&self) -> &dyn Rules {
        &*self.rules
    }

    /// The numbers it runs by (ticks, ranges, timings, the bag).
    pub fn settings(&self) -> &GameplaySettings {
        &self.settings
    }

    /// Items a bag holds.
    pub fn bag_size(&self) -> usize {
        self.settings.bag_size as usize
    }

    /// Runs by `settings` from now on (timers already started keep their end).
    pub fn set_settings(&mut self, settings: GameplaySettings) {
        self.settings = settings;
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
        let tick = self.settings.tick_ms.max(1);
        while self.pending_ms >= tick {
            self.pending_ms -= tick;
            self.now_ms += tick;
            self.tick();
        }
    }

    fn tick(&mut self) {
        self.tick_statuses();
        self.tick_timers();
        self.tick_transforms();
        self.tick_trades();
        self.tick_world_events();
        let ids: Vec<EntityId> = self.players.keys().copied().collect();
        for id in ids {
            self.tick_player(id);
        }
        let ids: Vec<EntityId> = self.monsters.keys().copied().collect();
        for id in ids {
            self.tick_monster(id);
        }
        self.tick_marks();
        self.tick_regions();
        self.tick_spawns();
        self.expire_items();
        for problem in self.rules.take_problems() {
            self.events.push(Event::Warning(problem));
        }
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

    /// Cells that lead to another map.
    pub fn portal_cells(&self) -> impl Iterator<Item = &CellPos> {
        self.portals.iter().map(|(cell, _)| cell)
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
            .any(|n| n.id == npc && distance(n.cell, player) <= self.settings.trade_range)
    }

    pub(crate) fn message(&mut self, player: EntityId, text: impl Into<String>) {
        self.events.push(Event::Message {
            player,
            text: text.into(),
        });
    }
}
