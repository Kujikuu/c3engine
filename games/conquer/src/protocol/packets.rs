//! Conquer 5065's packets: building what the server sends, reading what the client sends.
//! Layouts and meanings: docs/conquer5065_protocol.md, part C.

use c3_net::packet::{PacketError, Reader, Writer, packet_type};

pub const TALK: u16 = 1004;
pub const WALK: u16 = 1005;
pub const USER_INFO: u16 = 1006;
pub const ITEM_INFO: u16 = 1008;
pub const ITEM: u16 = 1009;
pub const ACTION: u16 = 1010;
pub const PLAYER: u16 = 1014;
pub const ATTRIB: u16 = 1017;
pub const INTERACT: u16 = 1022;
pub const ACCOUNT: u16 = 1051;
pub const CONNECT: u16 = 1052;
pub const LOGIN_REPLY: u16 = 1055;
pub const MAP_ITEM: u16 = 1101;
/// MsgMapItem actions of a trap: placed, gone (TQ's TRAP_CREATE / TRAP_ERASE).
pub const MAP_ITEM_TRAP: u8 = 10;
pub const MAP_ITEM_TRAP_GONE: u8 = 12;
/// Names shown in the world: effects on a cell or a role, sounds (MsgName).
pub const NAME: u16 = 1015;
/// MsgName types: an effect on a map cell, on a role.
pub const NAME_MAP_EFFECT: u8 = 9;
pub const NAME_ROLE_EFFECT: u8 = 10;
/// The hero's guild: id, fund, members, its rank and leader (MsgSyndicateAttributeInfo).
pub const GUILD_INFO: u16 = 1106;

/// Friends and enemies (MsgFriend 1019, 0x24 bytes): a friend or enemy's id, the action,
/// whether it is online, its look, level and name.
pub const FRIEND: u16 = 1019;
pub mod friend {
    pub const APPLY: u8 = 10;
    pub const ACCEPT: u8 = 11;
    pub const ONLINE: u8 = 12;
    pub const OFFLINE: u8 = 13;
    pub const BREAK: u8 = 14;
    pub const GET_INFO: u8 = 15;
    pub const ENEMY_ONLINE: u8 = 16;
    pub const ENEMY_OFFLINE: u8 = 17;
    pub const ENEMY_DEL: u8 = 18;
    pub const ENEMY_ADD: u8 = 19;
}

pub fn friend(action: u8, id: u32, online: bool, level: u32, name: &str) -> Vec<u8> {
    packet(FRIEND, |w| {
        w.u32(id).u8(action).u8(u8::from(online)).u8(0).u8(0);
        w.u32(0).u32(level).fixed_str(name, 16);
    })
}
/// MsgTradeBuddy / MsgBusiness (2046): a trade partner's id, the action, whether it is online,
/// the minutes of trial left, its look and name (MsgBusiness.h, 0x20 bytes).
pub fn business(action: u8, id: u32, online: bool, minutes: u16, look: u32, name: &str) -> Vec<u8> {
    packet(BUSINESS, |w| {
        w.u32(id).u8(action).u8(u8::from(online)).u16(minutes);
        w.u32(look).fixed_str(name, 16);
    })
}

pub const BUSINESS: u16 = 2046;
/// Offline training (Training.dll): MsgTraining's actions both ways, and what it trained.
/// MsgDice (MsgBetOutside) 1113, both ways (tq_re spec systems/dice.md §2.1).
pub const DICE: u16 = 1113;
pub mod dice {
    /// Client: a chip put on a bet; server: another player's chips.
    pub const CHIP_IN: u8 = 0;
    /// The player's own chip taken.
    pub const CHIP_IN_CONFIRM: u8 = 1;
    /// Client: the bets taken back (with ALL); server: another player's bets taken back.
    pub const CANCEL: u8 = 2;
    pub const CANCEL_CONFIRM: u8 = 3;
    /// Betting open (with the seconds left), betting closed.
    pub const BEGIN: u8 = 4;
    pub const END: u8 = 5;
    /// The roll.
    pub const ROLL: u8 = 6;
    /// The bet type meaning every bet (a cancel's).
    pub const ALL: u8 = 22;
    /// MsgName's table actions: a player at the table (its own name: seated), a player
    /// gone, a win (data: the payout).
    pub const NAME_JOIN: u8 = 17;
    pub const NAME_LEAVE: u8 = 18;
    pub const NAME_BONUS: u8 = 19;
}

/// MsgDice 1113 from the server: `entries` (bet type, silver) after the table's id, then
/// the zero slot TQ's builder leaves; `count` is the entry count or, opening, the seconds left.
pub fn dice(action: u8, count: u8, npc: u32, entries: &[(u8, u32)]) -> Vec<u8> {
    packet(DICE, |w| {
        w.u8(action).u8(count).u16(0).u32(npc);
        for &(kind, silver) in entries {
            w.u8(kind).u8(0).u16(0).u32(silver);
        }
        w.u32(0).u32(0);
    })
}

/// MsgDice's roll: one entry holding the four dice (only the first three are drawn).
pub fn dice_roll(npc: u32, dice: [u8; 4]) -> Vec<u8> {
    packet(DICE, |w| {
        w.u8(dice::ROLL).u8(1).u16(0).u32(npc).u32(0);
        for d in dice {
            w.u8(d);
        }
    })
}

pub const TRAINING: u16 = 2044;
pub const TRAINING_INFO: u16 = 2043;
pub mod training {
    /// Client: asked; server: the minutes offered.
    pub const ASK: u16 = 0;
    pub const START: u16 = 1;
    /// Server: back from training (the client asks what it trained).
    pub const READY: u16 = 2;
    pub const QUERY: u16 = 3;
    pub const CLAIM: u16 = 4;
    /// Server: no training on this map (StrRes 11056).
    pub const DENIED: u16 = 5;
}

/// MsgTraining 2044 (12 bytes): its action and data.
pub fn training(action: u16, data: u32) -> Vec<u8> {
    packet(TRAINING, |w| {
        w.u16(action).u16(0).u32(data);
    })
}

/// MsgTrainingInfo 2043 (16 bytes): minutes trained and left, the level reached and how far
/// into it (ten-millionths).
pub fn training_info(trained: u16, remaining: u16, level: u16, fraction: u32) -> Vec<u8> {
    packet(TRAINING_INFO, |w| {
        w.u16(trained)
            .u16(remaining)
            .u16(level)
            .u16(0)
            .u32(fraction);
    })
}

/// MsgEquipLock (the 5065 client's MsgItemLock): lock, unlock, query, unlocked.
pub const ITEM_LOCK: u16 = 2048;
pub mod item_lock {
    pub const APPLY: u8 = 0;
    pub const CANCEL: u8 = 1;
    pub const QUERY: u8 = 2;
    pub const UNLOCKED: u8 = 3;
}
pub mod business {
    pub const APPLY: u8 = 0;
    pub const REJECT: u8 = 1;
    pub const ONLINE: u8 = 2;
    pub const OFFLINE: u8 = 3;
    pub const BREAK: u8 = 4;
    pub const ADD: u8 = 5;
}

/// MsgMentor5065 (2065): a mentor action between two players, its data and names
/// (MsgMentor5065.h, 0x14 bytes before the strings).
pub fn mentor(action: u8, first: u32, second: u32, data: u32, names: &[&str]) -> Vec<u8> {
    packet(MENTOR, |w| {
        w.u8(action)
            .u8(0)
            .u8(0)
            .u8(0)
            .u32(first)
            .u32(second)
            .u32(data);
        strings(w, names);
    })
}

/// One of the hero's mentor relations (MsgMentorMember5065 2066, 0x44 bytes before the
/// strings [mentor, student, spouse]): the hero's id, the member's, its level, profession,
/// PK points, whether it is online and what it gave (time, blessed hours, item points).
pub struct MentorMember<'a> {
    pub action: u8,
    pub identity: u32,
    pub member: u32,
    pub level: u8,
    pub profession: u8,
    pub pk: u16,
    pub online: bool,
    pub gifts: (u32, u16, u16),
    pub names: [&'a str; 3],
}

pub fn mentor_member(m: &MentorMember) -> Vec<u8> {
    packet(MENTOR_MEMBER, |w| {
        w.u8(m.action)
            .u8(0)
            .u8(0)
            .u8(0)
            .u32(m.identity)
            .u32(m.member);
        w.u32(0).u32(0).u32(0).u32(0);
        w.u8(m.level).u8(m.profession).u16(m.pk).u32(0);
        w.u32(0).u32(0);
        w.u8(u8::from(m.online)).u8(0).u8(0).u8(0).u32(0);
        w.u32(m.gifts.0).u32(0).u16(m.gifts.1).u16(m.gifts.2);
        strings(w, &m.names);
    })
}

/// What a mentor's students gave, to claim (MsgMentorContribution5065 2067 action 4).
pub fn mentor_prize(action: u8, identity: u32, gifts: (u32, u16, u16)) -> Vec<u8> {
    packet(MENTOR_PRIZE, |w| {
        w.u8(action)
            .u8(0)
            .u8(0)
            .u8(0)
            .u32(identity)
            .u32(0)
            .u32(0)
            .u32(0);
        w.u32(gifts.0).u32(0).u16(gifts.1).u16(gifts.2);
        strings(w, &[]);
    })
}

pub const MENTOR: u16 = 2065;
pub const MENTOR_MEMBER: u16 = 2066;
pub const MENTOR_PRIZE: u16 = 2067;
/// MsgMentor5065 / member / contribution actions (MsgMentor5065.h).
pub mod mentor {
    pub const TEACH: u8 = 1;
    pub const LEARN: u8 = 2;
    pub const LEAVE: u8 = 3;
    pub const DISMISS: u8 = 4;
    pub const TEACHER_RESPONSE: u8 = 8;
    pub const STUDENT_RESPONSE: u8 = 9;
    pub const MEMBER_TEACHER: u8 = 1;
    pub const MEMBER_STUDENT: u8 = 2;
    pub const MEMBER_STUDENT_END: u8 = 3;
    pub const CLAIM_EXP: u8 = 1;
    pub const CLAIM_BLESS: u8 = 2;
    pub const CLAIM_ITEM: u8 = 3;
    pub const AVAILABLE: u8 = 4;
}

/// MsgPigeon (2050) to the client: a broadcast action's result (`data`: the line's place).
pub fn pigeon(action: u8, data: u32) -> Vec<u8> {
    packet(PIGEON, |w| {
        w.u8(action).u8(0).u8(0).u8(0).u32(data);
        strings(w, &[]);
    })
}

/// One waiting broadcast line as MsgPigeonInfo lists it (0x70 bytes).
pub struct PigeonLine<'a> {
    pub id: u32,
    pub owner: u32,
    pub place: u32,
    pub name: &'a str,
    pub seconds: u32,
    pub text: &'a str,
}

/// MsgPigeonInfo (2051): page `page` of the waiting broadcast lines.
pub fn pigeon_info(page: u32, lines: &[PigeonLine]) -> Vec<u8> {
    packet(PIGEON_INFO, |w| {
        w.u32(page).u16(0).u16(lines.len() as u16);
        for l in lines {
            w.u32(l.id).u32(l.owner).u32(l.place).fixed_str(l.name, 16);
            w.u32(l.seconds).fixed_str(l.text, 80);
        }
    })
}

/// MsgPeerage (2064): a nobility action, its data (an amount, or the page and pages) and its
/// lines (MsgPeerage.h: 0x10 bytes before the strings).
pub fn peerage(action: u16, data: u64, lines: &[&str]) -> Vec<u8> {
    packet(PEERAGE, |w| {
        w.u16(action)
            .u16(0)
            .u32(data as u32)
            .u32((data >> 32) as u32);
        strings(w, lines);
    })
}

pub const PEERAGE: u16 = 2064;
/// MsgPeerage actions: a donation's result, a page of the board, the hero's standing, what a
/// rank needs.
pub mod peerage {
    pub const DONATE: u16 = 1;
    pub const PAGE: u16 = 2;
    pub const UPDATE: u16 = 3;
    pub const QUERY_RANK: u16 = 4;
    pub const PAGE_SIZE: usize = 10;
}

pub const PIGEON: u16 = 2050;
pub const PIGEON_INFO: u16 = 2051;
/// MsgPigeon actions (MsgPigeon.h).
pub mod pigeon {
    pub const QUERY_QUEUE: u8 = 1;
    pub const QUERY_MINE: u8 = 2;
    pub const PUBLISH: u8 = 3;
    pub const MOVE_FIRST: u8 = 4;
    pub const MOVE_FIVE: u8 = 5;
    pub const PAGE: usize = 10;
}

/// A guild action about a guild or player (MsgSyndicate).
pub const GUILD: u16 = 1107;
pub mod guild {
    /// Accepting an invitation (`id` the inviter); a leader inviting (`id` the player).
    pub const APPLY_JOIN: u16 = 1;
    pub const INVITE_JOIN: u16 = 2;
    pub const LEAVE: u16 = 3;
    pub const KICK: u16 = 4;
    /// The client asks a guild's name.
    pub const QUERY_NAME: u16 = 6;
    /// Allies (`id` a player of the other guild, or the guild cleared) and enemies (guilds).
    pub const ALLY: u16 = 7;
    pub const CLEAR_ALLY: u16 = 8;
    pub const ENEMY: u16 = 9;
    pub const CLEAR_ENEMY: u16 = 10;
    /// Money (`id` the amount) into the guild's fund.
    pub const DONATE: u16 = 11;
}

/// MsgSyndicate `action` about `id`.
pub fn guild(action: u16, id: u32) -> Vec<u8> {
    packet(GUILD, |w| {
        w.u16(action).u16(0).u32(id);
    })
}
/// MsgName kind naming a guild (`data` its id).
pub const NAME_GUILD: u8 = 3;
/// MsgName of the player's spouse (NAMEACT_MATE).
pub const NAME_MATE: u8 = 6;

/// The hero's guild (id 0: none).
pub struct GuildInfo<'a> {
    pub id: u32,
    /// What the hero gave it (nProffer).
    pub contribution: i32,
    pub fund: u32,
    pub members: u32,
    pub rank: u8,
    pub leader: &'a str,
}

pub fn guild_info(g: &GuildInfo) -> Vec<u8> {
    packet(GUILD_INFO, |w| {
        w.u32(g.id)
            .i32(g.contribution)
            .u32(g.fund)
            .u32(g.members)
            .u8(g.rank);
        w.fixed_str(g.leader, 16).u8(0).u8(0).u8(0);
    })
}

/// A team action about a player (packets spec 1023).
pub const TEAM: u16 = 1023;
pub mod team {
    pub const CREATE: u16 = 0;
    pub const APPLY: u16 = 1;
    pub const LEAVE: u16 = 2;
    pub const ACCEPT_INVITE: u16 = 3;
    pub const INVITE: u16 = 4;
    pub const AGREE_JOIN: u16 = 5;
    pub const DISMISS: u16 = 6;
    pub const KICK: u16 = 7;
    pub const CLOSE: u16 = 8;
    pub const OPEN: u16 = 9;
    /// The team's money kept from each other or shared; its items kept or shared.
    pub const KEEP_MONEY: u16 = 10;
    pub const SHARE_MONEY: u16 = 11;
    pub const KEEP_ITEMS: u16 = 12;
    pub const SHARE_ITEMS: u16 = 13;
}
/// A team's members, the list replacing the client's (MsgTeamMember 1026).
pub const TEAM_MEMBER: u16 = 1026;
const TEAM_MEMBERS_REPLACE: u8 = 1;

/// One member of a team list.
pub struct TeamMember<'a> {
    pub name: &'a str,
    pub id: u32,
    pub look: u32,
    pub life: u16,
    pub max_life: u16,
}

/// MsgTeam `action` about player `id`.
pub fn team(action: u16, id: u32) -> Vec<u8> {
    packet(TEAM, |w| {
        w.u16(action).u16(0).u32(id);
    })
}

/// The hero's whole team.
pub fn team_members(members: &[TeamMember]) -> Vec<u8> {
    packet(TEAM_MEMBER, |w| {
        w.u8(TEAM_MEMBERS_REPLACE).u8(members.len() as u8).u16(0);
        for m in members {
            w.fixed_str(m.name, 16);
            w.u32(m.id).u32(m.look).u16(m.max_life).u16(m.life);
        }
    })
}

/// A trade between two players (packets spec 1056): `data` and an action.
pub const TRADE: u16 = 1056;
pub mod trade {
    pub const APPLY: u16 = 1;
    pub const QUIT: u16 = 2;
    pub const OPEN: u16 = 3;
    pub const FAIL: u16 = 4;
    pub const SUCCESS: u16 = 5;
    pub const ADD_ITEM: u16 = 6;
    pub const ADD_MONEY: u16 = 7;
    pub const PARTNER_MONEY: u16 = 8;
    pub const OWN_MONEY: u16 = 9;
    pub const OK: u16 = 10;
    pub const ITEM_REFUSED: u16 = 11;
}
/// MsgItemInfo's action of an item the trade partner offers.
pub const ITEM_TRADE: u8 = 2;
/// A storage NPC's warehouse: listed, an item stored, taken out (packets spec 1102).
pub const PACKAGE: u16 = 1102;
/// MsgPackage actions and the warehouse kind.
pub const PACKAGE_QUERY: u8 = 0;
pub const PACKAGE_CHECK_IN: u8 = 1;
pub const PACKAGE_CHECK_OUT: u8 = 2;
pub const PACKAGE_STORAGE: u8 = 10;
/// MsgPackage's kind for a sash (its id the sash's item id).
pub const PACKAGE_CHEST: u8 = 30;
/// Setting a gem in a socket or taking it out (items spec §6.1).
pub const GEM_EMBED: u16 = 1027;
pub const FUSE: u16 = 1028;
pub const ALLOT: u16 = 1024;
/// Composing +N: the main item, two minors and the gems (items spec §7.1).
pub const COMPOSE: u16 = 2036;
pub const NPC_INFO: u16 = 2030;
pub const NPC_INFO_EX: u16 = 1109;
pub const NPC: u16 = 2031;
pub const DIALOG: u16 = 2032;

/// Chat channels.
pub mod channel {
    /// TQ's _TXTATR_*: whisper, team, guild (NetworkDef.h).
    pub const WHISPER: u16 = 2001;
    pub const TEAM: u16 = 2003;
    pub const GUILD: u16 = 2004;
    pub const FRIEND: u16 = 2009;
    /// A guild's announcement, set by its leader and told to its members (_TXTATR_SYNANNOUNCEG).
    pub const GUILD_NOTICE: u16 = 2111;
    /// Words left for an offline friend (_TXTATR_LEAVEWORD; social spec §5).
    pub const LEAVE_WORD: u16 = 2110;
    /// A paid broadcast (Pigeon.dll's channel).
    pub const BROADCAST: u16 = 2500;
    /// A booth's cry (_TXTATR_CRYOUT).
    pub const CRY: u16 = 2104;
    pub const TALK: u16 = 2007;
    pub const SYSTEM: u16 = 2005;
    pub const ENTRANCE: u16 = 2101;
}

/// MsgAction actions.
pub mod action {
    pub const ENTER_MAP: u16 = 74;
    /// A booth opened in front of the hero (echoed with the booth's id), closed, and a
    /// player's booth cry asked (booth_trade spec §1.1).
    pub const CREATE_BOOTH: u16 = 111;
    /// A face picked at the face NPC (data the face).
    pub const CHANGE_FACE: u16 = 142;
    pub const DESTROY_BOOTH: u16 = 114;
    pub const QUERY_CRY: u16 = 115;
    pub const GET_ITEMS: u16 = 75;
    pub const GET_FRIENDS: u16 = 76;
    pub const GET_PROFICIENCIES: u16 = 77;
    pub const GET_SPELLS: u16 = 78;
    pub const EMOTION: u16 = 81;
    /// The hero's PK mode, asked and told (actionSetPkMode).
    pub const SET_PK_MODE: u16 = 96;
    /// The client stepped on a portal (data: its cell, x | y << 16).
    pub const CHANGE_MAP: u16 = 85;
    pub const FLY_MAP: u16 = 86;
    pub const LEVEL_UP: u16 = 92;
    /// The dead hero asks to come back (battle spec S9).
    pub const REVIVE: u16 = 94;
    pub const GET_GUILD: u16 = 97;
    /// A client window opened by number (1 compose, 4 the warehouse).
    pub const OPEN_DIALOG: u16 = 126;
    pub const QUERY_PLAYER: u16 = 102;
    /// Another player's equipment asked for (117; the 5065 client's view button sends 310,
    /// which the front answers alike, Redux's A15).
    pub const OBSERVE_EQUIPMENT: u16 = 117;
    pub const OBSERVE_PLAYER: u16 = 310;
    pub const LEAVE_MAP: u16 = 132;
    pub const JUMP: u16 = 133;
    /// The dead hero gets up as a ghost, after its fall.
    pub const GHOST: u16 = 137;
}

/// MsgUserAttrib attributes.
pub mod attr {
    pub const LIFE: u32 = 0;
    pub const MAX_LIFE: u32 = 1;
    pub const MANA: u32 = 2;
    pub const MAX_MANA: u32 = 3;
    pub const MONEY: u32 = 4;
    pub const EXP: u32 = 5;
    pub const LEVEL: u32 = 13;
    pub const SPIRIT: u32 = 14;
    pub const VITALITY: u32 = 15;
    pub const STRENGTH: u32 = 16;
    pub const AGILITY: u32 = 17;
    pub const PK: u32 = 6;
    pub const LOOK_FACE: u32 = 12;
    pub const STATUS: u32 = 26;
    pub const XP: u32 = 28;
    /// The money kept in storage.
    pub const SAVED_MONEY: u32 = 10;
}

/// MsgInteract kinds.
pub mod interact {
    pub const ATTACK: u16 = 2;
    /// A proposal of marriage, and its acceptance (the proposer as the target).
    pub const COURT: u16 = 8;
    pub const MARRY: u16 = 9;
    pub const KILL: u16 = 14;
    /// A spell cast: its type, level, target and cell come scrambled (packets spec 1022).
    pub const MAGIC: u16 = 21;
    pub const SHOOT: u16 = 25;
    /// CPs wait for the account (the withdraw button shows), and taking them (the reply
    /// carries how many).
    pub const WITHDRAW_READY: u16 = 31;
    pub const WITHDRAW: u16 = 32;
}

/// Magic whose level word is scrambled once more (packets spec 1022 MAGICATTACK).
const MAGIC_SCRAMBLED: [u16; 8] = [1000, 1001, 1002, 1030, 1125, 1150, 1160, 1165];

/// A spell the client casts, unscrambled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spell {
    pub kind: u16,
    pub level: u16,
    /// The target's id (0: the ground).
    pub target: u32,
    pub cell: (u16, u16),
}

/// Unscrambles MsgInteract MAGICATTACK fields, keyed by the caster's id `hero`
/// (packets spec 1022); `None` when its level check fails.
pub fn spell(hero: u32, ts: u32, target: u32, cell: (u16, u16), data: (u16, u16)) -> Option<Spell> {
    let key = hero as u16;
    let kind = (data.0 ^ key ^ 0x915D).rotate_left(3).wrapping_add(0x14BE);
    let mut level = data.1;
    if MAGIC_SCRAMBLED.contains(&kind) {
        level ^= 0x6279;
    }
    if (((level >> 8) ^ 0x37) & 0xFF) as u32 != ts & 0xFF {
        return None;
    }
    Some(Spell {
        kind,
        level: (level & 0xFF) ^ 0x21,
        target: (target.rotate_left(19) ^ hero ^ 0x5F2D_2463).wrapping_add(0x8B90_B51A),
        cell: (
            (cell.0 ^ key ^ 0x2ED6).rotate_left(1).wrapping_sub(0x22EE),
            (cell.1 ^ key ^ 0xB99B).rotate_left(5).wrapping_add(0x76DE),
        ),
    })
}

/// MsgItem actions.
pub mod item {
    pub const BUY: u16 = 1;
    pub const SELL: u16 = 2;
    pub const REMOVE: u16 = 3;
    /// From the client: drop bag item `id`; drop `id` money (at least 100).
    pub const DROP: u16 = 3;
    pub const DROP_MONEY: u16 = 12;
    pub const USE: u16 = 4;
    pub const EQUIP: u16 = 5;
    pub const UNEQUIP: u16 = 6;
    /// The money kept at a storage NPC (id): asked, put in, taken out (data the amount).
    pub const SAVED_MONEY: u16 = 9;
    pub const SAVE_MONEY: u16 = 10;
    pub const DRAW_MONEY: u16 = 11;
    /// A DragonBall (data) on an item (id): its quality.
    pub const IMPROVE: u16 = 19;
    /// A Meteor (data) on an item (id): its level.
    pub const UPLEV: u16 = 20;
    pub const PING: u16 = 27;
    /// A gem (data) enchanting an item (id): its extra life (items spec §6.4).
    pub const ENCHANT: u16 = 28;
    /// A bag item (id) repaired at a blacksmith.
    pub const REPAIR: u16 = 14;
    /// Fireworks over a player (data 0xFF), as a wedding has.
    pub const FIREWORKS: u16 = 26;
    /// Booths (MsgItem.h ITEMACT_BOOTH_*): a booth's goods asked (data the booth); a bag item
    /// set out for money or CPs (data the price), taken back or gone (data the booth),
    /// bought (data the booth).
    pub const BOOTH_QUERY: u16 = 21;
    pub const BOOTH_ADD: u16 = 22;
    pub const BOOTH_DEL: u16 = 23;
    pub const BOOTH_BUY: u16 = 24;
    pub const BOOTH_ADD_CPS: u16 = 29;
}

/// The client cues Conquer's front sends (`client_cue`): a MsgAction of an action type (with
/// its data), a MsgMapItem action at the hero's cell (with its id).
pub mod cue {
    pub const ACTION: &str = "msg_action";
    pub const MAP_ITEM: &str = "map_item";
    /// A MsgItem action about the player (with its data), for everyone around.
    pub const ITEM_ACTION: &str = "item_action";
    /// A MsgName of a type with a text (fireworks words), for everyone around.
    pub const NAME: &str = "msg_name";
}

/// The kinds of `Input::Combine` Conquer's rules blueprint answers.
pub mod combine {
    /// A gem set in an item's socket (`value`: 1 or 2).
    pub const EMBED: u32 = 1;
    /// The gem in an item's socket taken out (destroyed).
    pub const REMOVE_GEM: u32 = 2;
    /// +N composing: the main item, two minors, the gems.
    pub const COMPOSE: u32 = 3;
    /// A gem enchanting an item.
    pub const ENCHANT: u32 = 4;
    /// A worn-down item repaired (MsgItem 14).
    pub const REPAIR: u32 = 5;
    /// Gear fused into a +1 stone (MsgFuse 1028, `value` its fuse type).
    pub const FUSE: u32 = 6;
}

/// Status flag of a dead role.
pub const STATUS_DEAD: u32 = 0x20;
/// Status bits of a player's name (battle spec S3, S6): a criminal's blue, PK points' red and
/// black.
pub const STATUS_CRIME: u32 = 0x1;
pub const STATUS_RED: u32 = 0x4000;
pub const STATUS_BLACK: u32 = 0x8000;
/// A ghost.
pub const STATUS_GHOST: u32 = 0x400;
/// The XP bar is full: XP skills may be cast.
pub const STATUS_XP_FULL: u32 = 0x10;
/// MsgTaskDialog actions and the closing link.
pub const DIALOG_TEXT: u8 = 1;
pub const DIALOG_LINK: u8 = 2;
/// A text box answering with what is typed (`data` its length).
pub const DIALOG_EDIT: u8 = 3;
pub const DIALOG_CREATE: u8 = 100;
pub const DIALOG_ANSWER: u8 = 101;
pub const DIALOG_CLOSE: u8 = 255;

/// A packet of `kind` whose body `body` writes; the size goes in its header.
fn packet(kind: u16, body: impl FnOnce(&mut Writer)) -> Vec<u8> {
    let mut w = Writer::new();
    w.u16(0).u16(kind);
    body(&mut w);
    let mut bytes = w.into_bytes();
    let size = bytes.len() as u16;
    bytes[..2].copy_from_slice(&size.to_le_bytes());
    bytes
}

/// Strings as Conquer packs them: a count, then each after its length.
fn strings(w: &mut Writer, list: &[&str]) {
    w.u8(list.len() as u8);
    for s in list {
        w.short_str(s);
    }
}

pub fn login_reply(token: u32, data: u32, ip: &str, port: u32) -> Vec<u8> {
    packet(LOGIN_REPLY, |w| {
        w.u32(token).u32(data).fixed_str(ip, 16).u32(port);
    })
}

/// A chat line from `speaker` to `hearer`.
/// MsgName of `kind` with `data` (a cell as x | y << 16, or a role id) and one name.
/// MsgName of `kind` with several strings.
pub fn names(kind: u8, data: u32, list: &[&str]) -> Vec<u8> {
    packet(NAME, |w| {
        w.u32(data).u8(kind);
        strings(w, list);
    })
}

/// MsgName's guild member list (strings `name level online`).
pub const NAME_MEMBER_LIST: u8 = 11;

pub fn name(kind: u8, data: u32, text: &str) -> Vec<u8> {
    packet(NAME, |w| {
        w.u32(data).u8(kind);
        strings(w, &[text]);
    })
}

pub fn talk(channel: u16, speaker: &str, hearer: &str, words: &str, colour: u32) -> Vec<u8> {
    packet(TALK, |w| {
        w.u32(colour).u16(channel).u16(0).u32(0).u32(0).u32(0);
        let words: String = words.chars().take(255).collect();
        strings(w, &[speaker, hearer, "", &words]);
    })
}

/// The hero, as 1006 describes it.
pub struct HeroInfo<'a> {
    pub id: u32,
    pub look: u32,
    pub hair: u16,
    pub money: u32,
    pub exp: u32,
    pub life: u16,
    pub mana: u16,
    pub level: u8,
    pub profession: u8,
    /// Strength, agility, vitality and spirit: the client derives max life from them.
    pub points: [u16; 4],
    pub name: &'a str,
}

pub fn user_info(h: &HeroInfo) -> Vec<u8> {
    packet(USER_INFO, |w| {
        w.u32(h.id)
            .u32(h.look)
            .u16(h.hair)
            .u32(h.money)
            .u32(0)
            .u32(h.exp);
        // Fields up to the attributes (26..50) stay 0; no unspent points.
        w.zeros(50 - 26);
        for p in h.points {
            w.u16(p);
        }
        w.u16(0);
        w.u16(h.life)
            .u16(h.mana)
            .i16(0)
            .u8(h.level)
            .u8(h.profession)
            .u8(0)
            .u8(0)
            .u8(0);
        strings(w, &[h.name, "None"]);
    })
}

/// An item in the hero's bag (`position` 0) or worn (its slot).
/// MsgItemInfo actions: a new item, and one that changed.
pub const ITEM_ADD: u8 = 1;
/// MsgItemInfo's action of another player's worn item (its id that player's).
pub const ITEM_OTHER_EQUIPMENT: u8 = 4;
pub const ITEM_UPDATE: u8 = 3;

/// One item as MsgItemInfo (1008) shows it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ItemInfo {
    pub id: u32,
    pub kind: u32,
    /// Durability (in hundredths) or how many are stacked, and its most.
    pub amount: u16,
    pub limit: u16,
    pub action: u8,
    pub ident: u8,
    /// 0 the bag, else the equipment slot.
    pub position: u8,
    pub data: u32,
    pub gem1: u8,
    pub gem2: u8,
    /// The `+` level (`magic3`).
    pub plus: u8,
    pub bless: u8,
    pub enchant: u8,
    pub color: u16,
    /// 0 free, 1 locked, 3 unlocking (MsgItemInfo @38).
    pub lock: u16,
}

pub fn item_info(i: &ItemInfo) -> Vec<u8> {
    packet(ITEM_INFO, |w| {
        w.u32(i.id)
            .u32(i.kind)
            .u16(i.amount)
            .u16(i.limit)
            .u8(i.action)
            .u8(i.ident)
            .u8(i.position)
            .u8(0)
            .u32(i.data)
            .u8(i.gem1)
            .u8(i.gem2)
            .u8(0)
            .u8(0)
            .u8(i.plus)
            .u8(i.bless)
            .u8(i.enchant)
            .u8(0)
            .u32(0)
            .u16(0)
            .u16(i.lock)
            .u16(i.color)
            .u16(0);
    })
}

/// MsgEquipLock 2048 (16 bytes): item, action, lock status, the unlock day (YYYYMMDD).
pub fn item_lock(item: u32, action: u8, status: u8, until: u32) -> Vec<u8> {
    packet(ITEM_LOCK, |w| {
        w.u32(item).u8(action).u8(status).u16(0).u32(until);
    })
}

/// MsgMessageBoard (1111) action 3: a page of board `channel`'s posts from `index`, each its
/// author, its words (31 letters) and its time.
pub fn board_list(index: u16, channel: u16, posts: &[(&str, &str, &str)]) -> Vec<u8> {
    packet(MESSAGE_BOARD, |w| {
        w.u16(index).u16(channel).u8(board::LIST);
        let words: Vec<String> = posts
            .iter()
            .map(|p| p.1.chars().take(31).collect())
            .collect();
        let mut list = Vec::new();
        for (post, words) in posts.iter().zip(&words) {
            list.extend([post.0, words.as_str(), post.2]);
        }
        strings(w, &list);
    })
}

/// A message board post read: MsgTalk on the board's channel with its time as the suffix.
pub fn board_words(channel: u16, author: &str, words: &str, time: &str) -> Vec<u8> {
    packet(TALK, |w| {
        w.u32(0x00FF_FFFF).u16(channel).u16(0).u32(0).u32(0).u32(0);
        let words: String = words.chars().take(255).collect();
        strings(w, &[author, "ALLUSERS", time, &words]);
    })
}

pub const MESSAGE_BOARD: u16 = 1111;
/// MsgMessageBoard actions (MsgBoard.h); the boards' talk channels (_TXTATR_MSG_TRADE on).
pub mod board {
    pub const DEL: u8 = 1;
    pub const GET_LIST: u8 = 2;
    pub const LIST: u8 = 3;
    pub const GET_WORDS: u8 = 4;
    pub const FIRST_CHANNEL: u16 = 2201;
    pub const PAGE: usize = 8;
}

/// MsgItemInfoEx (1108): booth `owner`'s ware `i` at `price` (mode 1 silver, 3 CPs).
pub fn item_info_ex(i: &ItemInfo, owner: u32, price: u32, mode: u8) -> Vec<u8> {
    packet(ITEM_INFO_EX, |w| {
        w.u32(i.id)
            .u32(owner)
            .u32(price)
            .u32(i.kind)
            .u16(i.amount)
            .u16(i.limit)
            .u8(mode)
            .u8(i.ident)
            .u8(0)
            .u8(0)
            .u32(0)
            .u8(i.gem1)
            .u8(i.gem2)
            .u8(0)
            .u8(0)
            .u8(i.plus)
            .u8(i.bless)
            .u8(i.enchant)
            .u8(0)
            .u32(i.data)
            .u16(0)
            .u16(i.lock)
            .u16(i.color)
            .u16(0);
    })
}

pub const ITEM_INFO_EX: u16 = 1108;
/// MsgItemInfoEx modes: a booth's ware priced in silver, in CPs.
pub const ITEM_EX_BOOTH: u8 = 1;
pub const ITEM_EX_BOOTH_CPS: u8 = 3;

/// MsgTrade `action` with `data`.
pub fn trade(data: u32, action: u16) -> Vec<u8> {
    packet(TRADE, |w| {
        w.u32(data).u16(action).u16(0);
    })
}

/// One stored item of a warehouse list (24 bytes).
pub struct Stored {
    pub id: u32,
    pub kind: u32,
    pub ident: u8,
    pub gem1: u8,
    pub gem2: u8,
    pub plus: u8,
    pub bless: u8,
    pub enchant: u16,
    pub color: u8,
}

/// A storage NPC's warehouse list.
pub fn package(npc: u32, kind: u8, items: &[Stored]) -> Vec<u8> {
    packet(PACKAGE, |w| {
        w.u32(npc).u8(PACKAGE_QUERY).u8(kind).u16(0);
        w.u32(items.len().min(50) as u32);
        for i in items.iter().take(50) {
            w.u32(i.id).u32(i.kind);
            w.u8(i.ident)
                .u8(i.gem1)
                .u8(i.gem2)
                .u8(0)
                .u8(0)
                .u8(i.plus)
                .u8(i.bless)
                .u8(0);
            w.u16(i.enchant).u16(0).u16(0).u8(0).u8(i.color);
        }
    })
}

pub fn item_action(id: u32, data: u32, action: u16) -> Vec<u8> {
    packet(ITEM, |w| {
        w.u32(id).u32(data).u16(action).u16(0).u32(0).u32(0);
    })
}

/// MsgAction `action` about `id`.
pub fn action(id: u32, action: u16, data: u32, cell: (u16, u16), dir: u16) -> Vec<u8> {
    packet(ACTION, |w| {
        w.u32(0)
            .u32(id)
            .u32(data)
            .u16(cell.0)
            .u16(cell.1)
            .u16(dir)
            .u16(action);
    })
}

pub fn walk(id: u32, dir: u8, run: bool) -> Vec<u8> {
    packet(WALK, |w| {
        w.u32(id).u8(dir).u8(run.into()).u16(0);
    })
}

/// A player or monster coming into view (1014).
pub struct Spawn<'a> {
    pub id: u32,
    pub look: u32,
    pub status: u32,
    /// Helmet, armor, right and left weapon types.
    pub gear: [u32; 4],
    pub life: u16,
    /// For monsters, at 50 (players have it at 62).
    pub level: u16,
    pub cell: (u16, u16),
    pub hair: u16,
    pub dir: u8,
    pub monster: bool,
    /// A player's guild id, its rank in the top byte.
    pub guild: u32,
    pub name: &'a str,
}

pub fn spawn(s: &Spawn) -> Vec<u8> {
    packet(PLAYER, |w| {
        w.u32(s.id)
            .u32(s.look)
            .u32(s.status)
            .u32(0)
            .u32(s.guild)
            .u32(0);
        for g in s.gear {
            w.u32(g);
        }
        w.u16(0)
            .u16(0)
            .u16(s.life)
            .u16(if s.monster { s.level } else { 0 });
        w.u16(s.cell.0)
            .u16(s.cell.1)
            .u16(s.hair)
            .u8(s.dir)
            .u8(100)
            .u8(0)
            .u8(0);
        w.u16(if s.monster { 0 } else { s.level }).u8(0).zeros(3);
        w.zeros(90 - 68);
        strings(w, &[s.name]);
        w.u8(0);
    })
}

/// Attribute changes of `id`.
pub fn attributes(id: u32, values: &[(u32, u64)]) -> Vec<u8> {
    packet(ATTRIB, |w| {
        w.u32(id).u32(values.len() as u32);
        for &(kind, value) in values {
            w.u32(kind).u64(value);
        }
    })
}

/// A weapon skill: the weapon subtype, its level and experience (MsgWeaponSkill 1025).
pub const WEAPON_SKILL: u16 = 1025;
pub fn weapon_skill(kind: u32, level: u16, exp: u32) -> Vec<u8> {
    packet(WEAPON_SKILL, |w| {
        w.u32(kind).u16(level).u16(0).u32(exp);
    })
}

/// A learned skill: its type, level and experience (MsgMagicInfo 1103).
pub const MAGIC_INFO: u16 = 1103;
pub fn magic_info(kind: u16, level: u16, exp: u32) -> Vec<u8> {
    packet(MAGIC_INFO, |w| {
        w.u32(exp).u16(kind).u16(level);
    })
}

/// A spell cast by `caster` at a role or cell, with what it did to each role reached
/// (MsgMagicEffect 1105).
pub const MAGIC_EFFECT: u16 = 1105;
pub fn magic_effect(caster: u32, at: u32, kind: u16, level: u16, hits: &[(u32, u32)]) -> Vec<u8> {
    let hits = &hits[..hits.len().min(255)];
    packet(MAGIC_EFFECT, |w| {
        w.u32(caster).u32(at).u16(kind).u16(level);
        w.u8(hits.len() as u8).u8(0).u8(0).u8(0);
        for (role, damage) in hits {
            w.u32(*role).u32(*damage).i32(0);
        }
    })
}

pub fn interact(kind: u16, sender: u32, target: u32, cell: (u16, u16), data: u32) -> Vec<u8> {
    packet(INTERACT, |w| {
        w.u32(0)
            .u32(sender)
            .u32(target)
            .u16(cell.0)
            .u16(cell.1)
            .u16(kind)
            .u16(0)
            .u32(data)
            .i32(0);
    })
}

/// Item types money piles show as.
pub fn money_look(amount: u64) -> u32 {
    match amount {
        0..11 => 1_090_000,
        11..101 => 1_090_010,
        101..1001 => 1_090_020,
        1001..2001 => 1_091_000,
        2001..=5000 => 1_091_010,
        _ => 1_091_020,
    }
}

/// A ground item: `action` 1 shows it, 2 removes it.
pub fn map_item(id: u32, kind: u32, cell: (u16, u16), action: u8) -> Vec<u8> {
    packet(MAP_ITEM, |w| {
        w.u32(id)
            .u32(kind)
            .u16(cell.0)
            .u16(cell.1)
            .u16(0)
            .u8(action)
            .u8(0);
    })
}

/// A system NPC (2030); `name` replaces the one the client's NPC type table gives.
pub fn npc_info(id: u32, cell: (u16, u16), look: u16, kind: u16, name: &str) -> Vec<u8> {
    packet(NPC_INFO, |w| {
        w.u32(id).u16(cell.0).u16(cell.1).u16(look).u16(kind).u16(0);
        strings(w, &[name]);
    })
}

/// MsgNpcInfoEx: an NPC that can be hit, with its life (TQ's FUN_00483fa9; the name only
/// for a guild flag, kind 10).
pub fn npc_info_ex(
    id: u32,
    hit_points: (u32, u32),
    cell: (u16, u16),
    look: u16,
    kind: u16,
    sort: u16,
    name: &str,
) -> Vec<u8> {
    packet(NPC_INFO_EX, |w| {
        w.u32(id).u32(hit_points.0).u32(hit_points.1);
        w.u16(cell.0).u16(cell.1).u16(look).u16(kind).u16(sort);
        if kind == GUILD_FLAG {
            strings(w, &[name]);
        } else {
            w.u16(0);
        }
    })
}

/// The kind of a guild-war pole, the only NPC whose MsgNpcInfoEx carries a name.
const GUILD_FLAG: u16 = 10;

/// MsgNpc EVENT_LAYNPC: the player asked to place an NPC of `look` and `kind` (TQ's 401).
pub fn lay_npc(look: u32, kind: u16) -> Vec<u8> {
    packet(NPC, |w| {
        w.u32(0).u32(look).u16(LAY_NPC).u16(kind);
    })
}

/// MsgNpc's action asking the client to place an NPC.
const LAY_NPC: u16 = 5;

/// MsgNpc EVENT_LEAVEMAP: NPC `id` (a booth) leaves the client's map.
pub fn npc_gone(id: u32) -> Vec<u8> {
    packet(NPC, |w| {
        w.u32(id).u32(0).u16(NPC_LEAVE_MAP).u16(0);
    })
}

const NPC_LEAVE_MAP: u16 = 2;

/// A booth NPC (_ROLE_BOOTH_NPC) and its look (400 and its facing).
pub const BOOTH_NPC: u16 = 14;
pub const BOOTH_LOOK: u16 = 400;

/// One part of an NPC dialog.
pub fn dialog(action: u8, index: u8, text: &str) -> Vec<u8> {
    dialog_with(action, index, 0, text)
}

/// A MsgTaskDialog line with `data` (a text box's length).
pub fn dialog_with(action: u8, index: u8, data: u16, text: &str) -> Vec<u8> {
    packet(DIALOG, |w| {
        w.u32(0).u16(data).u8(index).u8(action);
        if text.is_empty() {
            strings(w, &[]);
        } else {
            // `~` shows as a space; a line holds 255 bytes.
            let text: String = text.replace(' ', "~").chars().take(250).collect();
            strings(w, &[&text]);
        }
    })
}

/// What a client asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Nobility (2064): donating, a page of the board, the hero's standing, a rank's price.
    Peerage {
        action: u16,
        data: u64,
    },
    /// A broadcast asked about or paid for (2050): the action, its data and the line.
    Pigeon {
        action: u8,
        data: u32,
        text: String,
    },
    /// A mentor action (2065): the action, two players' ids and its data.
    Mentor {
        action: u8,
        first: u32,
        second: u32,
        data: u32,
    },
    /// The hero's mentor relations asked for (2066), or what its students gave (2067).
    MentorMembers,
    MentorPrize {
        action: u8,
    },
    /// Offline training asked of (2044): its action.
    Training(u16),
    /// A dice table (1113): a chip of `amount` on bet `kind`, or the bets taken back.
    Dice {
        action: u8,
        npc: u32,
        kind: u8,
        amount: u32,
    },
    /// An item locked, unlocked or its unlock day asked (2048).
    ItemLock {
        item: u32,
        action: u8,
    },
    /// A trade partner asked about (2046): the action and the player's or partner's id.
    Business {
        action: u8,
        id: u32,
    },
    /// A message board asked about (1111): its channel (2201-2205), the first post of a
    /// page, the action (1 take down, 2 list, 4 read) and a name (whose post).
    Board {
        index: u16,
        channel: u16,
        action: u8,
        name: String,
    },
    /// Account login (1051): the password RC5-sealed.
    Account {
        account: String,
        password: [u8; 16],
    },
    /// A friend or enemy asked about (1019): the action and the player's or friend's id.
    Friend {
        action: u8,
        id: u32,
    },
    /// Game login (1052) with the token the account server gave.
    Connect {
        token: u32,
    },
    Action {
        action: u16,
        data: u32,
        cell: (u16, u16),
        dir: u16,
    },
    Walk {
        dir: u8,
        run: bool,
    },
    /// A MsgInteract: its kind, target, time stamp, cell and data words.
    Interact {
        kind: u16,
        target: u32,
        ts: u32,
        cell: (u16, u16),
        data: (u16, u16),
    },
    Item {
        action: u16,
        id: u32,
        data: u32,
        /// The dword at 0x14 (CMsgItem::CreateWithExtra: how many a CP-shop buy is for).
        extra: u32,
    },
    MapItem {
        action: u8,
        id: u32,
    },
    Npc {
        action: u16,
        id: u32,
    },
    /// Where the player placed the NPC it was asked to (MsgNpcInfo from the client).
    Placed {
        cell: (u16, u16),
        look: u16,
    },
    Dialog {
        action: u8,
        index: u8,
        /// What was typed in a text box.
        text: String,
    },
    Talk {
        channel: u16,
        /// Who it is for (a whisper's name; `ALLUSERS` otherwise).
        hearer: String,
        words: String,
    },
    /// A gem set in (`remove` false) or taken from socket `pos` of `item`.
    GemEmbed {
        item: u32,
        gem: u32,
        pos: u16,
        remove: bool,
    },
    /// Composing: the main item first, then the minors and gems.
    Compose {
        items: Vec<u32>,
    },
    /// Attribute points spent: strength, agility, vitality, spirit (MsgAllot).
    Allot([u8; 4]),
    /// A MsgName asked for: its type and number (the guild member list's page, 11).
    Name {
        kind: u8,
        data: u32,
    },
    /// Items fused into a +1 stone (`kind` 0; TQ knows no other).
    Fuse {
        kind: u16,
        items: Vec<u32>,
    },
    /// A guild action about a guild or player.
    Guild {
        action: u16,
        id: u32,
    },
    /// A team action about a player.
    Team {
        action: u16,
        id: u32,
    },
    /// A trade action with its number (a player, an item, money).
    Trade {
        action: u16,
        data: u32,
    },
    /// A storage NPC's warehouse: listed, an item stored or taken out.
    Package {
        npc: u32,
        action: u8,
        kind: u8,
        item: u32,
    },
    /// The client's ping (MsgItem 27), answered by sending it back unchanged.
    Ping(Vec<u8>),
    /// A packet the front doesn't use.
    Other(u16),
}

/// Reads a client packet.
pub fn parse(bytes: &[u8]) -> Result<Request, PacketError> {
    let kind = packet_type(bytes)?;
    let mut r = Reader::new(&bytes[4..]);
    Ok(match kind {
        ACCOUNT => {
            let account = r.fixed_str(16)?;
            let password = r.take(16)?.try_into().expect("16 bytes");
            Request::Account { account, password }
        }
        CONNECT => Request::Connect { token: r.u32()? },
        ACTION => {
            r.skip(8)?;
            let data = r.u32()?;
            let cell = (r.u16()?, r.u16()?);
            let dir = r.u16()?;
            Request::Action {
                action: r.u16()?,
                data,
                cell,
                dir,
            }
        }
        WALK => {
            r.skip(4)?;
            Request::Walk {
                dir: r.u8()? % 8,
                run: r.u8()? != 0,
            }
        }
        INTERACT => {
            let ts = r.u32()?;
            r.skip(4)?;
            let target = r.u32()?;
            let cell = (r.u16()?, r.u16()?);
            let kind = r.u16()?;
            r.skip(2)?;
            Request::Interact {
                kind,
                target,
                ts,
                cell,
                data: (r.u16()?, r.u16()?),
            }
        }
        ITEM => {
            let (id, data) = (r.u32()?, r.u32()?);
            match r.u16()? {
                item::PING => Request::Ping(bytes.to_vec()),
                action => {
                    let extra = r.skip(6).and_then(|()| r.u32()).unwrap_or(0);
                    Request::Item {
                        action,
                        id,
                        data,
                        extra,
                    }
                }
            }
        }
        GUILD => {
            let action = r.u16()?;
            r.skip(2)?;
            Request::Guild {
                action,
                id: r.u32()?,
            }
        }
        FRIEND => {
            let id = r.u32()?;
            Request::Friend {
                action: r.u8()?,
                id,
            }
        }
        TEAM => {
            let action = r.u16()?;
            r.skip(2)?;
            Request::Team {
                action,
                id: r.u32()?,
            }
        }
        TRADE => {
            let data = r.u32()?;
            Request::Trade {
                action: r.u16()?,
                data,
            }
        }
        PACKAGE => {
            let npc = r.u32()?;
            let action = r.u8()?;
            let kind = r.u8()?;
            r.skip(2)?;
            Request::Package {
                npc,
                action,
                kind,
                item: r.u32()?,
            }
        }
        ALLOT => Request::Allot([r.u8()?, r.u8()?, r.u8()?, r.u8()?]),
        NAME => {
            let data = r.u32()?;
            Request::Name {
                kind: r.u8()?,
                data,
            }
        }
        FUSE => {
            r.skip(4)?;
            let kind = r.u16()?;
            let count = usize::from(r.u16()?).min(20);
            let items = (0..count)
                .map(|_| r.u32())
                .collect::<Result<Vec<u32>, _>>()?;
            Request::Fuse { kind, items }
        }
        GEM_EMBED => {
            r.skip(4)?;
            let (item, gem) = (r.u32()?, r.u32()?);
            let pos = r.u16()?;
            Request::GemEmbed {
                item,
                gem,
                pos,
                remove: r.u16()? == 1,
            }
        }
        COMPOSE => match r.u8()? {
            0 => {
                let count = usize::from(r.u8()?).min(5);
                r.skip(2)?;
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    items.push(r.u32()?);
                }
                Request::Compose { items }
            }
            _ => Request::Other(kind),
        },
        MAP_ITEM => {
            let id = r.u32()?;
            r.skip(10)?;
            Request::MapItem {
                action: r.u8()?,
                id,
            }
        }
        NPC_INFO => {
            r.skip(4)?;
            let cell = (r.u16()?, r.u16()?);
            Request::Placed {
                cell,
                look: r.u16()?,
            }
        }
        NPC => {
            let id = r.u32()?;
            r.skip(4)?;
            Request::Npc {
                action: r.u16()?,
                id,
            }
        }
        DIALOG => {
            r.skip(6)?;
            let index = r.u8()?;
            let action = r.u8()?;
            // The first of its strings: what was typed in a text box.
            let text = (r.u8().ok().filter(|&n| n > 0)).and_then(|_| r.short_str().ok());
            Request::Dialog {
                action,
                index,
                text: text.unwrap_or_default(),
            }
        }
        MENTOR => {
            let action = r.u8()?;
            r.skip(3)?;
            Request::Mentor {
                action,
                first: r.u32()?,
                second: r.u32()?,
                data: r.u32()?,
            }
        }
        MENTOR_MEMBER => Request::MentorMembers,
        PEERAGE => {
            let action = r.u16()?;
            r.skip(2)?;
            let low = u64::from(r.u32()?);
            let high = u64::from(r.u32()?);
            Request::Peerage {
                action,
                data: low | high << 32,
            }
        }
        PIGEON => {
            let action = r.u8()?;
            r.skip(3)?;
            let data = r.u32()?;
            let text = (r.u8().ok().filter(|&n| n > 0)).and_then(|_| r.short_str().ok());
            Request::Pigeon {
                action,
                data,
                text: text.unwrap_or_default(),
            }
        }
        MENTOR_PRIZE => Request::MentorPrize { action: r.u8()? },
        TRAINING => Request::Training(r.u16()?),
        DICE => {
            let action = r.u8()?;
            r.skip(3)?;
            let npc = r.u32()?;
            let kind = r.u8()?;
            r.skip(3)?;
            Request::Dice {
                action,
                npc,
                kind,
                amount: r.u32()?,
            }
        }
        ITEM_LOCK => {
            let item = r.u32()?;
            Request::ItemLock {
                item,
                action: r.u8()?,
            }
        }
        BUSINESS => {
            let id = r.u32()?;
            Request::Business {
                action: r.u8()?,
                id,
            }
        }
        MESSAGE_BOARD => {
            let index = r.u16()?;
            let channel = r.u16()?;
            let action = r.u8()?;
            let name = (r.u8().ok().filter(|&n| n > 0)).and_then(|_| r.short_str().ok());
            Request::Board {
                index,
                channel,
                action,
                name: name.unwrap_or_default(),
            }
        }
        TALK => {
            r.skip(4)?;
            let channel = r.u16()?;
            r.skip(14)?;
            let count = r.u8()?;
            let mut list = Vec::new();
            for _ in 0..count {
                list.push(r.short_str()?);
            }
            Request::Talk {
                channel,
                hearer: list.get(1).cloned().unwrap_or_default(),
                words: list.get(3).cloned().unwrap_or_default(),
            }
        }
        other => Request::Other(other),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_match_the_layouts() {
        assert_eq!(login_reply(1, 2, "127.0.0.1", 5816).len(), 32);
        assert_eq!(item_info(&ItemInfo::default()).len(), 44);
        assert_eq!(item_action(1, 2, 3).len(), 24);
        assert_eq!(action(1, 74, 1002, (5, 6), 0).len(), 24);
        assert_eq!(walk(1, 2, false).len(), 12);
        assert_eq!(interact(2, 1, 2, (3, 4), 5).len(), 32);
        assert_eq!(map_item(1, 2, (3, 4), 1).len(), 20);
        assert_eq!(attributes(1, &[(0, 5)]).len(), 24);
        let hero = HeroInfo {
            id: 1_000_001,
            look: 11003,
            hair: 310,
            money: 5,
            exp: 0,
            life: 100,
            mana: 0,
            level: 1,
            profession: 10,
            points: [5, 2, 3, 0],
            name: "Hero",
        };
        // 71 + count + (len + "Hero") + (len + "None").
        assert_eq!(user_info(&hero).len(), 71 + 1 + 5 + 5);
        let s = Spawn {
            id: 400_001,
            look: 101,
            status: 0,
            gear: [0; 4],
            life: 30,
            level: 3,
            cell: (1, 2),
            hair: 0,
            dir: 0,
            monster: true,
            guild: 0,
            name: "Pheasant",
        };
        assert_eq!(spawn(&s).len(), 90 + 1 + 9 + 1);
    }

    #[test]
    fn offline_training_talks_in_12_and_16_bytes() {
        let offer = training(training::ASK, 900);
        assert_eq!(
            (offer.len(), &offer[4..6], &offer[8..12]),
            (12, &[0, 0][..], &900_u32.to_le_bytes()[..])
        );
        let info = training_info(120, 780, 15, 8_800_000);
        assert_eq!(info.len(), 16);
        assert_eq!(&info[4..10], &[120, 0, 0x0C, 3, 15, 0]);
        assert_eq!(&info[12..16], &8_800_000_u32.to_le_bytes());
        assert_eq!(
            parse(&training(training::CLAIM, 0)).unwrap(),
            Request::Training(training::CLAIM)
        );
    }

    #[test]
    fn equipment_locks_ask_and_answer_in_16_bytes() {
        let answer = item_lock(7, item_lock::CANCEL, 3, 20_261_025);
        assert_eq!(answer.len(), 16);
        assert_eq!(&answer[2..4], &ITEM_LOCK.to_le_bytes());
        assert_eq!(&answer[12..16], &20_261_025_u32.to_le_bytes());
        assert_eq!(
            parse(&item_lock(7, item_lock::APPLY, 0, 0)).unwrap(),
            Request::ItemLock {
                item: 7,
                action: item_lock::APPLY
            }
        );
    }

    #[test]
    fn a_cp_shop_buy_names_how_many() {
        // MsgItem 1009 as CMsgItem::CreateWithExtra builds it: shop 432, a Meteor, buy, 5.
        let mut b = vec![24, 0, 0xF1, 3];
        b.extend(432_u32.to_le_bytes());
        b.extend(1_088_001_u32.to_le_bytes());
        b.extend(item::BUY.to_le_bytes());
        b.extend([0; 6]);
        b.extend(5_u32.to_le_bytes());
        let request = Request::Item {
            action: item::BUY,
            id: 432,
            data: 1_088_001,
            extra: 5,
        };
        assert_eq!(parse(&b).unwrap(), request);
        assert!(matches!(
            parse(&b[..16]).unwrap(),
            Request::Item { extra: 0, .. }
        ));
    }

    #[test]
    fn client_packets_parse() {
        let walk_req = walk(1_000_001, 10, true);
        assert_eq!(
            parse(&walk_req).unwrap(),
            Request::Walk { dir: 2, run: true }
        );
        let act = action(1_000_001, 74, 0, (0, 0), 0);
        assert!(matches!(
            parse(&act).unwrap(),
            Request::Action { action: 74, .. }
        ));
        let chat = talk(channel::TALK, "Hero", "ALL", "hi there", 0xFFFFFF);
        assert_eq!(
            parse(&chat).unwrap(),
            Request::Talk {
                channel: channel::TALK,
                hearer: "ALL".into(),
                words: "hi there".into()
            }
        );
        let mut login = Writer::new();
        login
            .u16(52)
            .u16(ACCOUNT)
            .fixed_str("alice", 16)
            .zeros(16)
            .fixed_str("Dark", 16);
        assert!(
            matches!(parse(&login.into_bytes()).unwrap(), Request::Account { account, .. } if account == "alice")
        );
    }
}
