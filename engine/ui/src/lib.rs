//! c3-ui: the in-game UI, drawn with egui over the game view: life, mana and experience,
//! the chat log, the bag with equipment, shops, NPC dialogue and the quest log, plus names,
//! damage numbers, drop labels and quest marks over the world. The client and the Studio's Play mode show the same UI.
//!
//! It reads the simulation ([`c3_game::Sim`]) and returns what the player asked for as
//! [`UiAction`]s; the host carries them out (on the local simulation now, as requests to the
//! server in M6). Placing things over the world and item icons come from the host through
//! [`GameView`] and [`Icons`].

mod bag;
mod chat;
mod dialogue;
pub mod fonts;
mod hud;
pub mod icons;
mod quests;
mod shop;
pub mod style;

use c3_game::{EntityId, Event, Sim};
use c3_scene::CellPos;
use egui::{Pos2, Rect, TextureId};

/// What the player asked the game for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiAction {
    /// Pick dialogue answer `n`.
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
    /// A chat line (commands start with `/`).
    Say(String),
    /// Mute or unmute the game's sound.
    ToggleSound,
}

/// A line for the host's console (script output, data problems).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogLine {
    Info(String),
    Error(String),
}

/// Item pictures, loaded by the host.
pub trait Icons {
    /// The icon of item type `kind`, if the game has one.
    fn item(&mut self, ctx: &egui::Context, kind: u32) -> Option<TextureId>;
}

/// The game being shown and how it sits on screen.
pub struct GameView<'a> {
    pub sim: &'a Sim,
    pub hero: EntityId,
    /// The game area.
    pub rect: Rect,
    /// Screen point above an entity's head.
    pub head: &'a dyn Fn(EntityId) -> Option<Pos2>,
    /// Screen point of a cell's centre.
    pub cell: &'a dyn Fn(CellPos) -> Pos2,
    /// Screen point above the head of an NPC standing on a cell.
    pub npc_head: &'a dyn Fn(CellPos) -> Pos2,
    /// The monster under the pointer.
    pub hovered: Option<EntityId>,
    /// The game's equipment slots and their names, in display order.
    pub slots: &'a [(u8, &'a str)],
    /// Whether sound is on; `None` when there is no sound device.
    pub sound: Option<bool>,
}

impl GameView<'_> {
    fn player(&self) -> Option<&c3_game::Player> {
        self.sim.player(self.hero)
    }

    fn item_name(&self, kind: u32) -> String {
        item_name(self.sim, kind)
    }
}

/// The name of item type `kind`.
pub fn item_name(sim: &Sim, kind: u32) -> String {
    sim.rules()
        .item(kind)
        .map_or_else(|| format!("item {kind}"), |t| t.name.clone())
}

/// The in-game UI's state: what is open and what was said.
#[derive(Default)]
pub struct GameUi {
    chat: chat::Chat,
    overlay: hud::Overlay,
    bag_open: bool,
    quests_open: bool,
    log: Vec<LogLine>,
}

impl GameUi {
    /// Whether the player is typing (game keys should wait).
    pub fn typing(&self) -> bool {
        self.chat.typing
    }

    pub fn toggle_bag(&mut self) {
        self.bag_open ^= true;
    }

    pub fn toggle_quests(&mut self) {
        self.quests_open ^= true;
    }

    /// A line from the game in the chat log ("Level up!").
    pub fn system(&mut self, text: impl Into<String>, color: egui::Color32) {
        self.chat.push(text.into(), color);
    }

    /// Lines for the host's console since the last call.
    pub fn take_log(&mut self) -> Vec<LogLine> {
        std::mem::take(&mut self.log)
    }

    /// Feedback for what the simulation did (`now`: UI time in seconds).
    pub fn events(&mut self, events: &[Event], sim: &Sim, hero: EntityId, now: f64) {
        for event in events {
            self.overlay.event(event, hero, now);
            match event {
                Event::Warning(text) => self.log.push(LogLine::Error(text.clone())),
                Event::ScriptLog(text) => self.log.push(LogLine::Info(format!("Script: {text}"))),
                Event::ShopOpened { player, .. } if *player == hero => self.bag_open = true,
                _ => self.chat.event(event, sim, hero),
            }
        }
    }

    /// Draws the UI over `view.rect` and returns what the player asked for.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        view: &GameView,
        icons: &mut dyn Icons,
    ) -> Vec<UiAction> {
        let mut actions = Vec::new();
        fonts::use_game_font(ui);
        self.overlay.show(ui, view);
        let Some(player) = view.player() else {
            return actions;
        };
        let typing = self.chat.typing;
        let free_keys = !typing && !ui.ctx().egui_wants_keyboard_input();
        let (bag_key, quest_key, escape) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::B),
                i.key_pressed(egui::Key::L),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if bag_key && free_keys {
            self.bag_open ^= true;
        }
        if quest_key && free_keys {
            self.quests_open ^= true;
        }
        quests::npc_marks(&ui.painter_at(view.rect), view);
        let mut open = hud::Open {
            bag: &mut self.bag_open,
            quests: &mut self.quests_open,
        };
        hud::bar(ui, view, player, &mut open, &mut actions);
        self.chat.show(ui, view.rect, &mut actions);
        let shop = player.shop;
        if let Some(npc) = shop {
            shop::window(ui, view, npc, icons, &mut actions);
        } else if self.quests_open {
            quests::window(ui, view, &mut self.quests_open);
        } else {
            quests::tracker(ui, view);
        }
        if self.bag_open {
            bag::window(ui, view, shop, icons, &mut self.bag_open, &mut actions);
        }
        if let Some(conversation) = &player.dialogue {
            dialogue::window(ui, view.rect, conversation, free_keys, &mut actions);
        }
        // Esc closes the topmost window: dialogue, then shop, then bag, then quests.
        if escape && !typing {
            if player.dialogue.is_some() {
                actions.push(UiAction::CloseDialogue);
            } else if shop.is_some() {
                actions.push(UiAction::CloseShop);
            } else if self.bag_open {
                self.bag_open = false;
            } else {
                self.quests_open = false;
            }
        }
        actions
    }
}
