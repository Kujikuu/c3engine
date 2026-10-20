//! Players: what hosts tell them to do (walk, attack, pick up, buy…) and how they carry it
//! out each tick.

use crate::entity::{Item, Player, Task};

/// Takes one from the stack at `slot` of `bag` (the slot goes when it was the last).
fn take_one(bag: &mut Vec<Item>, slot: usize) {
    if bag[slot].count > 1 {
        bag[slot].count -= 1;
    } else {
        bag.remove(slot);
    }
}
use crate::event::Event;
use crate::sim::Sim;
use crate::{EntityId, distance, ms};
use c3_scene::CellPos;
use c3_scene::path::{self, PathRules};
use std::collections::VecDeque;

/// Path search budget for a player's walk.
const PLAYER_PATH: PathRules = PathRules {
    max_climb: 25,
    max_nodes: 20_000,
};

/// The most items worked together at once (`on_combine` names the first 5; scripts read
/// them all with `combine_items`).
const MAX_COMBINED: usize = 20;

/// The farthest a player reaches to hit an NPC standing among cells no one walks on.
const MAX_NPC_REACH: i32 = 8;

impl Sim {
    /// Brings a player into the world at `cell` (or the walkable cell nearest it).
    pub fn add_player(&mut self, name: impl Into<String>, cell: CellPos, level: u32) -> EntityId {
        let id = self.new_entity();
        self.insert_player(id, name.into(), cell, level);
        id
    }

    /// Player `id` (not in use here) as a new character.
    pub(crate) fn insert_player(&mut self, id: EntityId, name: String, cell: CellPos, level: u32) {
        assert!(
            !self.players.contains_key(&id),
            "player {id:?} is already here"
        );
        let cell = path::nearest_walkable(&self.grid, cell, 30).unwrap_or(cell);
        let stats = self.rules.player_stats(&crate::StatsFor::level(level));
        self.players.insert(
            id,
            Player {
                id,
                name,
                cell,
                direction: 0,
                level,
                exp: 0,
                life: stats.max_life,
                mana: stats.max_mana,
                money: 0,
                wallet: Default::default(),
                skills: Vec::new(),
                cooldowns: Default::default(),
                statuses: Vec::new(),
                pk_kills: 0,
                timers: Vec::new(),
                combining: Vec::new(),
                placing: None,
                placed: None,
                regions: Vec::new(),
                stats,
                bag: Vec::new(),
                equipment: Vec::new(),
                look: Default::default(),
                arrive_tag: None,
                team: None,
                mates: Vec::new(),
                profession: 0,
                pk: 0,
                pk_mode: self.settings.pk.start_mode,
                mark: crate::NameMark::Good,
                crime_until: 0,
                next_decay_ms: 0,
                attributes: std::collections::BTreeMap::new(),
                record: None,
                running: false,
                task: Task::Idle,
                dialogue: None,
                shop: None,
                flags: Default::default(),
                texts: Default::default(),
                path: VecDeque::new(),
                next_step_ms: 0,
                next_attack_ms: 0,
                revive_at_ms: None,
                ghost: false,
                transform: None,
                storage: std::collections::BTreeMap::new(),
                saved_money: 0,
                killed_by_monster: 0,
            },
        );
        self.index.insert(id, cell);
        self.events.push(Event::Appeared(id));
    }

    pub(crate) fn living(&mut self, id: EntityId) -> Option<&mut Player> {
        self.players.get_mut(&id).filter(|p| p.is_alive())
    }

    pub(crate) fn plan_path(&mut self, id: EntityId, goal: CellPos) {
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        p.path = path::find_path(&self.grid, p.cell, goal, &PLAYER_PATH).into();
    }

    /// Jumps the player to `to`, if it is walkable and within the jump range.
    pub fn jump(&mut self, id: EntityId, to: CellPos) {
        if !self.walkable(to) {
            return;
        }
        let range = self.settings.jump_range;
        let Some(p) = self.living(id) else { return };
        if distance(p.cell, to) > range {
            return;
        }
        let from = p.cell;
        p.direction = path::facing(from, to).unwrap_or(p.direction);
        p.cell = to;
        p.path.clear();
        p.task = Task::Idle;
        self.index.moved(id, from, to);
        self.events.push(Event::Jumped { id, to });
    }

    /// Whether a player can walk from `from` to `to` in a few steps (`to` walkable, a short
    /// path between).
    pub fn reachable(&self, from: CellPos, to: CellPos, max_steps: usize) -> bool {
        if from == to {
            return self.walkable(to);
        }
        let rules = PathRules {
            max_nodes: 2_000,
            ..PLAYER_PATH
        };
        let path = path::find_path(&self.grid, from, to, &rules);
        path.last() == Some(&to) && path.len() <= max_steps
    }

    /// Walks the player to `goal` (or as near as it can get).
    pub fn walk(&mut self, id: EntityId, goal: CellPos) {
        // A ghost walks (and goes through no portals).
        if self.players.get(&id).is_some_and(|p| p.ghost) {
            if let Some(p) = self.players.get_mut(&id) {
                p.task = Task::Walk;
            }
            return self.plan_path(id, goal);
        }
        // Walking to a portal goes through it.
        if self.portal_at(goal).is_some() {
            return self.enter_portal(id, goal);
        }
        self.stop_talking(id);
        let Some(p) = self.living(id) else { return };
        p.task = Task::Walk;
        p.arrive_tag = None;
        self.plan_path(id, goal);
    }

    pub fn set_running(&mut self, id: EntityId, running: bool) {
        if let Some(p) = self.players.get_mut(&id) {
            p.running = running;
        }
    }

    /// Sends the player after monster `target`, hitting it when in range.
    pub fn attack(&mut self, id: EntityId, target: EntityId) {
        if self.foe_cell(id, target).is_none() {
            return;
        }
        self.stop_talking(id);
        if let Some(p) = self.living(id) {
            p.task = Task::Attack(target);
            p.path.clear();
        }
    }

    /// The living monster within `radius` cells the player can reach in fewest steps.
    pub fn nearest_monster(&self, id: EntityId, radius: i32) -> Option<EntityId> {
        let p = self.players.get(&id)?;
        let range = p.stats.attack_range.max(1) as i32;
        let rules = PathRules {
            max_nodes: 2_000,
            ..PLAYER_PATH
        };
        self.monsters
            .values()
            .filter(|m| m.is_alive() && distance(m.cell, p.cell) <= radius)
            .filter_map(|m| {
                if distance(m.cell, p.cell) <= range {
                    return Some((0, m.id));
                }
                let path = path::find_path(&self.grid, p.cell, m.cell, &rules);
                let end = *path.last()?;
                (distance(end, m.cell) <= range).then_some((path.len(), m.id))
            })
            .min()
            .map(|(_, id)| id)
    }

    /// Walks the player to item `item` and picks it up.
    pub fn pick_up(&mut self, id: EntityId, item: EntityId) {
        let Some(cell) = self.items.get(&item).map(|i| i.cell) else {
            return;
        };
        self.stop_talking(id);
        let Some(p) = self.living(id) else { return };
        p.task = Task::PickUp(item);
        self.plan_path(id, cell);
    }

    /// Puts the player on `cell` at once (a teleport).
    pub fn place(&mut self, id: EntityId, cell: CellPos) {
        if !self.walkable(cell) {
            return;
        }
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        self.index.moved(id, p.cell, cell);
        p.cell = cell;
        p.path.clear();
        p.task = Task::Idle;
        self.events.push(Event::Placed { id, cell });
    }

    /// Wears bag item `item` in `slot`: its type's own, or its other one (`alt_slot`).
    pub fn wear_in(&mut self, id: EntityId, item: u32, slot: u8) {
        let kind = (self.players.get(&id))
            .and_then(|p| p.bag.iter().find(|i| i.id == item))
            .map(|i| i.kind);
        let Some(t) = kind.and_then(|k| self.rules.item(k)) else {
            return;
        };
        if t.slot == Some(slot) {
            return self.use_item(id, item);
        }
        if t.alt_slot != Some(slot) || !self.may_wear(id, t.id) {
            return;
        }
        let p = self.players.get_mut(&id).expect("found above");
        let at = p
            .bag
            .iter()
            .position(|i| i.id == item)
            .expect("found above");
        let mut worn = p.bag.remove(at);
        worn.worn_at = slot;
        self.wear(id, worn);
    }

    /// Whether player `id` may wear item type `kind` (its level and the game's check); says
    /// why not.
    fn may_wear(&mut self, id: EntityId, kind: u32) -> bool {
        let (Some(p), Some(t)) = (self.players.get(&id), self.rules.item(kind)) else {
            return false;
        };
        if p.level < t.requires_level {
            let text = format!("You must be level {} to wear {}.", t.requires_level, t.name);
            self.message(id, text);
            return false;
        }
        if !self.rules.can_wear(&self.player_fighter(id), t) {
            let text = format!("You can't wear {} yet.", t.name);
            self.message(id, text);
            return false;
        }
        true
    }

    /// Uses item `item` from the bag: wears equipment, drinks a potion.
    pub fn use_item(&mut self, id: EntityId, item: u32) {
        let Some(p) = self.players.get(&id).filter(|p| p.is_alive()) else {
            return;
        };
        let Some(slot) = p.bag.iter().position(|i| i.id == item) else {
            return;
        };
        let kind = p.bag[slot].kind;
        // An item a script answers (custom event `use_<type>`): the script does what it does,
        // taking the item if it is spent.
        let event = format!("use_{kind}");
        if self.event_defined(&event, 0) {
            self.using = Some((id, item));
            self.script_event(id, &event, Vec::new());
            self.using = None;
            self.events.push(Event::Used { player: id, kind });
            return;
        }
        let Some(t) = self.rules.item(kind) else {
            return;
        };
        if t.slot.is_some() {
            if !self.may_wear(id, kind) {
                return;
            }
            let worn = self
                .players
                .get_mut(&id)
                .expect("checked above")
                .bag
                .remove(slot);
            self.wear(id, worn);
            return;
        }
        if t.restores_life == 0 && t.restores_mana == 0 {
            self.message(id, format!("{} can't be used.", t.name));
            return;
        }
        let (life, mana) = (t.restores_life, t.restores_mana);
        let p = self.players.get_mut(&id).expect("checked above");
        p.life = (p.life + life).min(p.stats.max_life);
        p.mana = (p.mana + mana).min(p.stats.max_mana);
        take_one(&mut p.bag, slot);
        self.events.push(Event::Used { player: id, kind });
        self.script_event(id, "on_use", vec![i64::from(kind).into()]);
    }

    /// Uses bag item `item` on item `target`: the game's `on_use_on(item's type, target)`
    /// says what happens (an upgrade).
    pub fn use_item_on(&mut self, id: EntityId, item: u32, target: u32) {
        let Some(p) = self.players.get(&id).filter(|p| p.is_alive()) else {
            return;
        };
        let Some(used) = p.bag.iter().find(|i| i.id == item).map(|i| i.kind) else {
            return;
        };
        let held = p.bag.iter().chain(&p.equipment).any(|i| i.id == target);
        if !held || item == target {
            return;
        }
        let args = vec![i64::from(used).into(), i64::from(target).into()];
        self.script_event(id, "on_use_on", args);
        self.events.push(Event::Dressed { player: id });
    }

    /// Items `items` (the player's own, up to five) worked together as `kind`:
    /// `on_combine(kind, item1, ..., item5, value)`, missing ones 0.
    /// Places what player `id` was asked to at `cell` (on the map, where no NPC stands, within
    /// its view): the placement's event runs with it.
    pub fn place_asked(&mut self, id: EntityId, cell: CellPos, direction: u8) {
        let range = self.settings.view_range;
        let taken = self.npcs.iter().any(|n| n.cell == cell);
        let Some(p) = self.players.get_mut(&id).filter(|p| p.is_alive()) else {
            return;
        };
        let near = distance(p.cell, cell) <= range;
        let Some(event) = p.placing.take() else {
            return;
        };
        if taken || !near || self.grid.get(cell).is_none() {
            self.message(id, "That can't go there.");
            return;
        }
        p.placed = Some((cell, direction % 8));
        self.script_event(id, &event, Vec::new());
        if let Some(p) = self.players.get_mut(&id) {
            p.placed = None;
        }
    }

    pub fn combine(&mut self, id: EntityId, kind: u32, items: &[u32], value: i64) {
        let Some(p) = self.players.get(&id).filter(|p| p.is_alive()) else {
            return;
        };
        let held = |item: &u32| p.bag.iter().chain(&p.equipment).any(|i| i.id == *item);
        if items.is_empty() || items.len() > MAX_COMBINED || !items.iter().all(held) {
            return;
        }
        let mut args: Vec<c3_script::Dynamic> = vec![i64::from(kind).into()];
        args.extend((0..5).map(|i| i64::from(items.get(i).copied().unwrap_or(0)).into()));
        args.push(value.into());
        if let Some(p) = self.players.get_mut(&id) {
            p.combining = items.to_vec();
        }
        self.script_event(id, "on_combine", args);
        if let Some(p) = self.players.get_mut(&id) {
            p.combining.clear();
        }
        self.events.push(Event::Dressed { player: id });
    }

    /// Dresses the player in a new item of `kind` (starting gear); false if it isn't worn.
    pub fn equip(&mut self, id: EntityId, kind: u32) -> bool {
        if self.rules.item(kind).and_then(|t| t.slot).is_none() || !self.players.contains_key(&id) {
            return false;
        }
        let item = self.new_item(kind);
        self.wear(id, item);
        true
    }

    /// Takes off the item worn in item `item`'s place, into the bag (if there is room).
    pub fn unequip(&mut self, id: EntityId, item: u32) {
        let bag_size = self.bag_size();
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        let Some(at) = p.equipment.iter().position(|i| i.id == item) else {
            return;
        };
        if p.bag.len() >= bag_size {
            self.message(id, "Your bag is full.");
            return;
        }
        let item = p.equipment.remove(at);
        p.bag.push(item);
        self.refresh_stats(id);
        self.events.push(Event::Dressed { player: id });
    }

    /// Puts `item` on, moving what was worn in its slot to the bag.
    fn wear(&mut self, id: EntityId, item: Item) {
        let slot = self.slot_of(&item);
        let p = self.players.get(&id).expect("wearing");
        let at = (p.equipment.iter()).position(|e| self.slot_of(e) == slot);
        let p = self.players.get_mut(&id).expect("wearing");
        if let Some(at) = at {
            let mut old = p.equipment.remove(at);
            old.worn_at = 0;
            p.bag.push(old);
        }
        p.equipment.push(item);
        self.refresh_stats(id);
        self.events.push(Event::Dressed { player: id });
    }

    /// The slot worn item `item` is in.
    pub fn slot_of(&self, item: &Item) -> Option<u8> {
        item.slot(self.rules.item(item.kind))
    }

    /// The player's attributes: every one the settings name (their default unless the player
    /// has its own value), and any others it has.
    pub(crate) fn attributes_of(
        &self,
        p: &crate::Player,
    ) -> std::collections::BTreeMap<String, i64> {
        (self.settings.attributes.iter())
            .map(|a| (a.name.clone(), a.default))
            .chain(p.attributes.iter().map(|(k, &v)| (k.clone(), v)))
            .collect()
    }

    /// Player `id` as one side of a fight (see [`crate::Fighter::facts`]).
    /// Which of its type's looks `item` has (the settings' item look attribute; 0: none).
    pub fn item_look(&self, item: &Item) -> u32 {
        let names = &self.settings.item_attributes;
        (names
            .iter()
            .position(|n| *n == self.settings.item_look_attribute))
        .and_then(|i| item.attrs.get(i))
        .map_or(0, |&v| u32::try_from(v).unwrap_or(0))
    }

    /// The numbers the game shows of player `id` (rules `shown`).
    pub fn shown(&self, id: EntityId) -> Vec<(String, i64)> {
        self.rules.shown(&self.player_fighter(id))
    }

    pub(crate) fn player_fighter(&self, id: EntityId) -> crate::Fighter {
        let Some(p) = self.players.get(&id) else {
            return crate::Fighter::default();
        };
        let mut facts = self.attributes_of(p);
        facts.insert("monster".into(), 0);
        facts.insert("profession".into(), i64::from(p.profession));
        facts.insert("body".into(), i64::from(p.look.body));
        // The clock's seconds, as scripts' clock() (timed bonuses end by it).
        let clock = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        facts.insert("clock".into(), clock);
        let names = &self.settings.item_attributes;
        for name in names {
            facts.insert(format!("worn_{name}"), 0);
        }
        // What it wears, broken items left out.
        let worn = (p.equipment.iter())
            .filter_map(|item| Some((item, self.rules.item(item.kind)?)))
            .filter(|(item, t)| !item.is_broken(t));
        for (item, t) in worn {
            let slot = item.slot(Some(t));
            if let Some(slot) = slot {
                facts.insert(format!("worn{slot}"), i64::from(item.kind));
            }
            for (name, &value) in names.iter().zip(&item.attrs) {
                *facts.entry(format!("worn_{name}")).or_default() += i64::from(value);
                if let Some(slot) = slot {
                    facts.insert(format!("worn{slot}_{name}"), i64::from(value));
                }
                // A socketed item's numbers count while this one is worn.
                let socketed = (self.settings.sockets.contains(name) && value > 0)
                    .then(|| u32::try_from(value).ok())
                    .flatten()
                    .and_then(|v| {
                        self.rules
                            .item(self.settings.socket_item_base.saturating_add(v))
                    });
                for (fact, &v) in socketed.iter().flat_map(|s| &s.facts) {
                    *facts.entry(format!("socket_{fact}")).or_default() += v;
                }
            }
            // The numbers of what it wears add up.
            for (fact, &v) in &t.facts {
                *facts.entry(format!("gear_{fact}")).or_default() += v;
            }
        }
        crate::statuses::add_status_facts(&mut facts, &p.statuses, &*self.rules);
        crate::Fighter {
            level: p.level,
            stats: p.stats,
            facts,
        }
    }

    /// Recomputes the player's stats from its level and what it wears (broken items give
    /// nothing; each quality step adds its share).
    pub(crate) fn refresh_stats(&mut self, id: EntityId) {
        if !self.players.contains_key(&id) {
            return;
        }
        // Its attributes and what it wears (the fighter's facts).
        let attributes = self.player_fighter(id).facts;
        let p = &self.players[&id];
        let base = self.rules.player_stats(&crate::StatsFor {
            level: p.level,
            profession: p.profession,
            attributes: &attributes,
        });
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        let per_step = self.settings.quality_bonus_percent;
        p.stats = p
            .equipment
            .iter()
            .filter_map(|e| Some((e, self.rules.item(e.kind)?)))
            .filter(|(e, t)| !e.is_broken(t))
            .fold(base, |stats, (e, t)| {
                let percent = 100 + u32::from(e.quality) * per_step;
                stats.with_bonus(&t.bonus.scaled(percent))
            });
        // Transformed: the monster's fighting numbers, its own life kept.
        if let Some(t) = p.transform.and_then(|(m, _)| self.rules.monster(m)) {
            p.stats = crate::Stats {
                max_life: p.stats.max_life,
                max_mana: p.stats.max_mana,
                ..t.stats
            };
        }
        p.stats = crate::statuses::with_statuses(p.stats, &p.statuses, &*self.rules);
        p.life = p.life.min(p.stats.max_life);
        p.mana = p.mana.min(p.stats.max_mana);
    }

    /// Buys one `kind` from NPC `npc`'s shop.
    pub fn buy(&mut self, id: EntityId, npc: u32, kind: u32) {
        let Some(p) = self.players.get(&id) else {
            return;
        };
        if !self.npc_in_reach(p.cell, npc) || !self.rules.shop(npc).contains(&kind) {
            return;
        }
        // The shop's currency for it, else the item's own.
        let Some(t) = self.rules.item(kind) else {
            return;
        };
        let currency =
            (self.rules.shop_currency(npc, kind)).map_or_else(|| t.currency.clone(), str::to_owned);
        let Some(price) = t.price_in(&currency) else {
            return;
        };
        if self.funds(p, &currency) < price {
            let name = self.currency_name(&currency).to_owned();
            self.message(id, format!("You don't have enough {name}."));
            return;
        }
        if !self.give_item(id, kind) {
            return;
        }
        let p = self.players.get_mut(&id).expect("checked above");
        if currency.is_empty() {
            p.money -= price;
        } else {
            *p.wallet.entry(currency).or_default() -= price;
        }
        self.events.push(Event::Bought {
            player: id,
            kind,
            price,
        });
    }

    /// Sells item `item` from the bag to NPC `npc`'s shop.
    pub fn sell(&mut self, id: EntityId, npc: u32, item: u32) {
        let Some(p) = self.players.get(&id) else {
            return;
        };
        if !self.npc_in_reach(p.cell, npc) || self.rules.shop(npc).is_empty() {
            return;
        }
        let Some(slot) = p.bag.iter().position(|i| i.id == item) else {
            return;
        };
        let kind = p.bag[slot].kind;
        let price = self.rules.sell_price(kind);
        let currency = self
            .rules
            .item(kind)
            .map(|t| t.currency.clone())
            .unwrap_or_default();
        let p = self.players.get_mut(&id).expect("checked above");
        take_one(&mut p.bag, slot);
        if currency.is_empty() {
            p.money += price;
        } else {
            *p.wallet.entry(currency).or_default() += price;
        }
        self.events.push(Event::Sold {
            player: id,
            kind,
            price,
        });
    }

    /// Puts a new `kind` in the player's bag; false (and a message) when it is full.
    pub fn give_item(&mut self, id: EntityId, kind: u32) -> bool {
        self.give_items(id, kind, 1) == 1
    }

    /// Puts `count` new `kind` in the bag, onto its stacks first; how many fit (a message
    /// says when the bag is full).
    pub fn give_items(&mut self, id: EntityId, kind: u32, count: u32) -> u32 {
        let stack = self.rules.item(kind).map_or(1, |t| t.stack.max(1));
        let bag_size = self.bag_size();
        let Some(p) = self.players.get_mut(&id) else {
            return 0;
        };
        let mut left = count;
        for item in p
            .bag
            .iter_mut()
            .filter(|i| i.kind == kind && i.count < stack)
        {
            let added = (stack - item.count).min(left);
            item.count += added;
            left -= added;
        }
        while left > 0 {
            if self.players[&id].bag.len() >= bag_size {
                self.message(id, "Your bag is full.");
                break;
            }
            let mut item = self.new_item(kind);
            item.count = left.min(stack);
            left -= item.count;
            self.players
                .get_mut(&id)
                .expect("checked above")
                .bag
                .push(item);
        }
        count - left
    }

    /// Takes `count` of `kind` from the bag, if it holds that many.
    pub fn take_items(&mut self, id: EntityId, kind: u32, count: u32) -> bool {
        let Some(p) = self.players.get_mut(&id) else {
            return false;
        };
        let held: u64 = p
            .bag
            .iter()
            .filter(|i| i.kind == kind)
            .map(|i| u64::from(i.count))
            .sum();
        if held < u64::from(count) {
            return false;
        }
        let mut left = count;
        for item in p.bag.iter_mut().filter(|i| i.kind == kind) {
            let taken = item.count.min(left);
            item.count -= taken;
            left -= taken;
        }
        p.bag.retain(|i| i.count > 0);
        true
    }

    /// How much of `currency` (empty: the money) player `p` has.
    fn funds(&self, p: &Player, currency: &str) -> u64 {
        if currency.is_empty() {
            p.money
        } else {
            p.wallet.get(currency).copied().unwrap_or(0)
        }
    }

    /// What players call `currency` (empty: the money).
    pub fn currency_name<'a>(&'a self, currency: &'a str) -> &'a str {
        if currency.is_empty() {
            &self.settings.currency
        } else {
            currency
        }
    }

    /// Wears down player `id`'s worn items that `wears` picks (weapons when it hits, armour
    /// when it is hit): each may lose a point of durability, and one worn out is said.
    pub(crate) fn wear_items(&mut self, id: EntityId, wears: fn(&crate::ItemType) -> bool) {
        let chance = self.settings.wear_percent;
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        let mut broke = Vec::new();
        for item in &mut p.equipment {
            let Some(t) = self.rules.item(item.kind) else {
                continue;
            };
            if t.durability == 0 || item.durability == 0 || !wears(t) {
                continue;
            }
            if self.rng.chance(chance, 100) {
                item.durability -= 1;
                if item.durability == 0 {
                    broke.push(t.name.clone());
                }
            }
        }
        if !broke.is_empty() {
            self.refresh_stats(id);
            self.events.push(Event::Dressed { player: id });
            for name in broke {
                self.message(id, format!("Your {name} broke."));
            }
        }
    }

    /// Sets the player's money (scripts change it directly).
    pub fn set_money(&mut self, id: EntityId, money: u64) {
        if let Some(p) = self.players.get_mut(&id) {
            p.money = money;
        }
    }

    // ---- ticking ----

    pub(crate) fn tick_player(&mut self, id: EntityId) {
        let now = self.now_ms;
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        if let Some(at) = p.revive_at_ms {
            if now >= at && !self.settings.revive_by_request {
                self.revive(id);
            } else if p.ghost && p.task == Task::Walk && !self.step_player(id) {
                // A ghost walks.
                self.players.get_mut(&id).expect("ticking").task = Task::Idle;
            }
            return;
        }
        if self.is_stunned(id) {
            return;
        }
        let p = &self.players[&id];
        match p.task.clone() {
            Task::Idle => {}
            Task::Walk => {
                if !self.step_player(id) {
                    let p = self.players.get_mut(&id).expect("ticking");
                    p.task = Task::Idle;
                    // A script's walk tells it the player got there.
                    if let Some(tag) = p.arrive_tag.take() {
                        self.script_event(id, "on_arrive", vec![tag.into()]);
                    }
                }
            }
            Task::Attack(target) => self.chase_and_hit(id, target),
            Task::PickUp(item) => self.walk_to_item(id, item),
            Task::Talk(npc) => self.walk_to_npc(id, npc),
            Task::Portal(cell) => self.walk_to_portal(id, cell),
            Task::Cast { skill, at } => self.walk_to_cast(id, skill, at),
        }
    }

    /// Takes the next planned step when it is time; false when there is none left.
    pub(crate) fn step_player(&mut self, id: EntityId) -> bool {
        let now = self.now_ms;
        let p = self.players.get_mut(&id).expect("ticking");
        if now < p.next_step_ms {
            return true;
        }
        let Some(next) = p.path.pop_front() else {
            return false;
        };
        if !path::can_step(&self.grid, p.cell, next, &PLAYER_PATH) {
            p.path.clear();
            return false;
        }
        p.direction = path::direction(p.cell, next).unwrap_or(p.direction);
        self.index.moved(id, p.cell, next);
        p.cell = next;
        p.next_step_ms = now + p.step_ms();
        let walking = matches!(p.task, Task::Walk);
        self.events.push(Event::Moved {
            id,
            to: next,
            running: p.running,
        });
        self.step_on_traps(id, next);
        // Stepping onto a portal on the way somewhere goes through it, as in the classic game.
        if walking && let Some(to) = self.portal_at(next) {
            self.travel(id, to);
            return false;
        }
        true
    }

    fn chase_and_hit(&mut self, id: EntityId, target: EntityId) {
        let Some(goal) = self.foe_cell(id, target) else {
            let p = self.players.get_mut(&id).expect("ticking");
            p.task = Task::Idle;
            p.path.clear();
            return;
        };
        // An NPC stands on cells no one walks on (a pole, a gate): it is reached from the
        // nearest ring of walkable cells around it.
        let is_npc = !self.monsters.contains_key(&target) && !self.players.contains_key(&target);
        let (reach, beside) = match is_npc {
            true => self.nearest_ring(goal, self.players[&id].cell),
            false => (0, goal),
        };
        let p = self.players.get_mut(&id).expect("ticking");
        let range = (p.stats.attack_range.max(1) as i32).max(reach);
        if distance(p.cell, goal) <= range {
            p.path.clear();
            if self.now_ms >= p.next_attack_ms {
                p.next_attack_ms = self.now_ms + u64::from(p.stats.attack_ms.max(1));
                p.direction = path::facing(p.cell, goal).unwrap_or(p.direction);
                if self.monsters.contains_key(&target) {
                    self.player_hits(id, target);
                } else if self.players.contains_key(&target) {
                    self.player_hits_player(id, target);
                } else {
                    self.player_hits_npc(id, target, None);
                }
            }
            return;
        }
        // Re-plan when the monster moved away from where the path leads.
        if p.path.back().is_none_or(|&end| distance(end, goal) > range) {
            self.plan_path(id, beside);
            let p = self.players.get_mut(&id).expect("ticking");
            if p.path.is_empty() {
                // Nothing brings it closer (a wall between them).
                p.task = Task::Idle;
                self.message(id, "You can't reach it.");
                return;
            }
        }
        self.step_player(id);
    }

    /// The nearest ring around `center` (up to 8 cells out) with a walkable cell, and its
    /// walkable cell nearest `from`; ring 1 and `center` when there is none.
    fn nearest_ring(&self, center: CellPos, from: CellPos) -> (i32, CellPos) {
        for r in 1..=MAX_NPC_REACH {
            let ring = (-r..=r)
                .flat_map(|dx| (-r..=r).map(move |dy| (center.0 + dx, center.1 + dy)))
                .filter(|&c| distance(c, center) == r && self.walkable(c));
            if let Some(cell) = ring.min_by_key(|&c| distance(c, from)) {
                return (r, cell);
            }
        }
        (1, center)
    }

    /// Player `id` spends attribute points: all of `points` when it has that many and each
    /// goes to one of the settings' point attributes, else none.
    pub fn spend_points(&mut self, id: EntityId, points: &[(String, u32)]) {
        let pool = self.settings.points_attribute.clone();
        let allowed = |a: &String| self.settings.point_attributes.contains(a);
        let total: u64 = points.iter().map(|(_, n)| u64::from(*n)).sum();
        if pool.is_empty() || total == 0 || !points.iter().all(|(a, _)| allowed(a)) {
            return;
        }
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        let have = p.attributes.get(&pool).copied().unwrap_or(0);
        if (have.max(0) as u64) < total {
            return;
        }
        p.attributes.insert(pool, have - total as i64);
        for (attribute, n) in points {
            *p.attributes.entry(attribute.clone()).or_default() += i64::from(*n);
        }
        self.refresh_stats(id);
        self.events.push(Event::Dressed { player: id });
    }

    /// Whether player `id` may pick up `ground`: anyone's once unprotected; while protected,
    /// its owner's, the owner's spouse's, and its team's as the team shares (money unless the
    /// team keeps it, items when the team shares them, never the types no team shares).
    fn may_pick_up(&self, id: EntityId, ground: &crate::GroundItem) -> bool {
        let Some(owner) = ground
            .owner
            .filter(|_| self.now_ms < ground.protected_until_ms)
        else {
            return true;
        };
        if owner == id {
            return true;
        }
        let owner_name = self.players.get(&owner).map(|p| p.name.clone());
        let spouse = (self.players.get(&id))
            .and_then(|p| p.texts.get(&self.settings.spouse_text))
            .is_some_and(|s| !s.is_empty() && Some(s) == owner_name.as_ref());
        if spouse {
            return true;
        }
        let Some(leader) = self.players.get(&id).and_then(|p| p.team) else {
            return false;
        };
        if !self.team_members(id).contains(&owner) {
            return false;
        }
        match ground.kind {
            None => !self.teams_keeping_money.contains(&leader),
            Some(kind) => {
                self.teams_sharing_items.contains(&leader)
                    && !self.settings.team_never_shared.contains(&kind)
            }
        }
    }

    /// What of `money` player `id` picks up from teammate `owner`'s loot goes to the owner
    /// (the settings' team money cut); given to it.
    fn team_money_cut(&mut self, id: EntityId, owner: Option<EntityId>, money: u64) -> u64 {
        let cut = self.settings.team_money_cut;
        let Some(owner) = owner.filter(|&o| o != id && cut.percent > 0 && money > 0) else {
            return 0;
        };
        let (Some(picker), Some(lead)) = (self.players.get(&id), self.players.get(&owner)) else {
            return 0;
        };
        let applies = picker.level < cut.picker_below
            && lead.level >= cut.owner_from
            && picker.level + cut.gap <= lead.level
            && self.team_members(id).contains(&owner);
        let share = money * u64::from(cut.percent) / 100;
        if !applies || share == 0 {
            return 0;
        }
        let name = picker.name.clone();
        if let Some(lead) = self.players.get_mut(&owner) {
            lead.money += share;
        }
        self.message(
            owner,
            format!("{name} picked up your loot: {share} silver is yours."),
        );
        share
    }

    fn walk_to_item(&mut self, id: EntityId, item: EntityId) {
        let Some(cell) = self.items.get(&item).map(|i| i.cell) else {
            self.players.get_mut(&id).expect("ticking").task = Task::Idle;
            return;
        };
        let here = self.players[&id].cell;
        if here == cell {
            self.players.get_mut(&id).expect("ticking").task = Task::Idle;
            self.take_ground_item(id, item);
        } else if !self.step_player(id) {
            // The path ran out short of the item.
            self.players.get_mut(&id).expect("ticking").task = Task::Idle;
        }
    }

    fn take_ground_item(&mut self, id: EntityId, item: EntityId) {
        let Some(ground) = self.items.get(&item) else {
            return;
        };
        if !self.may_pick_up(id, ground) {
            self.message(id, "Someone else's loot can't be picked up yet.");
            return;
        }
        let owner = ground.owner;
        let (money, kind, dropped) = (ground.money, ground.kind, ground.item);
        let bag_size = self.bag_size();
        let taken = match (dropped, kind) {
            // A dropped item comes back as it was.
            (Some(item), _) => {
                let p = self.players.get_mut(&id).expect("picking up");
                let room = p.bag.len() < bag_size;
                if room {
                    p.bag.push(item);
                }
                room
            }
            (None, Some(kind)) => self.give_item(id, kind),
            (None, None) => true,
        };
        if !taken {
            return;
        }
        let ground = self.items.remove(&item).expect("checked above");
        self.index.remove(item, ground.cell);
        let money = money - self.team_money_cut(id, owner, money);
        let p = self.players.get_mut(&id).expect("picking up");
        p.money += money;
        self.events.push(Event::PickedUp {
            player: id,
            money,
            kind,
        });
        self.events.push(Event::Vanished(item));
    }

    /// The player dies; it comes back after a while where it fell.
    pub(crate) fn kill_player(&mut self, id: EntityId, killer: Option<EntityId>) {
        let revive_at = self.now_ms + ms(self.settings.revive_seconds);
        let by_monster = killer
            .and_then(|k| self.monsters.get(&k))
            .map_or(0, |m| m.kind);
        let revive_point = self.revive_point;
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        p.life = 0;
        p.task = Task::Idle;
        p.path.clear();
        p.revive_at_ms = Some(revive_at);
        p.killed_by_monster = by_monster;
        // The map's revive point is where it comes back.
        if revive_point.is_some() {
            p.record = revive_point;
        }
        let victim = (p.name.clone(), i64::from(p.level), i64::from(p.pk));
        self.events.push(Event::Died { id, killer });
        // A player's kill: the killer's own event (the victim's name and level).
        if let Some(by) = killer.filter(|k| self.players.contains_key(k)) {
            let args = vec![victim.0.into(), victim.1.into(), victim.2.into()];
            self.script_event(by, "on_kill_player", args);
            // And the one killed's: who killed it.
            let name = self.players[&by].name.clone();
            self.script_event(id, "on_killed_by_player", vec![name.into()]);
        }
        let killer = killer.map_or(0, |k| i64::from(k.0));
        self.script_event(id, "on_death", vec![killer.into()]);
    }

    /// Brings dead player `id` back once its wait is over: full life and mana, at its return
    /// point (the map's revive point when it died) or where it lies.
    pub fn revive(&mut self, id: EntityId) {
        let now = self.now_ms;
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        if p.revive_at_ms.is_none_or(|at| now < at) {
            return;
        }
        p.revive_at_ms = None;
        p.ghost = false;
        p.task = Task::Idle;
        p.path.clear();
        p.life = p.stats.max_life;
        p.mana = p.stats.max_mana;
        let to = p.record;
        self.events.push(Event::Revived(id));
        if let Some(to) =
            to.filter(|t| !self.is_this_map(t.map) || t.cell != self.players[&id].cell)
        {
            self.travel(id, to);
        }
        self.script_event(id, "on_revive", Vec::new());
    }

    /// Dead player `id` gets up as a ghost (when ghosts walk).
    pub fn become_ghost(&mut self, id: EntityId) {
        let walk = self.settings.ghosts_walk;
        if let Some(p) = self
            .players
            .get_mut(&id)
            .filter(|p| !p.is_alive() && walk && !p.ghost)
        {
            p.ghost = true;
            self.events.push(Event::Dressed { player: id });
        }
    }

    /// Adds experience, rising levels as it fills them.
    pub(crate) fn gain_exp(&mut self, id: EntityId, amount: u64) {
        if amount == 0 {
            return;
        }
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        p.exp += amount;
        self.events.push(Event::ExpGained { player: id, amount });
        loop {
            let p = self.players.get_mut(&id).expect("levelling");
            let Some(need) = self.rules.level_exp(p.level) else {
                // The top level keeps no experience.
                p.exp = 0;
                break;
            };
            if p.exp < need {
                break;
            }
            p.exp -= need;
            p.level += 1;
            let level = p.level;
            let name = p.name.clone();
            self.events.push(Event::LeveledUp { player: id, level });
            let map = self.map_ids.first().copied().map_or(0, i64::from);
            self.change_world(c3_script::WorldChange::Online {
                name,
                map,
                level: i64::from(level),
            });
            self.refresh_stats(id);
            let p = self.players.get_mut(&id).expect("levelling");
            p.life = p.stats.max_life;
            p.mana = p.stats.max_mana;
            self.script_event(id, "on_level_up", vec![i64::from(level).into()]);
        }
    }
}
