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
/// Names shown in the world: effects on a cell or a role, sounds (MsgName).
pub const NAME: u16 = 1015;
/// MsgName types: an effect on a map cell, on a role.
pub const NAME_MAP_EFFECT: u8 = 9;
pub const NAME_ROLE_EFFECT: u8 = 10;
/// Setting a gem in a socket or taking it out (items spec §6.1).
pub const GEM_EMBED: u16 = 1027;
/// Composing +N: the main item, two minors and the gems (items spec §7.1).
pub const COMPOSE: u16 = 2036;
pub const NPC_INFO: u16 = 2030;
pub const NPC: u16 = 2031;
pub const DIALOG: u16 = 2032;

/// Chat channels.
pub mod channel {
    pub const TALK: u16 = 2007;
    pub const SYSTEM: u16 = 2005;
    pub const ENTRANCE: u16 = 2101;
}

/// MsgAction actions.
pub mod action {
    pub const ENTER_MAP: u16 = 74;
    pub const GET_ITEMS: u16 = 75;
    pub const GET_FRIENDS: u16 = 76;
    pub const GET_PROFICIENCIES: u16 = 77;
    pub const GET_SPELLS: u16 = 78;
    pub const EMOTION: u16 = 81;
    /// The client stepped on a portal (data: its cell, x | y << 16).
    pub const CHANGE_MAP: u16 = 85;
    pub const FLY_MAP: u16 = 86;
    pub const LEVEL_UP: u16 = 92;
    /// The dead hero asks to come back (battle spec S9).
    pub const REVIVE: u16 = 94;
    pub const GET_GUILD: u16 = 97;
    pub const QUERY_PLAYER: u16 = 102;
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
}

/// MsgInteract kinds.
pub mod interact {
    pub const ATTACK: u16 = 2;
    pub const KILL: u16 = 14;
    pub const SHOOT: u16 = 25;
}

/// MsgItem actions.
pub mod item {
    pub const BUY: u16 = 1;
    pub const SELL: u16 = 2;
    pub const REMOVE: u16 = 3;
    pub const USE: u16 = 4;
    pub const EQUIP: u16 = 5;
    pub const UNEQUIP: u16 = 6;
    /// A DragonBall (data) on an item (id): its quality.
    pub const IMPROVE: u16 = 19;
    /// A Meteor (data) on an item (id): its level.
    pub const UPLEV: u16 = 20;
    pub const PING: u16 = 27;
    /// A gem (data) enchanting an item (id): its extra life (items spec §6.4).
    pub const ENCHANT: u16 = 28;
}

/// The client cues Conquer's front sends (`client_cue`): a MsgAction of an action type (with
/// its data), a MsgMapItem action at the hero's cell (with its id).
pub mod cue {
    pub const ACTION: &str = "msg_action";
    pub const MAP_ITEM: &str = "map_item";
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
            .u16(0)
            .u16(i.color)
            .u16(0);
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
    pub name: &'a str,
}

pub fn spawn(s: &Spawn) -> Vec<u8> {
    packet(PLAYER, |w| {
        w.u32(s.id).u32(s.look).u32(s.status).u32(0).u32(0).u32(0);
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
    /// Account login (1051): the password RC5-sealed.
    Account {
        account: String,
        password: [u8; 16],
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
    Interact {
        kind: u16,
        target: u32,
    },
    Item {
        action: u16,
        id: u32,
        data: u32,
    },
    MapItem {
        action: u8,
        id: u32,
    },
    Npc {
        action: u16,
        id: u32,
    },
    Dialog {
        action: u8,
        index: u8,
        /// What was typed in a text box.
        text: String,
    },
    Talk {
        channel: u16,
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
            r.skip(8)?;
            let target = r.u32()?;
            r.skip(4)?;
            Request::Interact {
                kind: r.u16()?,
                target,
            }
        }
        ITEM => {
            let (id, data) = (r.u32()?, r.u32()?);
            match r.u16()? {
                item::PING => Request::Ping(bytes.to_vec()),
                action => Request::Item { action, id, data },
            }
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
            name: "Pheasant",
        };
        assert_eq!(spawn(&s).len(), 90 + 1 + 9 + 1);
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
