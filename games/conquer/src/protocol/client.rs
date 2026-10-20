//! One client of the Conquer front: what it has been shown, how the world's updates look in
//! Conquer's packets, and what its requests ask of the engine.

use super::packets::{self as pk, Request, Spawn, action, attr, channel, interact, item};
use crate::character::player_look;
use c3_game::{
    CastAt, EntityId, EntityState, Event, Input, Player, Rules, Sim, TeamAction, WorldUpdate,
};
use c3_online::Welcome;
use c3_scene::{CellPos, Npc, path};
use std::collections::{BTreeMap, HashMap};

/// Players' ids as the client knows them start here.
const PLAYER_BASE: u32 = 1_000_000;
/// Monsters' ids as the client knows them.
const MONSTER_IDS: std::ops::RangeInclusive<u32> = 400_001..=599_999;
/// The face every character wears (the engine has no faces yet).
const FACE: u32 = 1;
/// Trojan.
const PROFESSION: u8 = 10;
const WHITE: u32 = 0x00FF_FFFF;
const YELLOW: u32 = 0x00FF_FF00;
/// Step deltas of Conquer's eight directions.
const DX: [i32; 8] = [0, -1, -1, -1, 0, 1, 1, 1];
const DY: [i32; 8] = [1, 1, 0, -1, -1, -1, 0, 1];

/// The ids the client knows roles by: players and ground items keep the engine's (players
/// offset), monsters get one in the monster range.
#[derive(Default)]
pub struct Ids {
    monsters: HashMap<EntityId, u32>,
    back: HashMap<u32, EntityId>,
    next: u32,
}

impl Ids {
    fn player(id: EntityId) -> u32 {
        PLAYER_BASE + id.0
    }

    fn monster(&mut self, id: EntityId) -> u32 {
        if let Some(&c) = self.monsters.get(&id) {
            return c;
        }
        let span = MONSTER_IDS.end() - MONSTER_IDS.start() + 1;
        let c = MONSTER_IDS.start() + self.next % span;
        self.next = self.next.wrapping_add(1);
        if let Some(old) = self.back.insert(c, id) {
            self.monsters.remove(&old);
        }
        self.monsters.insert(id, c);
        c
    }

    fn of(&mut self, state: &EntityState) -> u32 {
        match state {
            EntityState::Player(p) => Self::player(p.id),
            EntityState::Monster(m) => self.monster(m.id),
            EntityState::Item(i) => i.id.0,
        }
    }

    /// The entity the client means by `id`.
    fn entity(&self, id: u32) -> Option<EntityId> {
        if id >= PLAYER_BASE {
            Some(EntityId(id - PLAYER_BASE))
        } else if MONSTER_IDS.contains(&id) {
            self.back.get(&id).copied()
        } else {
            Some(EntityId(id))
        }
    }
}

/// TQ's PK mode numbers (Hero.h PKMODE_*): free, monsters only, team-exclude, arrestment
/// (and the guild mode, taken as team-exclude).
fn pk_mode(code: u32) -> c3_core::settings::PkMode {
    use c3_core::settings::PkMode;
    match code {
        1 => PkMode::Peace,
        2 | 4 => PkMode::Team,
        3 => PkMode::Arrest,
        _ => PkMode::Free,
    }
}

fn pk_mode_code(mode: c3_core::settings::PkMode) -> u32 {
    use c3_core::settings::PkMode;
    match mode {
        PkMode::Free => 0,
        PkMode::Peace => 1,
        PkMode::Team => 2,
        PkMode::Arrest => 3,
    }
}

/// The ids friends and enemies are known by in the client (above every player's and
/// monster's).
const CONTACT_IDS: u32 = 3_500_000_000;

/// What the client was told of the hero's guild: its id, the hero's rank, its fund and member
/// count, and what the hero gave it.
type GuildShown = (u32, u8, u32, u32, i32);

fn cell16(c: CellPos) -> (u16, u16) {
    (c.0.max(0) as u16, c.1.max(0) as u16)
}

fn look_face(p: &Player, rules: &dyn Rules) -> u32 {
    let face = if p.look.face == 0 {
        FACE
    } else {
        p.look.face % 1000
    };
    let face = face * 10_000 + 1000 + player_look(p).look;
    if let Some(t) = p.transform.and_then(|(m, _)| rules.monster(m)) {
        return face + (t.look % 1000) * 10_000_000;
    }
    // A ghost's look: the body's plus 980000000 (male) or 990000000 (female).
    match (p.ghost, p.look.body > 2) {
        (false, _) => face,
        (true, true) => face + 980_000_000,
        (true, false) => face + 990_000_000,
    }
}

/// Strength, agility, vitality and spirit.
type Points = [u32; 4];

/// The hero's attribute points.
fn points(p: &Player) -> Points {
    ["strength", "agility", "vitality", "spirit"].map(|name| {
        let value = p.attributes.get(name).copied().unwrap_or(0);
        u32::try_from(value.max(0)).unwrap_or(u32::MAX)
    })
}

/// The hero's XP bar (the `xp` attribute).
fn xp(p: &Player) -> u64 {
    u64::try_from(p.attributes.get("xp").copied().unwrap_or(0).max(0)).unwrap_or(0)
}

/// A player's status bits: dead, a ghost, a full XP bar, and its name's colour.
fn status(p: &Player, rules: &dyn Rules) -> u32 {
    let dead = if p.is_alive() { 0 } else { pk::STATUS_DEAD };
    // The client's effect bits of its statuses (TQ's, from their `client_bit`).
    let effects = (p.statuses.iter())
        .filter_map(|s| rules.status(s.id)?.facts.get("client_bit").copied())
        .fold(0, |bits, bit| bits | u32::try_from(bit).unwrap_or(0));
    let ghost = if p.ghost { pk::STATUS_GHOST } else { 0 };
    let xp_full = if xp(p) >= 100 { pk::STATUS_XP_FULL } else { 0 };
    dead | ghost
        | effects
        | xp_full
        | match p.mark {
            c3_game::NameMark::Good => 0,
            c3_game::NameMark::Criminal => pk::STATUS_CRIME,
            c3_game::NameMark::Red => pk::STATUS_RED,
            c3_game::NameMark::Black => pk::STATUS_BLACK,
        }
}

/// Player `p`'s guild and rank (TQ's rank numbers, as the client shows them).
fn guild_of(sim: &Sim, p: &Player) -> Option<(u32, u8)> {
    let (id, rank) = sim.world_state().guild_of(&p.name)?;
    Some((
        id & 0x00FF_FFFF,
        u8::try_from(rank.clamp(0, 255)).unwrap_or(0),
    ))
}

/// Player `p`'s weapon skills by weapon subtype: level and experience (the rules keep them
/// as attributes `weapon_<type>` and `weapon_exp_<type>`).
fn weapon_skills(p: &Player) -> BTreeMap<u32, (u16, u32)> {
    let attr = |name: String| p.attributes.get(&name).copied().unwrap_or(0);
    (p.attributes.iter())
        .filter_map(|(name, level)| {
            let kind: u32 = name.strip_prefix("weapon_")?.parse().ok()?;
            let level = u16::try_from((*level).clamp(0, 0xFFFF)).unwrap_or(0);
            let exp = u32::try_from(attr(format!("weapon_exp_{kind}")).max(0)).unwrap_or(0);
            (level > 0).then_some((kind, (level, exp)))
        })
        .collect()
}

/// Learned skill `skill` of player `p` as MsgMagicInfo: its level and experience, which the
/// rules keep as attributes `magic_<type>` and `magic_exp_<type>`.
fn magic_info(p: &Player, skill: u32) -> Vec<u8> {
    let attr = |name: String| p.attributes.get(&name).copied().unwrap_or(0);
    let level = u16::try_from(attr(format!("magic_{skill}")).clamp(0, 0xFFFF)).unwrap_or(0);
    let exp = u32::try_from(attr(format!("magic_exp_{skill}")).max(0)).unwrap_or(0);
    pk::magic_info(u16::try_from(skill).unwrap_or(0), level, exp)
}

/// Stored item `i` as a warehouse list's entry.
fn stored(i: &c3_game::Item, sim: &Sim) -> pk::Stored {
    let names = &sim.settings().item_attributes;
    let attr = |name: &str| {
        (names.iter().position(|n| n == name))
            .and_then(|at| i.attrs.get(at))
            .copied()
            .unwrap_or(0)
    };
    let byte = |name: &str| u8::try_from(attr(name).clamp(0, 255)).unwrap_or(0);
    pk::Stored {
        id: i.id,
        kind: i.kind,
        ident: byte("ident"),
        gem1: byte("gem1"),
        gem2: byte("gem2"),
        plus: byte("plus"),
        bless: byte("bless"),
        enchant: u16::try_from(attr("enchant").clamp(0, 0xFFFF)).unwrap_or(0),
        color: byte("color"),
    }
}

/// The most one CP-shop buy is for (a full bag of 40).
const BUY_MOST: u32 = 40;

/// Item `i` as MsgItemInfo: its durability in hundredths (or how many are stacked), and its
/// numbers by the settings' item attribute names (Conquer's: plus, gems, bless...).
fn item_info(i: &c3_game::Item, position: u8, action: u8, sim: &Sim) -> Vec<u8> {
    pk::item_info(&item_fields(i, position, action, sim))
}

/// Item `i`'s fields as the item packets carry them.
fn item_fields(i: &c3_game::Item, position: u8, action: u8, sim: &Sim) -> pk::ItemInfo {
    let t = sim.rules().item(i.kind);
    let names = &sim.settings().item_attributes;
    let attr = |name: &str| {
        (names.iter().position(|n| n == name))
            .and_then(|at| i.attrs.get(at))
            .copied()
            .unwrap_or(0)
    };
    let byte = |name: &str| u8::try_from(attr(name).clamp(0, 255)).unwrap_or(0);
    let stacked = t.is_some_and(|t| t.stack > 1);
    let (amount, limit) = match t {
        Some(t) if stacked => (i.count, t.stack),
        // Its own max durability (Kylin gems, repairs' decay), else its type's.
        Some(t) => {
            let max = u32::try_from(attr("max_dur")).ok().filter(|&m| m > 0);
            (i.durability * 100, max.unwrap_or(t.durability) * 100)
        }
        None => (1, 1),
    };
    let short = |v: u32| u16::try_from(v).unwrap_or(u16::MAX);
    pk::ItemInfo {
        id: i.id,
        kind: i.kind,
        amount: short(amount),
        limit: short(limit),
        action,
        ident: byte("ident"),
        position,
        data: u32::try_from(attr("data").max(0)).unwrap_or(0),
        gem1: byte("gem1"),
        gem2: byte("gem2"),
        plus: byte("plus"),
        bless: byte("bless"),
        enchant: byte("enchant"),
        color: u16::try_from(attr("color").clamp(0, i32::from(u16::MAX))).unwrap_or(0),
        lock: lock_status(i.lock),
    }
}

/// An item's lock as the client knows it: 0 free, 1 locked, 3 unlocking.
fn lock_status(lock: u32) -> u16 {
    match lock {
        0 => 0,
        1 => 1,
        _ => 3,
    }
}

/// Client ids of booths (TQ's dynamic NPC range 100001-199999), one per owner.
const BOOTH_IDS: u32 = 100_001;

fn booth_id(owner: EntityId) -> u32 {
    BOOTH_IDS + owner.0 % 99_999
}

/// The booth flag NPCs, where players open booths (Role.h _ROLE_BOOTH_FLAG_NPC).
const BOOTH_FLAG: u32 = 16;

/// How player `owner`'s booth changed for a client (booth_trade spec §1.1, §1.4): its NPC
/// shown (with its cry) or gone, wares gone; to the owner itself, wares set out echoed.
pub(super) fn booth_changes(
    (owner, name): (EntityId, &str),
    old: Option<&c3_game::Booth>,
    new: Option<&c3_game::Booth>,
    to_owner: bool,
) -> Vec<Vec<u8>> {
    let id = booth_id(owner);
    let mut out = Vec::new();
    match (old, new) {
        (_, Some(b)) if old.is_none_or(|o| o.cell != b.cell) => {
            let look = pk::BOOTH_LOOK + u16::from(b.direction % 8);
            out.push(pk::npc_info(id, cell16(b.cell), look, pk::BOOTH_NPC, name));
            if !b.cry.is_empty() {
                out.push(pk::talk(channel::CRY, name, "ALLUSERS", &b.cry, WHITE));
            }
        }
        (Some(_), None) => out.push(pk::npc_gone(id)),
        _ => {}
    }
    let before = old.map_or(&[][..], |b| &b.wares);
    let now = new.map_or(&[][..], |b| &b.wares);
    let listed = |wares: &[c3_game::Ware], item: u32| wares.iter().any(|w| w.item.id == item);
    if new.is_some() {
        for w in before.iter().filter(|w| !listed(now, w.item.id)) {
            out.push(pk::item_action(w.item.id, id, item::BOOTH_DEL));
        }
    }
    if to_owner {
        for w in now.iter().filter(|w| !listed(before, w.item.id)) {
            let add = if w.currency.is_empty() {
                item::BOOTH_ADD
            } else {
                item::BOOTH_ADD_CPS
            };
            let price = u32::try_from(w.price).unwrap_or(u32::MAX);
            out.push(pk::item_action(w.item.id, price, add));
        }
    }
    out
}

/// NPC `n` as MsgNpcInfo: its look with its facing.
/// Trap `t` as a map item: placed (`MAP_ITEM_TRAP`) or gone (`MAP_ITEM_TRAP_GONE`).
fn trap_item(t: &c3_scene::Trap, action: u8) -> Vec<u8> {
    pk::map_item(TRAP_IDS + t.id, t.look, cell16(t.cell), action)
}

/// Client ids of traps start here (TQ's trap ids).
const TRAP_IDS: u32 = 900_000_000;

/// How NPC `n` shows: with its life (`hit_points`) when it can be hit.
fn npc_info(n: &c3_scene::Npc, hit_points: Option<(u32, u32)>) -> Vec<u8> {
    let look = (n.look * 10 + u32::from(n.direction % 8)) as u16;
    match hit_points {
        Some(hp) => {
            let sort = (n.extra.get("sort").and_then(|s| s.trim().parse().ok())).unwrap_or(0);
            pk::npc_info_ex(n.id, hp, cell16(n.cell), look, n.kind as u16, sort, &n.name)
        }
        None => pk::npc_info(n.id, cell16(n.cell), look, n.kind as u16, &n.name),
    }
}

fn slot_of(rules: &dyn Rules, item: &c3_game::Item) -> u8 {
    item.slot(rules.item(item.kind)).unwrap_or(0)
}

/// How `state` comes into the client's view.
fn spawn(state: &EntityState, ids: &mut Ids, sim: &Sim) -> Vec<u8> {
    let rules = sim.rules();
    let id = ids.of(state);
    match state {
        EntityState::Player(p) => {
            let look = player_look(p);
            pk::spawn(&Spawn {
                id,
                look: look_face(p, rules),
                status: status(p, rules),
                gear: [look.armet, look.armor, look.r_weapon, look.l_weapon],
                life: p.life.min(u16::MAX.into()) as u16,
                level: p.level as u16,
                cell: cell16(p.cell),
                hair: look.hair as u16,
                dir: p.direction,
                monster: false,
                guild: guild_of(sim, p).map_or(0, |(g, rank)| g | u32::from(rank) << 24),
                name: &p.name,
            })
        }
        EntityState::Monster(m) => {
            let (name, look) = rules
                .monster(m.kind)
                .map_or(("Monster", 0), |t| (t.name.as_str(), t.look));
            pk::spawn(&Spawn {
                id,
                look,
                status: if m.is_alive() { 0 } else { pk::STATUS_DEAD },
                gear: [0; 4],
                life: m.life.min(u16::MAX.into()) as u16,
                level: m.level as u16,
                cell: cell16(m.cell),
                hair: 0,
                dir: m.direction,
                monster: true,
                guild: 0,
                name,
            })
        }
        EntityState::Item(i) => {
            let kind = i.kind.unwrap_or_else(|| pk::money_look(i.money));
            pk::map_item(id, kind, cell16(i.cell), 1)
        }
    }
}

/// How `state` leaves the client's view.
fn despawn(state: &EntityState, ids: &mut Ids) -> Vec<u8> {
    let id = ids.of(state);
    match state {
        EntityState::Item(i) => pk::map_item(id, i.kind.unwrap_or(0), cell16(i.cell), 2),
        EntityState::Player(p) => pk::action(id, action::LEAVE_MAP, 0, cell16(p.cell), 0),
        EntityState::Monster(m) => pk::action(id, action::LEAVE_MAP, 0, cell16(m.cell), 0),
    }
}

/// Whether a change of `old` into `new` needs a fresh spawn (looks or life and death).
fn respawns(old: &EntityState, new: &EntityState) -> bool {
    match (old, new) {
        (EntityState::Player(a), EntityState::Player(b)) => {
            a.equipment != b.equipment || a.is_alive() != b.is_alive()
        }
        (EntityState::Monster(a), EntityState::Monster(b)) => {
            a.is_alive() != b.is_alive() && b.is_alive()
        }
        (EntityState::Item(_), EntityState::Item(_)) => false,
        _ => true,
    }
}

fn cell_of(state: &EntityState) -> CellPos {
    match state {
        EntityState::Player(p) => p.cell,
        EntityState::Monster(m) => m.cell,
        EntityState::Item(i) => i.cell,
    }
}

fn life_of(state: &EntityState) -> u32 {
    match state {
        EntityState::Player(p) => p.life,
        EntityState::Monster(m) => m.life,
        EntityState::Item(_) => 0,
    }
}

#[derive(Default)]
pub struct ConquerClient {
    hero: Option<EntityId>,
    /// The map's id in the client's `GameMap.dat`.
    map_id: u32,
    npcs: Vec<Npc>,
    /// The most life and life of the NPCs that can be hit, by id.
    npc_hit_points: HashMap<u32, (u32, u32)>,
    /// The map's traps.
    traps: Vec<c3_scene::Trap>,
    /// Whether the client loaded the map (until then, spawns wait).
    entered: bool,
    /// The hero as last shown, and its attribute points.
    me: Option<Player>,
    points: Points,
    /// Others in view, with the packet that shows each.
    seen: BTreeMap<EntityId, (EntityState, Vec<u8>)>,
    /// Where the client last saw each role.
    cells: HashMap<EntityId, CellPos>,
    /// Where the client walked its hero to, ahead of the server.
    steps_to: Option<CellPos>,
    /// The hero's team as last shown.
    team: Vec<c3_game::Mate>,
    /// The hero's guild, rank, fund and member count as last shown (`Some(None)`: shown
    /// as none).
    guild: Option<Option<GuildShown>>,
    /// The hero's guild's announcement as last told.
    notice: Option<String>,
    /// The ids the client knows friends and enemies by, by name; and what it was told of
    /// each: online or not (`None`: not told yet).
    contact_ids: BTreeMap<String, u32>,
    friends_told: Option<BTreeMap<String, bool>>,
    enemies_told: Option<BTreeMap<String, bool>>,
    /// The guild names the client was told, by id.
    guild_names: BTreeMap<u32, String>,
    /// The hero's guild's members (by rank, then level): name, level, online.
    roster: Vec<(String, u32, bool)>,
    /// The guild of each player in view that has one.
    seen_guilds: HashMap<EntityId, u32>,
    /// The owner of the booth whose goods the client asked for.
    booth_asked: Option<EntityId>,
    /// The player whose equipment the client asked to see.
    observe_asked: Option<EntityId>,
    /// Whether the client was told CPs wait for it (MsgInteract 31, once a login).
    withdraw_told: bool,
    /// The sash whose items the client asked for, and the item whose unlock day.
    chest_asked: Option<u32>,
    lock_asked: Option<u32>,
    /// The hero's mentor and students as the client was last told (`None`: not yet), and
    /// whether it asked for them or for what students gave.
    mentors_told: Option<(String, Vec<String>)>,
    mentors_asked: bool,
    prize_asked: bool,
    /// A nobility request answered with the next update, and the hero's standing as last
    /// told (donation, rank, place).
    peerage_asked: Option<(u16, u64)>,
    noble_told: Option<(u64, u32, i64)>,
    /// A broadcast request answered with the next update: its action and data.
    pigeon_asked: Option<(u8, u32)>,
    /// The trade partners the client was told of: online or not (`None`: none told yet).
    partners_told: Option<BTreeMap<String, bool>>,
    /// A message board asked about: its channel, the first post of the page, and whose post
    /// to read (empty: the page).
    board_asked: Option<(u16, u16, String)>,
    /// Words friends left, given before the client loaded its map (shown once it has, as
    /// TQ gives them on MsgAction 76).
    held_words: Vec<Vec<u8>>,
}

impl ConquerClient {
    fn hero_id(&self) -> u32 {
        self.hero.map_or(0, Ids::player)
    }

    /// The player in view (or the hero) keeping booth `id`.
    fn booth_owner(&self, id: u32) -> Option<&Player> {
        let seen = self.seen.values().filter_map(|(s, _)| match s {
            EntityState::Player(p) => Some(&**p),
            _ => None,
        });
        (seen.chain(self.me.as_ref())).find(|p| p.booth.is_some() && booth_id(p.id) == id)
    }

    /// Nobility for the client (MsgPeerage, adapters spec §2.3): the hero's standing when it
    /// changed, and the answer to what it asked.
    fn nobility(&mut self, sim: &Sim) -> Vec<Vec<u8>> {
        use pk::peerage as p;
        let Some(me) = self.me.as_ref() else {
            return Vec::new();
        };
        let (world, n) = (sim.world_state(), &sim.settings().nobility);
        if n.min_level == 0 {
            return Vec::new();
        }
        let id = self.hero_id();
        let mine = world.donations.get(&me.name).map_or(0, |d| d.total);
        let (rank, place) = world.noble_rank(n, &me.name);
        let place = place.map_or(-1, |p| p as i64);
        let mut out = Vec::new();
        if self.noble_told != Some((mine, rank, place)) {
            self.noble_told = Some((mine, rank, place));
            let line = format!("{id} {mine} {rank} {place}");
            out.push(pk::peerage(p::UPDATE, u64::from(id), &[&line]));
        }
        match self.peerage_asked.take() {
            Some((p::DONATE, amount)) => {
                let line = format!("{id} {rank} {amount} {mine} {place}");
                out.push(pk::peerage(p::DONATE, u64::from(id), &[&line]));
            }
            Some((p::PAGE, page)) => {
                let nobles = world.nobles(n);
                let pages = nobles.len().min(50).div_ceil(p::PAGE_SIZE) as u64;
                let first = page as usize * p::PAGE_SIZE;
                let lines: Vec<String> = (nobles.iter().enumerate().skip(first).take(p::PAGE_SIZE))
                    .map(|(at, noble)| {
                        let online = sim.players().any(|q| q.name == noble.name);
                        let user = self.contact_id_of(&noble.name);
                        let (name, total, rank) = (&noble.name, noble.total, noble.rank);
                        format!("{user} {} 0 {name} {total} {rank} {at}", u8::from(online))
                    })
                    .collect();
                let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
                out.push(pk::peerage(p::PAGE, page | pages << 16, &refs));
            }
            Some((p::QUERY_RANK, wanted)) => {
                let price = world.noble_price(n, u32::try_from(wanted).unwrap_or(0), mine);
                out.push(pk::peerage(p::QUERY_RANK, price, &[]));
            }
            _ => {}
        }
        out
    }

    /// The id the client knows a player by when it may be offline (a contact id).
    fn contact_id_of(&self, name: &str) -> u32 {
        self.contact_ids.get(name).copied().unwrap_or(0)
    }

    /// A broadcast request's answer: a page of the waiting lines (all, or the hero's), or
    /// where its line now waits (MsgPigeon / MsgPigeonInfo).
    fn pigeon_reply(&self, sim: &Sim, ids: &mut Ids, action: u8, data: u32) -> Vec<Vec<u8>> {
        use pk::pigeon as p;
        let Some(me) = self.me.as_ref() else {
            return Vec::new();
        };
        let now = chrono::Utc::now().timestamp();
        let queue: Vec<&c3_game::Broadcast> = sim
            .world_state()
            .broadcasts
            .iter()
            .filter(|l| l.at > now)
            .collect();
        let place_of_mine =
            || (queue.iter().rposition(|l| l.author == me.name)).map_or(0, |i| i as u32 + 1);
        match action {
            p::QUERY_QUEUE | p::QUERY_MINE => {
                let first = data as usize * p::PAGE;
                let lines: Vec<pk::PigeonLine> = (queue.iter().enumerate())
                    .filter(|(_, l)| action == p::QUERY_QUEUE || l.author == me.name)
                    .skip(first)
                    .take(p::PAGE)
                    .map(|(place, l)| pk::PigeonLine {
                        id: l.id,
                        owner: (sim.players().find(|q| q.name == l.author))
                            .and_then(|q| self.client_id(q.id, ids))
                            .unwrap_or(0),
                        place: place as u32,
                        name: &l.author,
                        seconds: u32::try_from(l.at - now).unwrap_or(0),
                        text: &l.text,
                    })
                    .collect();
                vec![pk::pigeon_info(data, &lines)]
            }
            p::PUBLISH | p::MOVE_FIRST | p::MOVE_FIVE => vec![pk::pigeon(action, place_of_mine())],
            _ => Vec::new(),
        }
    }

    /// A message board's page of posts from `index`, or `name`'s post read (packets 1111).
    fn board(&self, sim: &Sim, channel: u16, index: u16, name: &str) -> Option<Vec<u8>> {
        let me = self.me.as_ref()?;
        let board = u8::try_from(channel.checked_sub(pk::board::FIRST_CHANNEL)?).ok()?;
        let key = sim.board_key(board, &me.name)?;
        let posts = sim
            .world_state()
            .boards
            .get(&key)
            .map_or(&[][..], Vec::as_slice);
        if !name.is_empty() {
            let post = posts.iter().find(|p| p.author == name)?;
            return Some(pk::board_words(
                channel,
                &post.author,
                &post.text,
                &post.time,
            ));
        }
        let page: Vec<(&str, &str, &str)> = (posts.iter().skip(usize::from(index)))
            .take(pk::board::PAGE)
            .map(|p| (p.author.as_str(), p.text.as_str(), p.time.as_str()))
            .collect();
        Some(pk::board_list(index, channel, &page))
    }

    /// A dice table's news as the 5065 client hears it (dice spec §2.4, §5).
    fn dice_news(&self, npc: u32, news: &c3_game::DiceNews, sim: &Sim) -> Vec<Vec<u8>> {
        use c3_game::DiceNews as N;
        use pk::dice as d;
        let silver = |n: u64| u32::try_from(n).unwrap_or(u32::MAX);
        match news {
            N::Offered => Vec::new(),
            N::Joined { name } => vec![pk::name(d::NAME_JOIN, npc, name)],
            N::Left { name } => vec![pk::name(d::NAME_LEAVE, npc, name)],
            N::Open { seconds } => {
                let seconds = u8::try_from(*seconds).unwrap_or(u8::MAX);
                vec![pk::dice(d::BEGIN, seconds, npc, &[])]
            }
            N::Closed => vec![pk::dice(d::END, 0, npc, &[])],
            N::Chips { bets, mine, cancel } => {
                let action = match (cancel, mine) {
                    (false, false) => d::CHIP_IN,
                    (false, true) => d::CHIP_IN_CONFIRM,
                    (true, false) => d::CANCEL,
                    (true, true) => d::CANCEL_CONFIRM,
                };
                let entries: Vec<(u8, u32)> = bets.iter().map(|&(b, m)| (b, silver(m))).collect();
                let count = u8::try_from(entries.len()).unwrap_or(u8::MAX);
                vec![pk::dice(action, count, npc, &entries)]
            }
            // TQ rolls four dice; the client draws three.
            N::Rolled { dice: [a, b, c] } => vec![pk::dice_roll(npc, [*a, *b, *c, 1])],
            N::Won { name, amount } => vec![pk::name(d::NAME_BONUS, silver(*amount), name)],
            // STR_MISS_BET, whispered by the table's NPC (channel 2000, white).
            N::Missed => {
                let speaker =
                    (sim.npcs().iter().find(|n| n.id == npc)).map_or("", |n| n.name.as_str());
                let hearer = self
                    .hero
                    .and_then(|h| sim.player(h))
                    .map_or("", |p| p.name.as_str());
                let words = "You miss the chance! Keep alert!";
                vec![pk::talk(2000, speaker, hearer, words, WHITE)]
            }
        }
    }

    /// The hero's item `item`'s lock as MsgEquipLock tells it: its unlock day asked, or what
    /// became of it (locked, unlocking, freed).
    fn item_lock(&self, item: u32, asked: bool, sim: &Sim) -> Option<Vec<u8>> {
        let p = sim.player(self.hero?)?;
        let it = (p.bag.iter().chain(&p.equipment)).find(|i| i.id == item)?;
        let status = lock_status(it.lock) as u8;
        let until = if it.lock > 1 { it.lock } else { 0 };
        let action = match (asked, it.lock) {
            (true, _) => pk::item_lock::QUERY,
            (false, 0) => pk::item_lock::UNLOCKED,
            (false, 1) => pk::item_lock::APPLY,
            (false, _) => pk::item_lock::CANCEL,
        };
        Some(pk::item_lock(item, action, status, until))
    }

    /// What the hero's sash `chest` holds, as MsgPackage 30 lists it.
    fn chest(&self, chest: u32, sim: &Sim) -> Vec<u8> {
        let kept: Vec<pk::Stored> = (self.hero.and_then(|h| sim.player(h)?.chests.get(&chest)))
            .into_iter()
            .flatten()
            .map(|i| stored(i, sim))
            .collect();
        pk::package(chest, pk::PACKAGE_CHEST, &kept)
    }

    /// Player `target` as the client's view of it shows it: its MsgPlayer, then each worn
    /// item as that player's (MsgItemInfo 4, packets spec 117 / Redux A15).
    fn observed(&self, target: EntityId, sim: &Sim) -> Vec<Vec<u8>> {
        let Some(p) = sim.player(target) else {
            return Vec::new();
        };
        let mut out: Vec<Vec<u8>> = (self.seen.get(&target).map(|(_, packet)| packet.clone()))
            .into_iter()
            .collect();
        let owner = Ids::player(target);
        for i in &p.equipment {
            let mut info = item_fields(i, slot_of(sim.rules(), i), pk::ITEM_OTHER_EQUIPMENT, sim);
            info.id = owner;
            out.push(pk::item_info(&info));
        }
        out
    }

    /// Booth `owner`'s goods, last first, and its cry (booth_trade spec §1.5).
    fn booth_goods(&self, owner: EntityId, sim: &Sim) -> Vec<Vec<u8>> {
        let Some(p) = sim.player(owner) else {
            return Vec::new();
        };
        let Some(b) = &p.booth else {
            return Vec::new();
        };
        let id = booth_id(owner);
        let mut out: Vec<Vec<u8>> = (b.wares.iter().rev())
            .map(|w| {
                let mode = match w.currency.is_empty() {
                    true => pk::ITEM_EX_BOOTH,
                    false => pk::ITEM_EX_BOOTH_CPS,
                };
                let price = u32::try_from(w.price).unwrap_or(u32::MAX);
                pk::item_info_ex(&item_fields(&w.item, 0, 0, sim), id, price, mode)
            })
            .collect();
        if !b.cry.is_empty() {
            out.push(pk::talk(channel::CRY, &p.name, "ALL", &b.cry, WHITE));
        }
        out
    }

    /// Lets the client in: the login answer, the hero and what it carries.
    pub fn welcome(&mut self, welcome: &Welcome, sim: &Sim) -> Result<Vec<Vec<u8>>, String> {
        let map_id = *welcome
            .map_ids
            .first()
            .ok_or("This map isn't one the Conquer client knows.")?;
        let p = sim
            .player(welcome.hero)
            .ok_or("The character isn't in the world.")?;
        self.hero = Some(welcome.hero);
        self.map_id = map_id;
        self.set_npcs(sim);
        self.traps = sim.traps().to_vec();
        self.cells.insert(p.id, p.cell);
        let look = player_look(p);
        let mut out = vec![
            pk::talk(channel::ENTRANCE, "SYSTEM", "ALLUSERS", "ANSWER_OK", WHITE),
            pk::user_info(&pk::HeroInfo {
                id: Ids::player(p.id),
                look: look_face(p, sim.rules()),
                hair: look.hair as u16,
                money: p.money.min(u32::MAX.into()) as u32,
                exp: p.exp.min(u32::MAX.into()) as u32,
                life: p.life.min(u16::MAX.into()) as u16,
                mana: p.mana.min(u16::MAX.into()) as u16,
                level: p.level.min(255) as u8,
                profession: match p.profession {
                    0 => PROFESSION,
                    chosen => u8::try_from(chosen).unwrap_or(PROFESSION),
                },
                points: points(p).map(|v| v.min(u16::MAX.into()) as u16),
                name: &p.name,
            }),
        ];
        out.extend(p.bag.iter().map(|i| item_info(i, 0, pk::ITEM_ADD, sim)));
        out.extend(
            (p.equipment.iter()).map(|i| item_info(i, slot_of(sim.rules(), i), pk::ITEM_ADD, sim)),
        );
        out.extend(p.skills.iter().map(|s| magic_info(p, *s)));
        out.extend(
            weapon_skills(p)
                .into_iter()
                .map(|(t, (l, x))| pk::weapon_skill(t, l, x)),
        );
        self.points = points(p);
        self.me = Some(p.clone());
        Ok(out)
    }

    /// The hero went to another map (`welcome`'s, simulated by `sim`): the client flies
    /// there, its items swapped for their new ids, and is shown what is around.
    pub fn moved(&mut self, welcome: &Welcome, sim: &Sim) -> Result<Vec<Vec<u8>>, String> {
        let map_id = *welcome
            .map_ids
            .first()
            .ok_or("That map isn't one the Conquer client knows.")?;
        let p = sim
            .player(welcome.hero)
            .ok_or("The character isn't in the world.")?
            .clone();
        let mut out: Vec<Vec<u8>> = self
            .me
            .iter()
            .flat_map(|old| old.bag.iter().chain(&old.equipment))
            .map(|i| pk::item_action(i.id, 0, item::REMOVE))
            .collect();
        out.extend(p.bag.iter().map(|i| item_info(i, 0, pk::ITEM_ADD, sim)));
        out.extend(
            (p.equipment.iter()).map(|i| item_info(i, slot_of(sim.rules(), i), pk::ITEM_ADD, sim)),
        );
        out.extend(p.skills.iter().map(|s| magic_info(&p, *s)));
        out.extend(
            weapon_skills(&p)
                .into_iter()
                .map(|(t, (l, x))| pk::weapon_skill(t, l, x)),
        );
        self.hero = Some(welcome.hero);
        self.map_id = map_id;
        self.set_npcs(sim);
        self.traps = sim.traps().to_vec();
        self.seen.clear();
        self.cells.clear();
        self.cells.insert(p.id, p.cell);
        self.steps_to = None;
        out.push(pk::action(
            self.hero_id(),
            action::FLY_MAP,
            map_id,
            cell16(p.cell),
            p.direction.into(),
        ));
        self.points = points(&p);
        out.push(pk::attributes(
            self.hero_id(),
            &Self::stats(&p, self.points),
        ));
        self.me = Some(p);
        out.extend(self.surroundings());
        Ok(out)
    }

    /// The client loaded its map: where the hero stands, its bars, and everything around.
    fn enter(&mut self) -> Vec<Vec<u8>> {
        let Some(me) = &self.me else {
            return Vec::new();
        };
        self.entered = true;
        let id = self.hero_id();
        let mut out = vec![
            pk::action(
                id,
                action::ENTER_MAP,
                self.map_id,
                cell16(me.cell),
                me.direction.into(),
            ),
            pk::attributes(id, &Self::stats(me, self.points)),
        ];
        out.extend(self.surroundings());
        out.append(&mut self.held_words);
        out
    }

    /// The map's NPCs, with the life of those that can be hit.
    fn set_npcs(&mut self, sim: &Sim) {
        self.npcs = sim.npcs().to_vec();
        self.npc_hit_points = (self.npcs.iter())
            .filter_map(|n| Some((n.id, sim.npc_hit_points(n)?)))
            .collect();
    }

    /// The map's NPCs and every role in view.
    fn surroundings(&self) -> Vec<Vec<u8>> {
        let npcs = (self.npcs.iter()).map(|n| npc_info(n, self.npc_hit_points.get(&n.id).copied()));
        let traps = self.traps.iter().map(|t| trap_item(t, pk::MAP_ITEM_TRAP));
        (npcs.chain(traps))
            .chain(self.seen.values().map(|(_, packet)| packet.clone()))
            .collect()
    }

    /// Every bar and number of the hero.
    fn stats(p: &Player, [strength, agility, vitality, spirit]: Points) -> Vec<(u32, u64)> {
        vec![
            (attr::STRENGTH, strength.into()),
            (attr::AGILITY, agility.into()),
            (attr::VITALITY, vitality.into()),
            (attr::SPIRIT, spirit.into()),
            (attr::LIFE, p.life.into()),
            (attr::MAX_LIFE, p.stats.max_life.into()),
            (attr::MANA, p.mana.into()),
            (attr::MAX_MANA, p.stats.max_mana.into()),
            (attr::MONEY, p.money),
            (attr::EXP, p.exp),
            (attr::XP, xp(p)),
            (attr::LEVEL, p.level.into()),
        ]
    }

    /// What changed on the hero, as attribute and item packets.
    fn hero_changes(&self, old: &Player, new: &Player, sim: &Sim) -> Vec<Vec<u8>> {
        let rules = sim.rules();
        let id = self.hero_id();
        let mut booth = Vec::new();
        // A booth opened: its id echoed, the client then sits its hero (CBoothManager::Open).
        if let (None, Some(b)) = (&old.booth, &new.booth) {
            let dir = u16::from(b.direction);
            let data = booth_id(new.id);
            booth.push(pk::action(
                id,
                action::CREATE_BOOTH,
                data,
                cell16(b.cell),
                dir,
            ));
        }
        booth.extend(booth_changes(
            (new.id, &new.name),
            old.booth.as_ref(),
            new.booth.as_ref(),
            true,
        ));
        // Skills learned, or whose level or experience changed.
        let changed = (new.skills.iter())
            .filter(|s| !old.skills.contains(s) || magic_info(old, **s) != magic_info(new, **s));
        let mut out = booth;
        out.extend(changed.map(|s| magic_info(new, *s)));
        let before = weapon_skills(old);
        out.extend(
            (weapon_skills(new).into_iter())
                .filter(|(t, now)| before.get(t) != Some(now))
                .map(|(t, (l, x))| pk::weapon_skill(t, l, x)),
        );
        let changed: Vec<(u32, u64)> = Self::stats(new, points(new))
            .into_iter()
            .zip(Self::stats(old, self.points))
            .filter(|(n, o)| n != o)
            .map(|(n, _)| n)
            .collect();

        if !changed.is_empty() {
            out.push(pk::attributes(id, &changed));
        }
        if status(old, rules) != status(new, rules)
            || old.pk != new.pk
            || old.transform != new.transform
        {
            let pk_points = u64::try_from(new.pk.max(0)).unwrap_or(0);
            let values = [
                (attr::STATUS, status(new, rules).into()),
                (attr::PK, pk_points),
                (attr::LOOK_FACE, look_face(new, rules).into()),
            ];
            out.push(pk::attributes(id, &values));
        }
        let has = |list: &[c3_game::Item], item: u32| list.iter().any(|i| i.id == item);
        for i in &new.equipment {
            let slot = slot_of(rules, i).into();
            if !has(&old.equipment, i.id) {
                out.push(if has(&old.bag, i.id) {
                    pk::item_action(i.id, slot, item::EQUIP)
                } else {
                    item_info(i, slot as u8, pk::ITEM_ADD, sim)
                });
            }
        }
        // Items that changed where they are (an upgrade, a gem, wear).
        let before = |id: u32| old.bag.iter().chain(&old.equipment).find(|i| i.id == id);
        for i in new.bag.iter().chain(&new.equipment) {
            if before(i.id).is_some_and(|o| o != i) {
                let worn = new.equipment.iter().any(|e| e.id == i.id);
                let slot = if worn { slot_of(rules, i) } else { 0 };
                out.push(item_info(i, slot, pk::ITEM_UPDATE, sim));
            }
        }
        for i in &new.bag {
            if !has(&old.bag, i.id) {
                out.push(if has(&old.equipment, i.id) {
                    pk::item_action(i.id, slot_of(rules, i).into(), item::UNEQUIP)
                } else {
                    item_info(i, 0, pk::ITEM_ADD, sim)
                });
            }
        }
        for i in old.bag.iter().chain(&old.equipment) {
            if !has(&new.bag, i.id) && !has(&new.equipment, i.id) {
                out.push(pk::item_action(i.id, 0, item::REMOVE));
            }
        }
        out
    }

    /// Shows the client what changed around its hero.
    pub fn update(&mut self, update: &WorldUpdate, sim: &Sim, ids: &mut Ids) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        if self.entered {
            out.extend(self.nobility(sim));
        }
        // CPs bought outside the game waiting: the withdraw button (once, after entering).
        if self.entered && !self.withdraw_told {
            self.withdraw_told = true;
            let name = (self.hero)
                .and_then(|h| sim.player(h))
                .map(|p| p.name.clone());
            if name.is_some_and(|n| sim.world_state().deposits.contains_key(&n)) {
                let id = self.hero_id();
                out.push(pk::interact(interact::WITHDRAW_READY, id, id, (0, 0), 0));
            }
        }
        if let Some((action, data)) = self.pigeon_asked.take() {
            out.extend(self.pigeon_reply(sim, ids, action, data));
        }
        if let Some(owner) = self.booth_asked.take() {
            out.extend(self.booth_goods(owner, sim));
        }
        if let Some(target) = self.observe_asked.take() {
            out.extend(self.observed(target, sim));
        }
        if let Some(chest) = self.chest_asked.take() {
            out.push(self.chest(chest, sim));
        }
        if let Some(item) = self.lock_asked.take() {
            out.extend(self.item_lock(item, true, sim));
        }
        if let Some((channel, index, name)) = self.board_asked.take() {
            out.extend(self.board(sim, channel, index, &name));
        }
        let hero = self.hero;
        for state in &update.changed {
            let id = state.id();
            if Some(id) == hero {
                if let EntityState::Player(p) = state {
                    if let (true, Some(old)) = (self.entered, &self.me) {
                        out.extend(self.hero_changes(old, p, sim));
                    }
                    self.points = points(p);
                    self.me = Some((**p).clone());
                }
                continue;
            }
            let packet = spawn(state, ids, sim);
            if let EntityState::Player(p) = state {
                out.extend(self.name_guild(sim, p));
            }
            // Another player's name colour changed: its status.
            if let (Some((EntityState::Player(old), _)), EntityState::Player(p)) =
                (self.seen.get(&id), state)
                && status(old, sim.rules()) != status(p, sim.rules())
                && self.entered
                && let Some(client) = self.client_id(id, ids)
            {
                out.push(pk::attributes(
                    client,
                    &[(attr::STATUS, status(p, sim.rules()).into())],
                ));
            }
            if let EntityState::Player(p) = state
                && self.entered
            {
                let old = match self.seen.get(&id) {
                    Some((EntityState::Player(o), _)) => o.booth.as_ref(),
                    _ => None,
                };
                out.extend(booth_changes((p.id, &p.name), old, p.booth.as_ref(), false));
            }
            let fresh = self
                .seen
                .get(&id)
                .is_none_or(|(old, _)| respawns(old, state));
            if fresh {
                self.cells.insert(id, cell_of(state));
                if self.entered {
                    out.push(packet.clone());
                }
            }
            self.seen.insert(id, (state.clone(), packet));
        }
        for id in &update.gone {
            if let Some((old, _)) = self.seen.remove(id) {
                self.cells.remove(id);
                if self.entered {
                    if let EntityState::Player(p) = &old
                        && p.booth.is_some()
                    {
                        out.push(pk::npc_gone(booth_id(p.id)));
                    }
                    out.push(despawn(&old, ids));
                }
            }
        }
        if self.entered {
            out.extend(self.team_changes(sim));
            out.extend(self.guild_changes(sim));
            out.extend(self.contact_changes(sim));
            out.extend(self.partner_changes(sim));
            out.extend(self.mentor_changes(sim));
            let mut shown = Vec::new();
            for (at, event) in update.events.iter().enumerate() {
                if shown.contains(&at) {
                    continue;
                }
                if let Event::Cast { .. } = event {
                    let (packets, hits) = self.cast(event, &update.events[at + 1..], ids);
                    shown.extend(hits.into_iter().map(|hit| at + 1 + hit));
                    out.extend(packets);
                } else {
                    out.extend(self.event(event, ids, sim));
                }
            }
        } else {
            let words = (update.events.iter())
                .filter(|e| {
                    matches!(
                        e,
                        Event::Chat {
                            channel: c3_game::ChatChannel::LeaveWord,
                            ..
                        }
                    )
                })
                .collect::<Vec<_>>();
            for event in words {
                let packets = self.event(event, ids, sim);
                self.held_words.extend(packets);
            }
        }
        out
    }

    /// The name of player `p`'s guild, the first time the client meets it.
    fn name_guild(&mut self, sim: &Sim, p: &Player) -> Option<Vec<u8>> {
        let Some((id, _)) = guild_of(sim, p) else {
            self.seen_guilds.remove(&p.id);
            return None;
        };
        self.seen_guilds.insert(p.id, id);
        let name = &sim.world_state().guilds.get(&id)?.name;
        if self.guild_names.get(&id) == Some(name) {
            return None;
        }
        self.guild_names.insert(id, name.clone());
        Some(pk::name(pk::NAME_GUILD, id, name))
    }

    /// The guild of player `e` in view.
    fn guild_of_seen(&self, e: EntityId) -> Option<u32> {
        self.seen_guilds.get(&e).copied()
    }

    /// The id the client knows friend or enemy `name` by (made up as first met).
    fn contact_id(&mut self, name: &str) -> u32 {
        let next = CONTACT_IDS + self.contact_ids.len() as u32;
        *self.contact_ids.entry(name.to_owned()).or_insert(next)
    }

    /// The friend or enemy the client knows by `id`.
    fn contact_name(&self, id: u32) -> Option<String> {
        (self.contact_ids.iter())
            .find(|(_, i)| **i == id)
            .map(|(n, _)| n.clone())
    }

    /// The hero's friends and enemies, as they changed: all of them at first (GET_INFO,
    /// ENEMY_ADD), then those made, lost, coming online or leaving (MsgFriend.cpp).
    fn contact_changes(&mut self, sim: &Sim) -> Vec<Vec<u8>> {
        use pk::friend as f;
        let Some(p) = self.hero.and_then(|h| sim.player(h)) else {
            return Vec::new();
        };
        let world = sim.world_state();
        let online =
            |n: &String| world.online.contains_key(n) || sim.players().any(|q| &q.name == n);
        let now = |names: Vec<String>| -> BTreeMap<String, bool> {
            names.into_iter().map(|n| (n.clone(), online(&n))).collect()
        };
        let friends = now(world.friends_of(&p.name));
        let enemies = now(world.enemies_of(&p.name).to_vec());
        let mut out = Vec::new();
        let lists = [
            (
                friends,
                self.friends_told.take(),
                [f::GET_INFO, f::ACCEPT, f::BREAK, f::ONLINE, f::OFFLINE],
            ),
            (
                enemies,
                self.enemies_told.take(),
                [
                    f::ENEMY_ADD,
                    f::ENEMY_ADD,
                    f::ENEMY_DEL,
                    f::ENEMY_ONLINE,
                    f::ENEMY_OFFLINE,
                ],
            ),
        ];
        let mut told = Vec::new();
        for (now, before, [first, added, gone, on, off]) in lists {
            let level = |n: &str| world.levels.get(n).copied().unwrap_or(0);
            for (name, &is_on) in &now {
                let action = match before.as_ref().map(|b| b.get(name)) {
                    None => first,
                    Some(None) => added,
                    Some(Some(&was)) if was != is_on => {
                        if is_on {
                            on
                        } else {
                            off
                        }
                    }
                    Some(Some(_)) => continue,
                };
                let id = self.contact_id(name);
                out.push(pk::friend(action, id, is_on, level(name), name));
            }
            for name in before
                .iter()
                .flat_map(|b| b.keys())
                .filter(|n| !now.contains_key(*n))
            {
                let id = self.contact_id(name);
                out.push(pk::friend(gone, id, false, 0, name));
            }
            told.push(now);
        }
        self.enemies_told = told.pop();
        self.friends_told = told.pop();
        out
    }

    /// The hero's mentor and students when they changed or the client asked (MsgMentorMember
    /// 2066: the mentor, each student, the end), and what its students gave when asked.
    fn mentor_changes(&mut self, sim: &Sim) -> Vec<Vec<u8>> {
        use pk::mentor as m;
        let Some(p) = self.hero.and_then(|h| sim.player(h)) else {
            return Vec::new();
        };
        let world = sim.world_state();
        let mentor = world
            .mentors
            .get(&p.name)
            .map(|l| l.mentor.clone())
            .unwrap_or_default();
        let students = world.students_of(&p.name);
        let now = (mentor.clone(), students.clone());
        let mut out = Vec::new();
        if std::mem::take(&mut self.prize_asked) {
            let pool = world.mentor_pools.get(&p.name).copied().unwrap_or_default();
            let short = |v: i64| u16::try_from(v.max(0)).unwrap_or(u16::MAX);
            let gifts = (
                u32::try_from(pool.time.max(0)).unwrap_or(u32::MAX),
                short(pool.bless),
                short(pool.items),
            );
            out.push(pk::mentor_prize(m::AVAILABLE, self.hero_id(), gifts));
        }
        if self.mentors_told.as_ref() == Some(&now) && !std::mem::take(&mut self.mentors_asked) {
            return out;
        }
        self.mentors_told = Some(now);
        let hero = self.hero_id();
        let online = |n: &str| world.online.contains_key(n) || sim.players().any(|q| q.name == n);
        let level =
            |n: &str| u8::try_from(world.levels.get(n).copied().unwrap_or(0)).unwrap_or(u8::MAX);
        if !mentor.is_empty() {
            let id = self.contact_id(&mentor);
            out.push(pk::mentor_member(&pk::MentorMember {
                action: m::MEMBER_TEACHER,
                identity: hero,
                member: id,
                level: level(&mentor),
                profession: 0,
                pk: 0,
                online: online(&mentor),
                gifts: (0, 0, 0),
                names: [&mentor, &p.name, ""],
            }));
        }
        for student in &students {
            let given = (world.mentor_gifts.get(&(p.name.clone(), student.clone())))
                .copied()
                .unwrap_or_default();
            let short = |v: i64| u16::try_from(v.max(0)).unwrap_or(u16::MAX);
            let gifts = (
                u32::try_from(given.time.max(0)).unwrap_or(u32::MAX),
                short(given.bless),
                short(given.items),
            );
            let id = self.contact_id(student);
            out.push(pk::mentor_member(&pk::MentorMember {
                action: m::MEMBER_STUDENT,
                identity: hero,
                member: id,
                level: level(student),
                profession: 0,
                pk: 0,
                online: online(student),
                gifts,
                names: [&p.name, student, ""],
            }));
        }
        out.push(pk::mentor_member(&pk::MentorMember {
            action: m::MEMBER_STUDENT_END,
            identity: hero,
            member: 0,
            level: 0,
            profession: 0,
            pk: 0,
            online: false,
            gifts: (0, 0, 0),
            names: ["", "", ""],
        }));
        out
    }

    /// The hero's trade partners as they come, go online or offline, or end (MsgBusiness: the
    /// list at login with each one's trial minutes, social spec §3).
    fn partner_changes(&mut self, sim: &Sim) -> Vec<Vec<u8>> {
        use pk::business as b;
        let Some(p) = self.hero.and_then(|h| sim.player(h)) else {
            return Vec::new();
        };
        let world = sim.world_state();
        let probation = sim.settings().partner_probation_secs;
        let now_secs = chrono::Utc::now().timestamp();
        let online =
            |n: &String| world.online.contains_key(n) || sim.players().any(|q| &q.name == n);
        let partners = world.partners_of(&p.name);
        let now: BTreeMap<String, bool> = partners
            .iter()
            .map(|(n, _)| (n.clone(), online(n)))
            .collect();
        let before = self.partners_told.take();
        let mut out = Vec::new();
        for (name, since) in &partners {
            let is_on = now[name];
            let action = match before.as_ref().map(|b| b.get(name)) {
                None | Some(None) => b::ADD,
                Some(Some(&was)) if was != is_on => {
                    if is_on {
                        b::ONLINE
                    } else {
                        b::OFFLINE
                    }
                }
                Some(Some(_)) => continue,
            };
            let minutes = ((since + probation - now_secs).max(0) / 60).min(i64::from(u16::MAX));
            let look = sim
                .players()
                .find(|q| &q.name == name)
                .map_or(0, |q| look_face(q, sim.rules()));
            let id = self.contact_id(name);
            out.push(pk::business(action, id, is_on, minutes as u16, look, name));
        }
        for name in before
            .iter()
            .flat_map(|b| b.keys())
            .filter(|n| !now.contains_key(*n))
        {
            let id = self.contact_id(name);
            out.push(pk::business(b::BREAK, id, false, 0, 0, name));
        }
        self.partners_told = Some(now);
        out
    }

    /// The hero's guild, when it changed: its details and name.
    fn guild_changes(&mut self, sim: &Sim) -> Vec<Vec<u8>> {
        let Some(p) = self.hero.and_then(|h| sim.player(h)) else {
            return Vec::new();
        };
        let world = sim.world_state();
        let guilds = &world.guilds;
        self.roster = guild_of(sim, p)
            .and_then(|(id, _)| guilds.get(&id))
            .map(|g| {
                let mut members: Vec<(String, u32, bool, i64)> = (g.members.iter())
                    .map(|(name, &rank)| {
                        let level = world.levels.get(name).copied().unwrap_or(0);
                        (name.clone(), level, world.online.contains_key(name), rank)
                    })
                    .collect();
                members.sort_by(|a, b| b.3.cmp(&a.3).then(b.1.cmp(&a.1)));
                members.into_iter().map(|(n, l, o, _)| (n, l, o)).collect()
            })
            .unwrap_or_default();
        let info = guild_of(sim, p).and_then(|(id, rank)| {
            let g = guilds.get(&id)?;
            let given = g.contributions.get(&p.name).copied().unwrap_or(0);
            Some(pk::GuildInfo {
                id,
                contribution: i32::try_from(given).unwrap_or(i32::MAX),
                fund: u32::try_from(g.fund.max(0)).unwrap_or(u32::MAX),
                members: u32::try_from(g.members.len()).unwrap_or(u32::MAX),
                rank,
                leader: &g.leader,
            })
        });
        // The announcement, told again as it changes.
        let notice =
            (info.as_ref()).map_or_else(String::new, |i| guilds[&i.id].announcement.clone());
        let mut out = Vec::new();
        if self.notice.as_ref() != Some(&notice) {
            self.notice = Some(notice.clone());
            if !notice.is_empty() {
                out.push(pk::talk(
                    channel::GUILD_NOTICE,
                    "SYSTEM",
                    "ALLUSERS",
                    &notice,
                    WHITE,
                ));
            }
        }
        let shown = (info.as_ref()).map(|i| (i.id, i.rank, i.fund, i.members, i.contribution));
        if self.guild == Some(shown) {
            return out;
        }
        self.guild = Some(shown);
        let Some(info) = info else {
            out.push(pk::guild_info(&pk::GuildInfo {
                id: 0,
                contribution: 0,
                fund: 0,
                members: 0,
                rank: 0,
                leader: "",
            }));
            return out;
        };
        let name = &guilds[&info.id].name;
        self.guild_names.insert(info.id, name.clone());
        out.push(pk::name(pk::NAME_GUILD, info.id, name));
        out.push(pk::guild_info(&info));
        out
    }

    /// A page of the guild member list after member `after` (`u32::MAX`: the first page).
    fn member_page(&self, after: u32) -> Vec<u8> {
        const PAGE: usize = 10;
        let start = if after == u32::MAX {
            0
        } else {
            after as usize + 1
        };
        let from_online = after == u32::MAX || self.roster.get(after as usize).is_some_and(|m| m.2);
        let pick = |online: bool, from: usize| {
            (self.roster.iter().enumerate().skip(from))
                .filter(move |(_, m)| m.2 == online)
                .map(|(_, m)| format!("{} {} {}", m.0, m.1, u8::from(m.2)))
        };
        let mut lines: Vec<String> = Vec::new();
        if from_online {
            lines.extend(pick(true, start).take(PAGE));
            let more = PAGE - lines.len();
            lines.extend(pick(false, 0).take(more));
        } else {
            lines.extend(pick(false, start).take(PAGE));
        }
        let list: Vec<&str> = lines.iter().map(String::as_str).collect();
        pk::names(pk::NAME_MEMBER_LIST, after, &list)
    }

    /// The hero's team list, when it changed (members, their life); left, an empty team.
    fn team_changes(&mut self, sim: &Sim) -> Vec<Vec<u8>> {
        let Some(hero) = self.hero else {
            return Vec::new();
        };
        let team = sim.team_mates(hero);
        if team == self.team {
            return Vec::new();
        }
        let left = team.is_empty();
        self.team = team;
        if left {
            return vec![pk::team(pk::team::LEAVE, self.hero_id())];
        }
        let short = |n: u32| u16::try_from(n).unwrap_or(u16::MAX);
        let members: Vec<pk::TeamMember> = (self.team.iter())
            .map(|m| pk::TeamMember {
                name: &m.name,
                id: Ids::player(m.id),
                look: sim.player(m.id).map_or(0, |p| look_face(p, sim.rules())),
                life: short(m.life),
                max_life: short(m.max_life),
            })
            .collect();
        vec![pk::team_members(&members)]
    }

    /// The client's spell cast (scrambled MsgInteract fields) as the engine's cast.
    fn cast_input(
        &self,
        ts: u32,
        target: u32,
        cell: (u16, u16),
        data: (u16, u16),
        ids: &Ids,
    ) -> Option<Input> {
        let spell = pk::spell(self.hero_id(), ts, target, cell, data)?;
        let at = if spell.target == self.hero_id() {
            CastAt::Me
        } else if let Some(e) = ids.entity(spell.target) {
            CastAt::Entity(e)
        } else {
            CastAt::Cell((spell.cell.0.into(), spell.cell.1.into()))
        };
        Some(Input::Cast {
            skill: spell.kind.into(),
            at,
        })
    }

    /// A spell cast as one MsgMagicEffect carrying its hits (the `Attacked` events that
    /// follow it, whose indexes in `after` it returns), and the life each target is left.
    fn cast(&self, event: &Event, after: &[Event], ids: &mut Ids) -> (Vec<Vec<u8>>, Vec<usize>) {
        let Event::Cast {
            caster,
            skill,
            cell,
            targets,
        } = event
        else {
            return (Vec::new(), Vec::new());
        };
        let Some(by) = self.client_id(*caster, ids) else {
            return (Vec::new(), Vec::new());
        };
        let mut hits = Vec::new();
        let mut used = Vec::new();
        let mut lives = Vec::new();
        for (i, e) in after.iter().enumerate() {
            if let Event::Attacked {
                attacker,
                target,
                damage,
            } = e
                && attacker == caster
                && targets.contains(target)
                && let Some(t) = self.client_id(*target, ids)
            {
                hits.push((t, *damage));
                used.push(i);
                lives.push(pk::attributes(
                    t,
                    &[(attr::LIFE, self.life(*target).into())],
                ));
            }
        }
        // A spell at one role names it; else its cell.
        let at = match targets.as_slice() {
            [one] if hits.len() <= 1 => self.client_id(*one, ids),
            _ => None,
        }
        .unwrap_or_else(|| {
            let (x, y) = cell16(*cell);
            u32::from(x) | u32::from(y) << 16
        });
        let kind = u16::try_from(*skill).unwrap_or(0);
        let mut out = vec![pk::magic_effect(by, at, kind, 0, &hits)];
        out.extend(lives);
        (out, used)
    }

    /// The client's id of `id` (the hero or a role in view).
    fn client_id(&self, id: EntityId, ids: &mut Ids) -> Option<u32> {
        if Some(id) == self.hero {
            return Some(Ids::player(id));
        }
        self.seen.get(&id).map(|(state, _)| ids.of(state))
    }

    fn state_cell(&self, id: EntityId) -> Option<CellPos> {
        if Some(id) == self.hero {
            return self.me.as_ref().map(|p| p.cell);
        }
        self.seen.get(&id).map(|(s, _)| cell_of(s))
    }

    fn life(&self, id: EntityId) -> u32 {
        if Some(id) == self.hero {
            return self.me.as_ref().map_or(0, |p| p.life);
        }
        self.seen.get(&id).map_or(0, |(s, _)| life_of(s))
    }

    fn system(&self, text: &str) -> Vec<u8> {
        let name = self.me.as_ref().map_or("", |p| p.name.as_str());
        pk::talk(channel::SYSTEM, "SYSTEM", name, text, YELLOW)
    }

    /// What one event looks like to the client.
    fn event(&mut self, event: &Event, ids: &mut Ids, sim: &Sim) -> Vec<Vec<u8>> {
        let hero = self.hero;
        match event {
            // An NPC moved (gone from where it stood, shown where it stands), left or came.
            Event::NpcMoved { npc, cell } => {
                let Some(at) = self.npcs.iter().position(|n| n.id == *npc) else {
                    return Vec::new();
                };
                let old = cell16(self.npcs[at].cell);
                let mut out = vec![pk::action(*npc, action::LEAVE_MAP, 0, old, 0)];
                match cell {
                    Some(cell) => {
                        self.npcs[at].cell = *cell;
                        let hp = self.npc_hit_points.get(npc).copied();
                        out.push(npc_info(&self.npcs[at], hp));
                    }
                    None => {
                        self.npcs.remove(at);
                    }
                }
                out
            }
            Event::NpcArrived(npc) => {
                self.npcs.retain(|n| n.id != npc.id);
                self.npcs.push(npc.clone());
                let hp = sim.npc_hit_points(npc);
                if let Some(hp) = hp {
                    self.npc_hit_points.insert(npc.id, hp);
                }
                vec![npc_info(npc, hp)]
            }
            Event::TrapPlaced(trap) => {
                self.traps.retain(|t| t.id != trap.id);
                self.traps.push(trap.clone());
                vec![trap_item(trap, pk::MAP_ITEM_TRAP)]
            }
            Event::TrapGone(id) => match self.traps.iter().position(|t| t.id == *id) {
                Some(at) => {
                    let trap = self.traps.remove(at);
                    vec![trap_item(&trap, pk::MAP_ITEM_TRAP_GONE)]
                }
                None => Vec::new(),
            },
            Event::Moved { id, to, running } => {
                let (Some(cid), Some(from)) =
                    (self.client_id(*id, ids), self.cells.get(id).copied())
                else {
                    return Vec::new();
                };
                if from == *to {
                    return Vec::new();
                }
                self.cells.insert(*id, *to);
                if Some(*id) == hero && self.steps_to == Some(*to) {
                    self.steps_to = None;
                }
                let dir = path::facing(from, *to).unwrap_or(0);
                vec![pk::walk(cid, dir, *running)]
            }
            Event::Jumped { id, to } | Event::Placed { id, cell: to } => {
                let Some(cid) = self.client_id(*id, ids) else {
                    return Vec::new();
                };
                let from = self.cells.insert(*id, *to).unwrap_or(*to);
                if Some(*id) == hero {
                    self.steps_to = None;
                }
                let flies = matches!(event, Event::Placed { .. }) && Some(*id) == hero;
                if flies {
                    // The client rebuilds the map, dropping every role: show them again.
                    let mut out = vec![pk::action(
                        cid,
                        action::FLY_MAP,
                        self.map_id,
                        cell16(*to),
                        0,
                    )];
                    out.extend(self.surroundings());
                    out
                } else {
                    let data = u32::from(cell16(*to).0) | u32::from(cell16(*to).1) << 16;
                    let dir = path::facing(from, *to).unwrap_or(0);
                    vec![pk::action(
                        cid,
                        action::JUMP,
                        data,
                        cell16(from),
                        dir.into(),
                    )]
                }
            }
            // A hit on an NPC: its life from the world's.
            Event::Attacked {
                attacker,
                target,
                damage,
            } if self.npc_hit_points.contains_key(&target.0) && !self.seen.contains_key(target) => {
                let Some(a) = self.client_id(*attacker, ids) else {
                    return Vec::new();
                };
                let Some(npc) = self.npcs.iter().find(|n| n.id == target.0) else {
                    return Vec::new();
                };
                let Some(hp) = sim.npc_hit_points(npc) else {
                    return Vec::new();
                };
                let cell = cell16(npc.cell);
                self.npc_hit_points.insert(target.0, hp);
                vec![
                    pk::interact(interact::ATTACK, a, target.0, cell, *damage),
                    pk::attributes(target.0, &[(attr::LIFE, hp.1.into())]),
                ]
            }
            Event::Attacked {
                attacker,
                target,
                damage,
            } => {
                let (Some(a), Some(t)) =
                    (self.client_id(*attacker, ids), self.client_id(*target, ids))
                else {
                    return Vec::new();
                };
                let cell = cell16(self.state_cell(*target).unwrap_or_default());
                vec![
                    pk::interact(interact::ATTACK, a, t, cell, *damage),
                    pk::attributes(t, &[(attr::LIFE, self.life(*target).into())]),
                ]
            }
            Event::Died { id, killer } => {
                let Some(t) = self.client_id(*id, ids) else {
                    return Vec::new();
                };
                let k = killer.and_then(|k| self.client_id(k, ids)).unwrap_or(0);
                let cell = cell16(self.state_cell(*id).unwrap_or_default());
                vec![
                    pk::interact(interact::KILL, k, t, cell, 1),
                    pk::attributes(t, &[(attr::STATUS, pk::STATUS_DEAD.into())]),
                ]
            }
            Event::Revived(id) if Some(*id) == hero => {
                let id = self.hero_id();
                vec![pk::attributes(
                    id,
                    &[
                        (attr::STATUS, 0),
                        (attr::LIFE, self.me.as_ref().map_or(0, |p| p.life.into())),
                    ],
                )]
            }
            Event::LeveledUp { player, .. } if Some(*player) == hero => {
                vec![pk::action(self.hero_id(), action::LEVEL_UP, 0, (0, 0), 0)]
            }
            // The client shows its own lines itself; words left for the hero come as said to it.
            Event::Chat {
                player,
                name,
                text,
                channel: on,
                ..
            } if Some(*player) != hero
                || matches!(
                    on,
                    c3_game::ChatChannel::LeaveWord | c3_game::ChatChannel::Broadcast
                ) =>
            {
                let (code, hearer) = match on {
                    c3_game::ChatChannel::Local => (channel::TALK, "ALLUSERS".to_owned()),
                    c3_game::ChatChannel::Whisper => (
                        channel::WHISPER,
                        self.me.as_ref().map(|p| p.name.clone()).unwrap_or_default(),
                    ),
                    c3_game::ChatChannel::Team => (channel::TEAM, "ALLUSERS".to_owned()),
                    c3_game::ChatChannel::Guild => (channel::GUILD, "ALLUSERS".to_owned()),
                    c3_game::ChatChannel::Friend => (channel::FRIEND, "ALLUSERS".to_owned()),
                    c3_game::ChatChannel::LeaveWord => (
                        channel::LEAVE_WORD,
                        self.me.as_ref().map(|p| p.name.clone()).unwrap_or_default(),
                    ),
                    c3_game::ChatChannel::Broadcast => (channel::BROADCAST, "ALLUSERS".to_owned()),
                    c3_game::ChatChannel::Board(n) => (
                        pk::board::FIRST_CHANNEL + u16::from(*n),
                        "ALLUSERS".to_owned(),
                    ),
                };
                let colour = match on {
                    c3_game::ChatChannel::LeaveWord => 0,
                    _ => WHITE,
                };
                vec![pk::talk(code, name, &hearer, text, colour)]
            }
            // The one looked at told who (STR_QUERYEQUIPMENT_HINT).
            Event::Observed { player, viewer } if Some(*player) == hero => {
                let name = sim.player(*viewer).map_or("", |p| p.name.as_str());
                vec![self.system(&format!("{name} is checking your equipment."))]
            }
            Event::Message { text, .. }
            | Event::Notice { text, .. }
            | Event::CommandReply { text, .. } => {
                vec![self.system(text)]
            }
            Event::PlacementAsked { player, look, kind } if Some(*player) == hero => {
                vec![pk::lay_npc(*look, *kind as u16)]
            }
            // A window the game opens: MsgAction 126 with its number.
            Event::OpenWindow { player, dialog } if Some(*player) == hero => {
                let cell = cell16(self.state_cell(*player).unwrap_or_default());
                vec![pk::action(
                    self.hero_id(),
                    action::OPEN_DIALOG,
                    *dialog,
                    cell,
                    0,
                )]
            }
            Event::ClientCue { player, name, args } if Some(*player) == hero => {
                let cell = cell16(self.state_cell(*player).unwrap_or_default());
                let [a, b] = args.map(|v| u32::try_from(v).unwrap_or(0));
                match name.as_str() {
                    pk::cue::ACTION => {
                        vec![pk::action(self.hero_id(), a as u16, b, cell, 0)]
                    }
                    pk::cue::MAP_ITEM => vec![pk::map_item(b, 0, cell, a as u8)],
                    _ => Vec::new(),
                }
            }
            // A proposal to the hero: MsgInteract Court from the proposer.
            Event::Proposed { player, from } if Some(*player) == hero => {
                let Some(f) = self.client_id(*from, ids) else {
                    return Vec::new();
                };
                let cell = cell16(self.state_cell(*from).unwrap_or_default());
                vec![pk::interact(interact::COURT, f, self.hero_id(), cell, 0)]
            }
            // A player's emote, to everyone who sees it (the hero too).
            // Another player asks the hero to be friends (the client says so in its log).
            // A mentor's offer (8, with its battle power) or a would-be student's ask (9).
            Event::MentorAsked {
                player,
                from,
                teach,
            } if Some(*player) == hero => {
                let (Some(who), Some(name)) = (
                    self.client_id(*from, ids),
                    sim.player(*from).map(|q| q.name.clone()),
                ) else {
                    return Vec::new();
                };
                let action = match teach {
                    true => pk::mentor::TEACHER_RESPONSE,
                    false => pk::mentor::STUDENT_RESPONSE,
                };
                vec![pk::mentor(action, who, self.hero_id(), 0, &[&name])]
            }
            Event::PartnerAsked { player, from } if Some(*player) == hero => {
                let (Some(who), Some(name)) = (
                    self.client_id(*from, ids),
                    sim.player(*from).map(|q| q.name.clone()),
                ) else {
                    return Vec::new();
                };
                vec![pk::business(pk::business::APPLY, who, true, 0, 0, &name)]
            }
            Event::FriendAsked { player, from } if Some(*player) == hero => {
                let (Some(who), Some(name)) = (
                    self.client_id(*from, ids),
                    sim.player(*from).map(|q| q.name.clone()),
                ) else {
                    return Vec::new();
                };
                vec![pk::friend(pk::friend::APPLY, who, true, 0, &name)]
            }
            Event::Emoted { player, emote } => {
                let Some(who) = self.client_id(*player, ids) else {
                    return Vec::new();
                };
                let cell = cell16(self.state_cell(*player).unwrap_or_default());
                vec![pk::action(who, action::EMOTION, *emote, cell, 0)]
            }
            // The hero's PK mode, as its main bar's PK button shows it.
            Event::PkModeSet { player, mode } if Some(*player) == hero => {
                let code = pk_mode_code(*mode);
                vec![pk::action(
                    self.hero_id(),
                    action::SET_PK_MODE,
                    code,
                    (0, 0),
                    0,
                )]
            }
            // A wedding: fireworks over the bride or groom, the spouse's name to each.
            Event::Married { player, spouse } => {
                let mut out = Vec::new();
                if let Some(id) = self.client_id(*player, ids) {
                    out.push(pk::item_action(id, 0xFF, pk::item::FIREWORKS));
                }
                for (me, other) in [(*player, *spouse), (*spouse, *player)] {
                    if Some(me) == hero
                        && let Some(name) = sim.player(other).map(|p| p.name.clone())
                    {
                        out.push(pk::name(pk::NAME_MATE, self.hero_id(), &name));
                    }
                }
                out
            }
            Event::ClientCueText {
                player,
                name,
                a,
                text,
            } if Some(*player) == hero && name == pk::cue::NAME => {
                vec![pk::name(u8::try_from(*a).unwrap_or(0), 0, text)]
            }
            Event::RoomCueText {
                player,
                name,
                a,
                text,
            } => match (name.as_str(), self.client_id(*player, ids)) {
                (pk::cue::NAME, Some(_)) => {
                    vec![pk::name(u8::try_from(*a).unwrap_or(0), 0, text)]
                }
                _ => Vec::new(),
            },
            Event::RoomCue { player, name, args } => {
                let Some(id) = self.client_id(*player, ids) else {
                    return Vec::new();
                };
                let [a, b] = args.map(|v| u32::try_from(v).unwrap_or(0));
                match name.as_str() {
                    pk::cue::ITEM_ACTION => vec![pk::item_action(id, b, a as u16)],
                    _ => Vec::new(),
                }
            }
            // The hero's trade: asked, opened, the partner's goods, accepted, done or closed.
            Event::Trade {
                player,
                partner,
                news,
            } if Some(*player) == hero => {
                use c3_game::TradeNews as N;
                let them = self.client_id(*partner, ids).unwrap_or(0);
                let money = |n: &u64| u32::try_from(*n).unwrap_or(u32::MAX);
                match news {
                    N::Asked => vec![pk::trade(them, pk::trade::APPLY)],
                    N::Opened => vec![pk::trade(them, pk::trade::OPEN)],
                    N::PartnerItem(item) => vec![item_info(item, 0, pk::ITEM_TRADE, sim)],
                    N::PartnerMoney(n) => vec![pk::trade(money(n), pk::trade::PARTNER_MONEY)],
                    N::OwnMoney(n) => vec![pk::trade(money(n), pk::trade::OWN_MONEY)],
                    N::PartnerAccepted => vec![pk::trade(them, pk::trade::OK)],
                    N::Refused(item) => vec![pk::trade(*item, pk::trade::ITEM_REFUSED)],
                    N::Done => vec![pk::trade(them, pk::trade::SUCCESS)],
                    N::Closed => vec![pk::trade(them, pk::trade::FAIL)],
                }
            }
            // Invited into a guild: the client asks the hero.
            Event::GuildInvited { player, inviter } if Some(*player) == hero => {
                vec![pk::guild(pk::guild::INVITE_JOIN, Ids::player(*inviter))]
            }
            Event::GuildApplied { player, applicant } if Some(*player) == hero => {
                vec![pk::guild(pk::guild::APPLY_JOIN, Ids::player(*applicant))]
            }
            // Someone asks to join the hero's team: the client asks the hero.
            Event::TeamApplied { player, applicant } if Some(*player) == hero => {
                vec![pk::team(pk::team::APPLY, Ids::player(*applicant))]
            }
            // Invited to a team: the client asks the hero.
            Event::TeamInvited { player, leader } if Some(*player) == hero => {
                vec![pk::team(pk::team::INVITE, Ids::player(*leader))]
            }
            // The warehouse as it is now, and the money kept.
            // A dice table's news (MsgName 17/18/19, MsgDice 1113; dice spec §2.4). The seat
            // offer is the client's own question for a type-19 NPC.
            Event::Dice { player, npc, news } if Some(*player) == hero => {
                self.dice_news(*npc, news, sim)
            }
            // Offline training news (MsgTraining 2044, MsgTrainingInfo 2043).
            Event::Training { player, news } if Some(*player) == hero => {
                use c3_game::TrainingNews as N;
                use pk::training as t;
                let word = |n: u32| u16::try_from(n).unwrap_or(u16::MAX);
                match *news {
                    N::Offer { minutes } => vec![pk::training(t::ASK, minutes)],
                    N::Started => vec![pk::training(t::START, 0)],
                    N::Ready => vec![pk::training(t::READY, 0)],
                    N::Denied => vec![pk::training(t::DENIED, 0)],
                    N::Info {
                        trained,
                        remaining,
                        level,
                        fraction,
                    } => vec![pk::training_info(
                        word(trained),
                        word(remaining),
                        word(level),
                        fraction,
                    )],
                    N::Claimed => Vec::new(),
                }
            }
            // CPs that waited taken (MsgInteract 32 with how many: StrRes 11200).
            Event::Withdrawn { player, amount, .. } if Some(*player) == hero => {
                let id = self.hero_id();
                let amount = u32::try_from(*amount).unwrap_or(u32::MAX);
                vec![pk::interact(interact::WITHDRAW, id, id, (0, 0), amount)]
            }
            // An item locked, set unlocking or freed (MsgEquipLock 2048).
            Event::ItemLocked { player, item } if Some(*player) == hero => {
                self.item_lock(*item, false, sim).into_iter().collect()
            }
            // A sash as it is now (MsgPackage 30 with the sash's id).
            Event::ChestOpened { player, chest } | Event::ChestChanged { player, chest }
                if Some(*player) == hero =>
            {
                vec![self.chest(*chest, sim)]
            }
            Event::StorageOpened { player, npc } | Event::StorageChanged { player, npc }
                if Some(*player) == hero =>
            {
                let Some(p) = sim.player(*player) else {
                    return Vec::new();
                };
                let kept: Vec<pk::Stored> = (p.storage.get(npc).into_iter().flatten())
                    .map(|i| stored(i, sim))
                    .collect();
                let money = [(attr::MONEY, p.money), (attr::SAVED_MONEY, p.saved_money)];
                vec![
                    pk::package(*npc, pk::PACKAGE_STORAGE, &kept),
                    pk::item_action(
                        *npc,
                        u32::try_from(p.saved_money).unwrap_or(u32::MAX),
                        item::SAVED_MONEY,
                    ),
                    pk::attributes(self.hero_id(), &money),
                ]
            }
            Event::CellEffect { cell, effect } => {
                let (x, y) = cell16(*cell);
                vec![pk::name(
                    pk::NAME_MAP_EFFECT,
                    u32::from(x) | u32::from(y) << 16,
                    effect,
                )]
            }
            Event::Effect { entity, effect } => match self.client_id(*entity, ids) {
                Some(id) => vec![pk::name(pk::NAME_ROLE_EFFECT, id, effect)],
                None => Vec::new(),
            },
            // The client opens a shop NPC's goods (its Shop.dat) by itself.
            Event::ShopOpened { .. } => Vec::new(),
            Event::Dialogue { .. } => self.dialog(),
            _ => Vec::new(),
        }
    }

    /// The hero's dialogue as an NPC dialog window.
    fn dialog(&self) -> Vec<Vec<u8>> {
        let Some(conv) = self.me.as_ref().and_then(|p| p.dialogue.as_ref()) else {
            return Vec::new();
        };
        let mut out: Vec<Vec<u8>> = conv
            .page
            .lines
            .iter()
            .map(|line| pk::dialog(pk::DIALOG_TEXT, 0, &format!("{line}\n")))
            .collect();
        for (i, option) in conv
            .page
            .options
            .iter()
            .enumerate()
            .take(usize::from(pk::DIALOG_CLOSE))
        {
            out.push(match option.input {
                Some(length) => {
                    pk::dialog_with(pk::DIALOG_EDIT, i as u8, length as u16, &option.label)
                }
                None => pk::dialog(pk::DIALOG_LINK, i as u8, &option.label),
            });
        }
        out.push(pk::dialog(pk::DIALOG_CREATE, pk::DIALOG_CLOSE, ""));
        out
    }

    /// Answers a request: packets back to the client, and what it asks of the game.
    pub fn request(&mut self, request: Request, ids: &Ids) -> (Vec<Vec<u8>>, Vec<Input>) {
        let Some(me) = self.me.clone() else {
            return (Vec::new(), Vec::new());
        };
        let id = self.hero_id();
        let mut inputs = Vec::new();
        let mut out = Vec::new();
        match request {
            Request::Action {
                action: a, data, ..
            } => match a {
                action::ENTER_MAP => out = self.enter(),
                action::GET_ITEMS
                | action::GET_FRIENDS
                | action::GET_PROFICIENCIES
                | action::GET_SPELLS
                | action::GET_GUILD => out.push(pk::action(id, a, 0, (0, 0), 0)),
                action::EMOTION => inputs.push(Input::Emote(data)),
                action::OBSERVE_EQUIPMENT | action::OBSERVE_PLAYER => {
                    if let Some(target) = ids.entity(data) {
                        self.observe_asked = Some(target);
                        inputs.push(Input::Observe(target));
                    }
                }
                action::QUERY_PLAYER => {
                    if let Some((_, packet)) = ids.entity(data).and_then(|e| self.seen.get(&e)) {
                        out.push(packet.clone());
                    }
                }
                action::CHANGE_MAP => {
                    let portal = ((data & 0xFFFF) as i32, (data >> 16) as i32);
                    self.steps_to = None;
                    inputs.push(Input::EnterPortal(portal));
                }
                action::JUMP => {
                    let to = ((data & 0xFFFF) as i32, (data >> 16) as i32);
                    self.steps_to = Some(to);
                    inputs.push(Input::Jump(to));
                }
                action::REVIVE => {
                    self.steps_to = None;
                    inputs.push(Input::Revive);
                }
                action::GHOST => inputs.push(Input::Ghost),
                action::SET_PK_MODE => inputs.push(Input::SetPkMode(pk_mode(data))),
                action::CHANGE_FACE => inputs.push(Input::ChangeFace(data)),
                // A booth opened at the flag beside the hero, closed; a player's cry asked.
                action::CREATE_BOOTH => {
                    let flags = (self.npcs.iter()).filter(|n| n.kind == BOOTH_FLAG);
                    let near = flags.min_by_key(|n| c3_game::distance(n.cell, me.cell));
                    if let Some(n) = near.filter(|n| c3_game::distance(n.cell, me.cell) <= 3) {
                        inputs.push(Input::Booth(c3_game::BoothAction::Open(n.id)));
                    }
                }
                action::DESTROY_BOOTH => inputs.push(Input::Booth(c3_game::BoothAction::Close)),
                action::QUERY_CRY => {
                    let owner = ids.entity(data).and_then(|e| self.seen.get(&e));
                    if let Some((EntityState::Player(p), _)) = owner
                        && let Some(b) = p.booth.as_ref().filter(|b| !b.cry.is_empty())
                    {
                        out.push(pk::talk(channel::CRY, &p.name, "ALL", &b.cry, WHITE));
                    }
                }
                _ => {}
            },
            Request::Walk { dir, run } => {
                let from = self.steps_to.unwrap_or(me.cell);
                let to = (from.0 + DX[dir as usize], from.1 + DY[dir as usize]);
                self.steps_to = Some(to);
                if run != me.running {
                    inputs.push(Input::SetRunning(run));
                }
                inputs.push(Input::Walk(to));
            }
            Request::Interact {
                kind: interact::MAGIC,
                target,
                ts,
                cell,
                data,
            } => {
                self.steps_to = None;
                inputs.extend(self.cast_input(ts, target, cell, data, ids));
            }
            Request::Interact {
                kind: interact::WITHDRAW,
                ..
            } => inputs.push(Input::Withdraw),
            Request::Interact { kind, target, .. }
                if kind == interact::COURT || kind == interact::MARRY =>
            {
                if let Some(e) = ids.entity(target) {
                    inputs.push(match kind {
                        interact::COURT => Input::Propose(e),
                        _ => Input::AcceptProposal(e),
                    });
                }
            }
            Request::Interact { kind, target, .. }
                if kind == interact::ATTACK || kind == interact::SHOOT =>
            {
                if let Some(e) = ids.entity(target) {
                    self.steps_to = None;
                    inputs.push(Input::Attack(e));
                }
            }
            Request::Item {
                action: a,
                id: item_id,
                data,
                extra,
            } => match a {
                // Equipment worn in the slot the client names (data): a one-handed
                // weapon in the left hand.
                item::USE if data != 0 => inputs.push(Input::Wear {
                    item: item_id,
                    slot: u8::try_from(data).unwrap_or(0),
                }),
                item::USE => inputs.push(Input::Use(item_id)),
                item::UNEQUIP => inputs.push(Input::Unequip(item_id)),
                item::DROP => inputs.push(Input::Drop(item_id)),
                item::DROP_MONEY if item_id >= 100 => inputs.push(Input::DropMoney(item_id.into())),
                // A CP-shop buy names how many (the 290 asking); each is bought in turn.
                item::BUY => {
                    let buy = Input::Buy {
                        npc: item_id,
                        kind: data,
                    };
                    inputs.extend(std::iter::repeat_n(buy, extra.clamp(1, BUY_MOST) as usize));
                }
                item::SELL => inputs.push(Input::Sell {
                    npc: item_id,
                    item: data,
                }),
                // A DragonBall or Meteor (data) used on an item (id): items spec §2.3, §3.3.
                item::IMPROVE | item::UPLEV => inputs.push(Input::UseOn {
                    item: data,
                    target: item_id,
                }),
                item::SAVED_MONEY => inputs.push(Input::OpenStorage(item_id)),
                item::SAVE_MONEY => inputs.push(Input::SaveMoney {
                    npc: item_id,
                    amount: data.into(),
                }),
                item::DRAW_MONEY => inputs.push(Input::DrawMoney {
                    npc: item_id,
                    amount: data.into(),
                }),
                item::ENCHANT => inputs.push(Input::Combine {
                    kind: pk::combine::ENCHANT,
                    items: vec![item_id, data],
                    value: 0,
                }),
                item::REPAIR => inputs.push(Input::Combine {
                    kind: pk::combine::REPAIR,
                    items: vec![item_id],
                    value: 0,
                }),
                // Booths: goods asked, a ware set out (silver or CPs), taken back, bought.
                item::BOOTH_QUERY => self.booth_asked = self.booth_owner(data).map(|p| p.id),
                item::BOOTH_ADD | item::BOOTH_ADD_CPS => {
                    let currency = match a {
                        item::BOOTH_ADD => String::new(),
                        _ => crate::module::CONQUER_POINTS.to_owned(),
                    };
                    inputs.push(Input::Booth(c3_game::BoothAction::Add {
                        item: item_id,
                        price: data.into(),
                        currency,
                    }));
                }
                item::BOOTH_DEL => inputs.push(Input::Booth(c3_game::BoothAction::Remove(item_id))),
                item::BOOTH_BUY => {
                    if let Some(owner) = self.booth_owner(data).map(|p| p.id) {
                        let buy = c3_game::BoothAction::Buy {
                            owner,
                            item: item_id,
                        };
                        inputs.push(Input::Booth(buy));
                    }
                }
                _ => {}
            },
            Request::Guild {
                action: pk::guild::QUERY_NAME,
                id,
            } => {
                if let Some(name) = self.guild_names.get(&id) {
                    out.push(pk::name(pk::NAME_GUILD, id, name));
                }
            }
            Request::Guild { action: a, id } => {
                use c3_game::GuildAction as G;
                let player = ids.entity(id);
                let action = match (a, player) {
                    (pk::guild::INVITE_JOIN, Some(e)) => Some(G::Invite(e)),
                    (pk::guild::APPLY_JOIN, Some(e)) => Some(G::Apply(e)),
                    (pk::guild::LEAVE, _) => Some(G::Leave),
                    (pk::guild::KICK, Some(e)) => Some(G::Kick(e)),
                    (pk::guild::DONATE, _) => Some(G::Donate(id.into())),
                    (pk::guild::ALLY, Some(e)) => self.guild_of_seen(e).map(|g| G::Ally(g, true)),
                    (pk::guild::CLEAR_ALLY, _) => Some(G::Ally(id, false)),
                    (pk::guild::ENEMY, _) => Some(G::Enemy(id, true)),
                    (pk::guild::CLEAR_ENEMY, _) => Some(G::Enemy(id, false)),
                    _ => None,
                };
                inputs.extend(action.map(Input::Guild));
            }
            // The guild member list, a page of 10 (syndicate §2.3): after member `data` (-1: from
            // the start), online members while it is one, then offline ones from the start.
            Request::Allot(points) => {
                let names = ["strength", "agility", "vitality", "spirit"];
                let spent: Vec<(String, u32)> = (names.iter().zip(points))
                    .filter(|(_, n)| *n > 0)
                    .map(|(a, n)| ((*a).to_owned(), u32::from(n)))
                    .collect();
                if !spent.is_empty() {
                    inputs.push(Input::SpendPoints(spent));
                }
            }
            Request::Name {
                kind: pk::NAME_MEMBER_LIST,
                data,
            } => {
                out.push(self.member_page(data));
            }
            // Sitting at a dice table and leaving it (MsgName 17, 18 with the table's id).
            Request::Name {
                kind: pk::dice::NAME_JOIN,
                data,
            } => inputs.push(Input::Dice(c3_game::DiceAction::Join(data))),
            Request::Name {
                kind: pk::dice::NAME_LEAVE,
                data,
            } => inputs.push(Input::Dice(c3_game::DiceAction::Leave(data))),
            // A chip on a bet; every bet taken back (MsgDice 1113 action 2 with type 22).
            Request::Dice {
                action: pk::dice::CHIP_IN,
                npc,
                kind,
                amount,
            } if amount > 0 => inputs.push(Input::Dice(c3_game::DiceAction::Chip {
                npc,
                bet: kind,
                money: u64::from(amount),
            })),
            Request::Dice {
                action: pk::dice::CANCEL,
                npc,
                kind: pk::dice::ALL,
                ..
            } => inputs.push(Input::Dice(c3_game::DiceAction::CancelAll(npc))),
            // Asking another player to be friends (or taking up its ask), letting a friend
            // go, forgetting an enemy.
            Request::Friend {
                action: a,
                id: target,
            } => {
                use c3_game::FriendAction as F;
                use pk::friend as f;
                let input = match a {
                    f::APPLY | f::ACCEPT => ids.entity(target).map(|e| match a {
                        f::APPLY => F::Ask(e),
                        _ => F::Accept(e),
                    }),
                    f::BREAK => self.contact_name(target).map(F::Remove),
                    f::ENEMY_DEL => self.contact_name(target).map(F::Forget),
                    _ => None,
                };
                inputs.extend(input.map(Input::Friend));
            }
            // Mentors: offering to teach, asking to learn, answering either, leaving one's
            // mentor, sending a student away; the relations and what students gave asked for;
            // claiming it.
            Request::Mentor {
                action: a,
                first,
                second,
                data,
            } => {
                use c3_game::MentorAction as M;
                use pk::mentor as m;
                let name = |id: u32| {
                    (ids.entity(id).and_then(|e| self.seen.get(&e)))
                        .and_then(|(s, _)| match s {
                            EntityState::Player(p) => Some(p.name.clone()),
                            _ => None,
                        })
                        .or_else(|| self.contact_name(id))
                };
                let input = match a {
                    m::TEACH => ids.entity(second).map(M::Teach),
                    m::LEARN => ids.entity(second).map(M::Learn),
                    m::TEACHER_RESPONSE if data != 0 => ids.entity(first).map(M::Learn),
                    m::STUDENT_RESPONSE if data != 0 => ids.entity(first).map(M::Teach),
                    m::TEACHER_RESPONSE | m::STUDENT_RESPONSE => ids.entity(first).map(M::Refuse),
                    m::LEAVE => Some(M::Leave),
                    m::DISMISS => name(second).map(M::Expel),
                    _ => None,
                };
                inputs.extend(input.map(Input::Mentor));
            }
            Request::MentorMembers => self.mentors_asked = true,
            // Nobility: a donation (answered with its result), the board, a rank's price.
            Request::Peerage { action: a, data } => {
                if a == pk::peerage::DONATE {
                    inputs.push(Input::Donate(data));
                }
                self.peerage_asked = Some((a, data));
            }
            // A broadcast paid for or moved forward, the waiting lines asked for.
            Request::Pigeon {
                action: a,
                data,
                text,
            } => {
                use c3_game::BroadcastAction as B;
                use pk::pigeon as p;
                let input = match a {
                    p::PUBLISH => Some(B::Send(text)),
                    p::MOVE_FIRST => Some(B::First(data)),
                    p::MOVE_FIVE => Some(B::Bump(data)),
                    _ => None,
                };
                inputs.extend(input.map(Input::Broadcast));
                self.pigeon_asked = Some((a, data));
            }
            Request::MentorPrize { action: a } => {
                use c3_game::{MentorAction as M, MentorGift as G};
                use pk::mentor as m;
                let gift = match a {
                    m::CLAIM_EXP => Some(G::Exp),
                    m::CLAIM_BLESS => Some(G::Bless),
                    m::CLAIM_ITEM => Some(G::Item),
                    _ => None,
                };
                inputs.extend(gift.map(|g| Input::Mentor(M::Claim(g))));
                self.prize_asked = true;
            }
            // Asking a player to be a trade partner (or asking back), turning one down,
            // ending a partnership.
            // Offline training (MsgTraining 2044): asked, started, what it trained, claimed.
            Request::Training(a) => {
                use c3_game::TrainingAction as T;
                use pk::training as t;
                let action = match a {
                    t::ASK => Some(T::Ask),
                    t::START => Some(T::Start),
                    t::QUERY => Some(T::Query),
                    t::CLAIM => Some(T::Claim),
                    _ => None,
                };
                inputs.extend(action.map(Input::Training));
            }
            // Locking, unlocking (MsgEquipLock 2048), and the unlock day asked.
            Request::ItemLock { item, action: a } => match a {
                pk::item_lock::APPLY => inputs.push(Input::Lock(item)),
                pk::item_lock::CANCEL => inputs.push(Input::Unlock(item)),
                _ => self.lock_asked = Some(item),
            },
            Request::Business { action: a, id } => {
                use c3_game::PartnerAction as P;
                use pk::business as b;
                let input = match a {
                    b::APPLY => ids.entity(id).map(P::Ask),
                    b::REJECT => ids.entity(id).map(P::Refuse),
                    b::BREAK => self.contact_name(id).map(P::Break),
                    _ => None,
                };
                inputs.extend(input.map(Input::Partner));
            }
            Request::Team { action: a, id } => {
                let other = ids.entity(id);
                let action = match (a, other) {
                    (pk::team::CREATE, _) => Some(TeamAction::Create),
                    (pk::team::CLOSE, _) => Some(TeamAction::Close),
                    (pk::team::OPEN, _) => Some(TeamAction::Open),
                    (pk::team::KEEP_MONEY, _) => Some(TeamAction::KeepMoney),
                    (pk::team::SHARE_MONEY, _) => Some(TeamAction::ShareMoney),
                    (pk::team::KEEP_ITEMS, _) => Some(TeamAction::KeepItems),
                    (pk::team::SHARE_ITEMS, _) => Some(TeamAction::ShareItems),
                    (pk::team::APPLY, Some(e)) => Some(TeamAction::Apply(e)),
                    (pk::team::AGREE_JOIN, Some(e)) => Some(TeamAction::Accept(e)),
                    (pk::team::LEAVE | pk::team::DISMISS, _) => Some(TeamAction::Leave),
                    (pk::team::INVITE, Some(e)) => Some(TeamAction::Invite(e)),
                    (pk::team::ACCEPT_INVITE, Some(e)) => Some(TeamAction::Join(e)),
                    (pk::team::KICK, Some(e)) => Some(TeamAction::Kick(e)),
                    _ => None,
                };
                inputs.extend(action.map(Input::Team));
            }
            Request::Trade { action: a, data } => match a {
                pk::trade::APPLY => {
                    if let Some(e) = ids.entity(data) {
                        inputs.push(Input::TradeAsk(e));
                    }
                }
                pk::trade::QUIT => inputs.push(Input::TradeClose),
                pk::trade::ADD_ITEM => inputs.push(Input::TradeItem(data)),
                pk::trade::ADD_MONEY => inputs.push(Input::TradeMoney(data.into())),
                pk::trade::OK => inputs.push(Input::TradeAccept),
                _ => {}
            },
            // A sash: listed (the client opens its window itself), an item put in or taken
            // out (packets spec 1102 type 30).
            Request::Package {
                npc: chest,
                action: a,
                kind: pk::PACKAGE_CHEST,
                item,
            } => match a {
                pk::PACKAGE_CHECK_IN => inputs.push(Input::ChestPut { chest, item }),
                pk::PACKAGE_CHECK_OUT => inputs.push(Input::ChestTake { chest, item }),
                _ => self.chest_asked = Some(chest),
            },
            Request::Package {
                npc,
                action: a,
                kind: pk::PACKAGE_STORAGE,
                item,
            } => inputs.push(match a {
                pk::PACKAGE_CHECK_IN => Input::Store { npc, item },
                pk::PACKAGE_CHECK_OUT => Input::Retrieve { npc, item },
                _ => Input::OpenStorage(npc),
            }),
            Request::Fuse { kind, items } if !items.is_empty() => inputs.push(Input::Combine {
                kind: pk::combine::FUSE,
                items,
                value: i64::from(kind),
            }),
            Request::GemEmbed {
                item: target,
                gem,
                pos,
                remove,
            } => inputs.push(Input::Combine {
                kind: if remove {
                    pk::combine::REMOVE_GEM
                } else {
                    pk::combine::EMBED
                },
                items: if remove {
                    vec![target]
                } else {
                    vec![target, gem]
                },
                value: i64::from(pos.clamp(1, 2)),
            }),
            Request::Compose { items } if items.len() >= 3 => inputs.push(Input::Combine {
                kind: pk::combine::COMPOSE,
                items,
                value: 0,
            }),
            Request::MapItem {
                action: 3,
                id: item_id,
            } => inputs.push(Input::PickUp(EntityId(item_id))),
            Request::Npc { action: 0, id: npc } => inputs.push(Input::Talk(npc)),
            // The facing is the look's last digit; a statue's is the look, posed in its
            // action (the sort) and frame (the type).
            Request::Placed {
                cell,
                look,
                kind,
                sort,
            } => inputs.push(Input::Place {
                cell: (i32::from(cell.0), i32::from(cell.1)),
                direction: match sort {
                    0 => (look % 10) as u8,
                    _ => (look % 8) as u8,
                },
                pose: (sort != 0).then(|| (u32::from(sort), u32::from(kind))),
            }),
            Request::Dialog {
                action: pk::DIALOG_ANSWER,
                index,
                text,
            } => inputs.push(if index == pk::DIALOG_CLOSE {
                Input::CloseDialogue
            } else if text.is_empty() {
                Input::Answer(index.into())
            } else {
                Input::AnswerText {
                    index: index.into(),
                    text,
                }
            }),
            Request::Ping(packet) => out.push(packet),
            // A message board: a post taken down, a page listed, a post read.
            Request::Board {
                index,
                channel: code,
                action: a,
                name,
            } => match a {
                pk::board::DEL => {
                    if let Some(board) = code.checked_sub(pk::board::FIRST_CHANNEL) {
                        inputs.push(Input::TakeDown {
                            board: board as u8,
                            author: name,
                        });
                    }
                }
                pk::board::GET_LIST => self.board_asked = Some((code, index, String::new())),
                pk::board::GET_WORDS if !name.is_empty() => {
                    self.board_asked = Some((code, index, name));
                }
                _ => {}
            },
            Request::Talk {
                channel: code,
                hearer,
                words,
            } if !words.trim().is_empty() => {
                use c3_game::ChatChannel as C;
                let on = match code {
                    channel::WHISPER => Some(C::Whisper),
                    channel::TEAM => Some(C::Team),
                    channel::GUILD => Some(C::Guild),
                    channel::FRIEND => Some(C::Friend),
                    channel::LEAVE_WORD => Some(C::LeaveWord),
                    channel::BROADCAST => Some(C::Broadcast),
                    // A post on a message board (2201 trade .. 2205 other).
                    2201..=2205 => Some(C::Board((code - pk::board::FIRST_CHANNEL) as u8)),
                    _ => None,
                };
                inputs.push(match on {
                    // A leader's guild announcement; a booth's cry.
                    _ if code == channel::GUILD_NOTICE => Input::GuildNotice(words),
                    _ if code == channel::CRY => Input::Booth(c3_game::BoothAction::Cry(words)),
                    // Commands and talk go as they are.
                    Some(on) if !words.starts_with('/') => Input::Chat {
                        channel: on,
                        to: hearer,
                        text: words,
                    },
                    _ => Input::Say(words),
                });
            }
            _ => {}
        }
        (out, inputs)
    }
}
