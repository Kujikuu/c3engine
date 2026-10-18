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
mod menu;
mod quests;
mod shop;
pub mod style;

use c3_core::input::{Action, Device};
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
    /// Change what `action` is bound to on the active device (the next key or button).
    Rebind(Action),
    /// Back to the game's own controls.
    ResetControls,
    /// Leave the game (the client closes; the Studio stops playing).
    Leave,
}

/// One action on the Controls page.
pub struct ControlRow {
    pub action: Action,
    /// What it is bound to on the active device, as players read it (`Tab / Y`).
    pub bindings: String,
    /// Whether players may rebind it here.
    pub rebindable: bool,
}

/// The controls as the menu shows them.
pub struct ControlsView<'a> {
    /// The device in use.
    pub device: Device,
    pub rows: &'a [ControlRow],
    /// The action waiting for its new key or button.
    pub listening: Option<Action>,
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
    /// What the controls do, on the device in use (the top hint line).
    pub hint: &'a str,
    pub controls: &'a ControlsView<'a>,
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
    menu: Option<menu::Page>,
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

    /// Starts typing a chat line.
    pub fn open_chat(&mut self) {
        self.chat.open();
    }

    /// Opens or closes the game menu.
    /// Opens the menu on its Controls page.
    pub fn open_controls(&mut self) {
        self.menu = Some(menu::Page::Controls);
    }

    pub fn toggle_menu(&mut self) {
        self.menu = match self.menu {
            Some(_) => None,
            None => Some(menu::Page::Main),
        };
    }

    /// Whether a window wants the menu buttons (gamepads move between its buttons).
    pub fn window_open(&self, sim: &Sim, hero: EntityId) -> bool {
        let player = sim.player(hero);
        self.menu.is_some()
            || self.bag_open
            || self.quests_open
            || player.is_some_and(|p| p.dialogue.is_some() || p.shop.is_some())
    }

    /// Closes the topmost window (menu, dialogue, shop, bag, quests); `false` when none was
    /// open. A dialogue or shop closes through the returned action.
    pub fn back(&mut self, sim: &Sim, hero: EntityId) -> (bool, Option<UiAction>) {
        if let Some(page) = self.menu {
            self.menu = (page == menu::Page::Controls).then_some(menu::Page::Main);
            return (true, None);
        }
        let player = sim.player(hero);
        if player.is_some_and(|p| p.dialogue.is_some()) {
            return (true, Some(UiAction::CloseDialogue));
        }
        if player.is_some_and(|p| p.shop.is_some()) {
            return (true, Some(UiAction::CloseShop));
        }
        if self.bag_open {
            self.bag_open = false;
            return (true, None);
        }
        if self.quests_open {
            self.quests_open = false;
            return (true, None);
        }
        (false, None)
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
        hud::hint_line(ui, view);
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
        menu::window(ui, view.rect, &mut self.menu, view.controls, &mut actions);
        actions
    }
}
