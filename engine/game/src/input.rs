//! What a player asks of the game, as one value: the local session hands these to its
//! simulation, a client sends them to the server, which hands them to its own.

use crate::EntityId;
use crate::event::Event;
use crate::sim::Sim;
use c3_scene::CellPos;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Input {
    /// Walk to a cell (or as near as it can get).
    Walk(CellPos),
    /// Jump to a cell (within `JUMP_RANGE`, onto walkable ground).
    Jump(CellPos),
    /// Walk to the portal on a cell and go through it.
    EnterPortal(CellPos),
    SetRunning(bool),
    Attack(EntityId),
    PickUp(EntityId),
    /// Make, join, leave or run a team.
    Team(crate::TeamAction),
    /// Join, leave or run a guild.
    Guild(crate::GuildAction),
    /// Drop bag item `0`, or this much money, where the player stands.
    Drop(u32),
    DropMoney(u64),
    /// Walk up to NPC `id` and talk.
    Talk(u32),
    /// Pick answer `n` of the dialogue.
    Answer(usize),
    /// Pick answer `index`, a text box, with what was typed.
    AnswerText {
        index: usize,
        text: String,
    },
    CloseDialogue,
    Buy {
        npc: u32,
        kind: u32,
    },
    Sell {
        npc: u32,
        item: u32,
    },
    CloseShop,
    /// Use or wear bag item `id`.
    Use(u32),
    /// Use bag item `item` on item `target` (bag or worn): a gem, an upgrade stone.
    UseOn {
        item: u32,
        target: u32,
    },
    /// Spend attribute points: so many on each attribute (the settings' point attributes).
    SpendPoints(Vec<(String, u32)>),
    /// Propose marriage to player `target`; accept the proposal of player `from`.
    Propose(EntityId),
    AcceptProposal(EntityId),
    /// Place what the player was asked to place (`PlacementAsked`) at `cell`, facing
    /// `direction`.
    Place {
        cell: CellPos,
        #[serde(default)]
        direction: u8,
    },
    /// Work items together (set a gem, compose, enchant): the game's kind of work, the items
    /// (up to five, the player's own) and a number (a socket); its rules answer.
    Combine {
        kind: u32,
        items: Vec<u32>,
        #[serde(default)]
        value: i64,
    },
    /// Wear bag item `item` in equipment slot `slot` (its type's, or its `alt_slot`).
    Wear {
        item: u32,
        slot: u8,
    },
    /// Bag item `item` stored with storage NPC `npc`; taken back out (`Retrieve`).
    Store {
        npc: u32,
        item: u32,
    },
    /// Ask player `0` to trade (asked back, the trade opens); offer a bag item, some money;
    /// accept; close the trade.
    TradeAsk(EntityId),
    TradeItem(u32),
    TradeMoney(u64),
    TradeAccept,
    TradeClose,
    /// See storage NPC `npc`'s storage (its items and the money kept).
    OpenStorage(u32),
    Retrieve {
        npc: u32,
        item: u32,
    },
    /// Money put into the storage, or taken from it, at storage NPC `npc`.
    SaveMoney {
        npc: u32,
        amount: u64,
    },
    DrawMoney {
        npc: u32,
        amount: u64,
    },
    /// Take off worn item `id`.
    Unequip(u32),
    /// A chat line; lines starting with `/` are commands.
    Say(String),
    /// Whom the player's attacks may hit among other players.
    SetPkMode(c3_core::settings::PkMode),
    /// Show emote `n` (the game's action number: a wave, a bow, sitting down).
    Emote(u32),
    /// A guild's leader sets what it tells its members.
    GuildNotice(String),
    /// Friends and enemies: asking, taking up, letting go.
    Friend(crate::FriendAction),
    /// Take `author`'s post (one's own; anyone's for a game master) off message board `board`.
    TakeDown {
        board: u8,
        #[serde(default)]
        author: String,
    },
    /// Trade partners: asking, turning down, ending.
    Partner(crate::PartnerAction),
    /// Booths: opening one, setting out wares, buying from one.
    Booth(crate::BoothAction),
    /// Say `text` on `channel` (to player `to` when whispering).
    Chat {
        channel: crate::ChatChannel,
        #[serde(default)]
        to: String,
        text: String,
    },
    /// Come back to life (once the wait is over), when the game waits to be asked.
    Revive,
    /// Get up as a ghost (dead, when ghosts walk).
    Ghost,
    /// Cast skill `skill` at `at`.
    Cast {
        skill: u32,
        at: crate::skills::CastAt,
    },
}

/// Longest chat line kept.
pub const MAX_CHAT: usize = 200;

impl Sim {
    /// Carries out what player `id` asked for.
    pub fn input(&mut self, id: EntityId, input: Input) {
        match input {
            Input::Walk(cell) => self.walk(id, cell),
            Input::Jump(cell) => self.jump(id, cell),
            Input::Cast { skill, at } => self.cast(id, skill, at),
            Input::EnterPortal(cell) => self.enter_portal(id, cell),
            Input::SetRunning(on) => self.set_running(id, on),
            Input::Attack(target) => self.attack(id, target),
            Input::PickUp(item) => self.pick_up(id, item),
            Input::Drop(item) => self.drop_item(id, item),
            Input::Team(action) => self.team(id, action),
            Input::Guild(action) => self.guild(id, action),
            Input::DropMoney(amount) => {
                if self.living(id).is_some() {
                    self.drop_money_of(id, i64::try_from(amount).unwrap_or(i64::MAX));
                }
            }
            Input::Talk(npc) => self.talk(id, npc),
            Input::Answer(n) => self.answer(id, n, None),
            Input::AnswerText { index, text } => self.answer(id, index, Some(text)),
            Input::CloseDialogue => self.close_dialogue(id),
            Input::Buy { npc, kind } => self.buy(id, npc, kind),
            Input::Sell { npc, item } => self.sell(id, npc, item),
            Input::CloseShop => self.close_shop(id),
            Input::Revive => self.revive(id),
            Input::SetPkMode(mode) => self.set_pk_mode(id, mode),
            Input::GuildNotice(text) => self.guild_notice(id, &text),
            Input::Friend(action) => self.friend_action(id, action),
            Input::Booth(action) => self.booth_action(id, action),
            Input::Partner(action) => self.partner_action(id, action),
            Input::TakeDown { board, author } => self.take_down(id, board, &author),
            Input::Emote(emote) => {
                if self.players.get(&id).is_some_and(|p| p.is_alive()) {
                    self.events.push(Event::Emoted { player: id, emote });
                }
            }
            Input::Ghost => self.become_ghost(id),
            Input::Use(item) => self.use_item(id, item),
            Input::Wear { item, slot } => self.wear_in(id, item, slot),
            Input::UseOn { item, target } => self.use_item_on(id, item, target),
            Input::Combine { kind, items, value } => self.combine(id, kind, &items, value),
            Input::SpendPoints(points) => self.spend_points(id, &points),
            Input::Propose(target) => self.propose(id, target),
            Input::AcceptProposal(from) => self.accept_proposal(id, from),
            Input::Place { cell, direction } => self.place_asked(id, cell, direction),
            Input::Store { npc, item } => self.store(id, npc, item),
            Input::OpenStorage(npc) => self.open_storage(id, npc),
            Input::TradeAsk(target) => self.ask_trade(id, target),
            Input::TradeItem(item) => self.offer_item(id, item),
            Input::TradeMoney(amount) => self.offer_money(id, amount),
            Input::TradeAccept => self.accept_trade(id),
            Input::TradeClose => self.close_trade(id),
            Input::Retrieve { npc, item } => self.retrieve(id, npc, item),
            Input::SaveMoney { npc, amount } => self.move_money(id, npc, amount, true),
            Input::DrawMoney { npc, amount } => self.move_money(id, npc, amount, false),
            Input::Unequip(item) => self.unequip(id, item),
            Input::Say(line) if line.starts_with('/') => {
                let (ok, text) = match self.command(id, &line) {
                    Ok(text) => (true, text),
                    Err(text) => (false, text),
                };
                self.events.push(Event::CommandReply {
                    player: id,
                    ok,
                    text,
                });
            }
            Input::Say(line) => self.chat(id, crate::ChatChannel::Local, "", &line),
            Input::Chat { channel, to, text } => self.chat(id, channel, &to, &text),
        }
    }
}
