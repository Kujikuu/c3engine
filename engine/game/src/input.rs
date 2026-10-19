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
    /// Walk up to NPC `id` and talk.
    Talk(u32),
    /// Pick answer `n` of the dialogue.
    Answer(usize),
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
    /// Take off worn item `id`.
    Unequip(u32),
    /// A chat line; lines starting with `/` are commands.
    Say(String),
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
            Input::Talk(npc) => self.talk(id, npc),
            Input::Answer(n) => self.answer(id, n),
            Input::CloseDialogue => self.close_dialogue(id),
            Input::Buy { npc, kind } => self.buy(id, npc, kind),
            Input::Sell { npc, item } => self.sell(id, npc, item),
            Input::CloseShop => self.close_shop(id),
            Input::Revive => self.revive(id),
            Input::Ghost => self.become_ghost(id),
            Input::Use(item) => self.use_item(id, item),
            Input::UseOn { item, target } => self.use_item_on(id, item, target),
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
            Input::Say(line) => {
                let text: String = line.trim().chars().take(MAX_CHAT).collect();
                if let Some(p) = self.players.get(&id).filter(|_| !text.is_empty()) {
                    let name = p.name.clone();
                    self.events.push(Event::Chat {
                        player: id,
                        name,
                        text,
                    });
                }
            }
        }
    }
}
