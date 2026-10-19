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
        let stats = self.rules.player_stats(level);
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
                regions: Vec::new(),
                stats,
                bag: Vec::new(),
                equipment: Vec::new(),
                look: Default::default(),
                arrive_tag: None,
                team: None,
                running: false,
                task: Task::Idle,
                dialogue: None,
                shop: None,
                flags: Default::default(),
                path: VecDeque::new(),
                next_step_ms: 0,
                next_attack_ms: 0,
                revive_at_ms: None,
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

    /// Uses item `item` from the bag: wears equipment, drinks a potion.
    pub fn use_item(&mut self, id: EntityId, item: u32) {
        let Some(p) = self.players.get(&id).filter(|p| p.is_alive()) else {
            return;
        };
        let Some(slot) = p.bag.iter().position(|i| i.id == item) else {
            return;
        };
        let kind = p.bag[slot].kind;
        let Some(t) = self.rules.item(kind) else {
            return;
        };
        if t.slot.is_some() {
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
        let slot_of = |kind: u32, rules: &dyn crate::Rules| rules.item(kind).and_then(|t| t.slot);
        let slot = slot_of(item.kind, &*self.rules);
        let p = self.players.get_mut(&id).expect("wearing");
        if let Some(at) = p
            .equipment
            .iter()
            .position(|e| slot_of(e.kind, &*self.rules) == slot)
        {
            let old = p.equipment.remove(at);
            p.bag.push(old);
        }
        p.equipment.push(item);
        self.refresh_stats(id);
        self.events.push(Event::Dressed { player: id });
    }

    /// Recomputes the player's stats from its level and what it wears (broken items give
    /// nothing; each quality step adds its share).
    pub(crate) fn refresh_stats(&mut self, id: EntityId) {
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        let base = self.rules.player_stats(p.level);
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
        let Some((price, currency)) = self.rules.item(kind).map(|t| (t.price, t.currency.clone()))
        else {
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
            if now >= at {
                p.revive_at_ms = None;
                p.life = p.stats.max_life;
                p.mana = p.stats.max_mana;
                self.events.push(Event::Revived(id));
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
        let p = self.players.get_mut(&id).expect("ticking");
        let range = p.stats.attack_range.max(1) as i32;
        if distance(p.cell, goal) <= range {
            p.path.clear();
            if self.now_ms >= p.next_attack_ms {
                p.next_attack_ms = self.now_ms + u64::from(p.stats.attack_ms.max(1));
                p.direction = path::facing(p.cell, goal).unwrap_or(p.direction);
                if self.monsters.contains_key(&target) {
                    self.player_hits(id, target);
                } else {
                    self.player_hits_player(id, target);
                }
            }
            return;
        }
        // Re-plan when the monster moved away from where the path leads.
        if p.path.back().is_none_or(|&end| distance(end, goal) > range) {
            self.plan_path(id, goal);
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
        let (money, kind) = (ground.money, ground.kind);
        if let Some(kind) = kind
            && !self.give_item(id, kind)
        {
            return;
        }
        let ground = self.items.remove(&item).expect("checked above");
        self.index.remove(item, ground.cell);
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
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        p.life = 0;
        p.task = Task::Idle;
        p.path.clear();
        p.revive_at_ms = Some(revive_at);
        self.events.push(Event::Died { id, killer });
        let killer = killer.map_or(0, |k| i64::from(k.0));
        self.script_event(id, "on_death", vec![killer.into()]);
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
            self.events.push(Event::LeveledUp { player: id, level });
            self.refresh_stats(id);
            let p = self.players.get_mut(&id).expect("levelling");
            p.life = p.stats.max_life;
            p.mana = p.stats.max_mana;
            self.script_event(id, "on_level_up", vec![i64::from(level).into()]);
        }
    }
}
