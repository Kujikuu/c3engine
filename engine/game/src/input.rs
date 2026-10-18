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
    /// Take off worn item `id`.
    Unequip(u32),
    /// A chat line; lines starting with `/` are commands.
    Say(String),
}

/// Longest chat line kept.
pub const MAX_CHAT: usize = 200;

impl Sim {
    /// Carries out what player `id` asked for.
    pub fn input(&mut self, id: EntityId, input: Input) {
        match input {
            Input::Walk(cell) => self.walk(id, cell),
            Input::Jump(cell) => self.jump(id, cell),
            Input::SetRunning(on) => self.set_running(id, on),
            Input::Attack(target) => self.attack(id, target),
            Input::PickUp(item) => self.pick_up(id, item),
            Input::Talk(npc) => self.talk(id, npc),
            Input::Answer(n) => self.answer(id, n),
            Input::CloseDialogue => self.close_dialogue(id),
            Input::Buy { npc, kind } => self.buy(id, npc, kind),
            Input::Sell { npc, item } => self.sell(id, npc, item),
            Input::CloseShop => self.close_shop(id),
            Input::Use(item) => self.use_item(id, item),
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
