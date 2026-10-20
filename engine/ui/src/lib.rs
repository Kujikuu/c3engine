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
pub mod front;
pub mod gui;
mod gui_data;
mod hud;
pub mod icons;
mod menu;
mod quests;
mod radar;
mod shop;
pub mod style;

use c3_core::input::{Action, Device};
use c3_game::{EntityId, Event, Player, Sim};
use c3_scene::CellPos;
use egui::{Pos2, Rect, TextureId};

/// What a click on slot `index` of grid or list `list` asks: a shop's ware bought; an
/// NPC's answer picked; a bag item sold to the open shop, else used or worn.
fn grid_click(
    data: &gui_data::HeroData,
    player: &Player,
    list: &str,
    index: usize,
) -> Option<UiAction> {
    let stored = |npc: u32| {
        player
            .storage
            .get(&npc)
            .and_then(|s| s.get(index))
            .map(|i| i.id)
    };
    // A worn item clicked is taken off (`hero.worn.<slot>`).
    if let Some(slot) = list
        .strip_prefix("hero.worn.")
        .and_then(|s| s.parse::<u8>().ok())
    {
        let item = (player.equipment.iter()).find(|i| data.view.sim.slot_of(i) == Some(slot))?;
        return Some(UiAction::Game(c3_game::Input::Unequip(item.id)));
    }
    // Trading, a bag item clicked is offered.
    if data.state.trade.is_some() && list == "hero.bag" {
        let item = player.bag.get(index)?.id;
        return Some(UiAction::Game(c3_game::Input::TradeItem(item)));
    }
    if let Some(npc) = data.state.storage {
        let input = match list {
            "storage.items" => c3_game::Input::Retrieve {
                npc,
                item: stored(npc)?,
            },
            "hero.bag" => c3_game::Input::Store {
                npc,
                item: player.bag.get(index)?.id,
            },
            _ => return None,
        };
        return Some(UiAction::Game(input));
    }
    match (list, player.shop) {
        ("shop.items", Some(npc)) => {
            let kind = *gui::GuiData::items(data, list).get(index)?;
            Some(UiAction::Buy { npc, kind })
        }
        // An answer that is typed sends what is in the text box.
        ("dialogue.answers", _) => {
            let page = &player.dialogue.as_ref()?.page;
            Some(match page.options.get(index)?.input {
                Some(_) => UiAction::Game(c3_game::Input::AnswerText {
                    index,
                    text: (data.state.fields.get("dialogue.typed").cloned()).unwrap_or_default(),
                }),
                None => UiAction::Answer(index),
            })
        }
        ("hero.bag", shop) => {
            let item = player.bag.get(index)?.id;
            Some(match shop {
                Some(npc) => UiAction::Sell { npc, item },
                None => UiAction::Use(item),
            })
        }
        _ => None,
    }
}

/// What the player asked the game for.
#[derive(Debug, Clone, PartialEq)]
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
    /// Play a sound (an asset path): the game UI's clicks.
    PlaySound(String),
    /// A game input as it is (the game UI's warehouse, ...).
    Game(c3_game::Input),
    /// Use or wear bag item `id`.
    Use(u32),
    /// Take off worn item `id`.
    Unequip(u32),
    /// Cast the skill in slot `n` of the skill bar.
    Cast(usize),
    /// A chat line (commands start with `/`).
    Say(String),
    /// Mute or unmute the game's sound.
    ToggleSound,
    /// Run, or walk again.
    ToggleRun,
    /// The player changed the Settings page (volumes, fullscreen).
    SetPrefs(c3_core::settings::PlayerPrefs),
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
    /// The player's volumes and window, for the Settings page.
    pub prefs: c3_core::settings::PlayerPrefs,
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
    /// The map drawn small, under the radar's dots.
    pub minimap: Option<RadarMap>,
}

/// A minimap for the radar: its texture covers world `bounds` (`[left, top, right,
/// bottom]`) of the map laid out by `projection`.
#[derive(Debug, Clone, Copy)]
pub struct RadarMap {
    pub texture: egui::TextureId,
    pub bounds: [f32; 4],
    pub projection: c3_scene::IsoProjection,
    /// The game's pictures for the hero, players and team leaders (texture and size).
    pub marks: RadarMarks,
}

/// The game's pictures for the radar's marks; none: a dot.
#[derive(Debug, Clone, Copy, Default)]
pub struct RadarMarks {
    pub hero: Option<(egui::TextureId, egui::Vec2)>,
    pub player: Option<(egui::TextureId, egui::Vec2)>,
    pub leader: Option<(egui::TextureId, egui::Vec2)>,
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
    /// The simulation asked the hero where to place an NPC (answered next frame).
    placing: bool,
    overlay: hud::Overlay,
    bag_open: bool,
    quests_open: bool,
    menu: Option<menu::Page>,
    log: Vec<LogLine>,
    /// The game's UI from data, drawn in place of the engine's bar when the game has one.
    gui: Option<gui::Gui>,
    /// The game UI's state: the storage NPC whose warehouse is open, what is typed in its
    /// fields.
    gui_state: gui_data::GuiState,
    /// Whether the hero is in a team, and leads it (as last drawn).
    in_team: bool,
    leads_team: bool,
    /// The control just clicked played its own sound.
    own_sound: bool,
    /// What the next click on a player does (`trade`, `team_invite`, ...), armed by a
    /// `pick:<what>` action.
    pick: Option<String>,
}

impl GameUi {
    /// Draws `gui` (the game's UI from data) in place of the engine's bar.
    pub fn set_gui(&mut self, gui: Option<gui::Gui>) {
        self.gui = gui;
    }

    /// What a click on `player` asks, when a `pick:` action armed it (taken: once).
    pub fn take_pick(&mut self, player: EntityId) -> Option<UiAction> {
        use c3_game::{GuildAction as G, Input, TeamAction as T};
        let input = match self.pick.take()?.as_str() {
            "trade" => Input::TradeAsk(player),
            "team_invite" => Input::Team(T::Invite(player)),
            "team_join" => Input::Team(T::Apply(player)),
            "team_kick" => Input::Team(T::Kick(player)),
            "guild_invite" => Input::Guild(G::Invite(player)),
            _ => return None,
        };
        Some(UiAction::Game(input))
    }

    /// Opens dialog `id` of the game's UI from data.
    pub fn open_dialog(&mut self, id: u32) {
        if let Some(gui) = &mut self.gui {
            gui.open(id);
        }
    }
    /// Carries out what the player clicked on the data-driven UI. Actions name the engine's
    /// windows (`bag`, `quests`, `menu`, `chat`), a game input (`run`, `sound`), or open a
    /// dialog of the UI (`toggle:<dialog id>`) or show one of a dialog's pages
    /// (`page:<page id>`).
    fn gui_event(&mut self, event: gui::GuiEvent, actions: &mut Vec<UiAction>) {
        let action = match event {
            gui::GuiEvent::Sound(path) => {
                actions.push(UiAction::PlaySound(path));
                self.own_sound = true;
                return;
            }
            gui::GuiEvent::Action(action) => {
                // The UI's click, unless the control played its own.
                if let Some(gui) = &self.gui
                    && !gui.sounds.click.is_empty()
                    && !std::mem::take(&mut self.own_sound)
                {
                    actions.push(UiAction::PlaySound(gui.sounds.click.clone()));
                }
                action
            }
            gui::GuiEvent::Edited { field, text } => {
                self.gui_state.fields.insert(field, text);
                return;
            }
            gui::GuiEvent::Item { .. } => return,
        };
        // Several actions, `;` between them, run in turn; a workbench clears after.
        for part in action.split(';') {
            self.run_action(part, actions);
        }
        if action.contains("combine:") {
            self.gui_state.work.clear();
        }
    }

    /// Carries out one action of the data-driven UI.
    fn run_action(&mut self, action: &str, actions: &mut Vec<UiAction>) {
        let storage = self.gui_state.storage;
        let typed = |s: &Self| -> u64 {
            let text = s.gui_state.fields.get("storage.amount");
            text.and_then(|t| t.trim().parse().ok()).unwrap_or(0)
        };
        match action {
            "storage_deposit" | "storage_withdraw" if storage.is_some() => {
                let (npc, amount) = (storage.expect("checked"), typed(self));
                actions.push(UiAction::Game(if action == "storage_deposit" {
                    c3_game::Input::SaveMoney { npc, amount }
                } else {
                    c3_game::Input::DrawMoney { npc, amount }
                }));
                self.gui_state.fields.remove("storage.amount");
            }
            "close_storage" => self.gui_state.storage = None,
            // Cast the hero's skill `n` (`cast:<n>`).
            _ if action.starts_with("cast:") => {
                if let Ok(slot) = action.trim_start_matches("cast:").parse() {
                    actions.push(UiAction::Cast(slot));
                }
            }
            _ if action.starts_with("pick:") => {
                let what = action.trim_start_matches("pick:");
                self.system(format!("Click a player ({what})."), style::GOLD);
                self.pick = Some(what.to_owned());
            }
            // Spend an attribute point (`spend:<attribute>`).
            _ if action.starts_with("spend:") => {
                let attribute = action.trim_start_matches("spend:").to_owned();
                actions.push(UiAction::Game(c3_game::Input::SpendPoints(vec![(
                    attribute, 1,
                )])));
            }
            // Show view `<name>` of view group `<group>` (`view:<group>:<name>`).
            _ if action.starts_with("view:") => {
                if let Some((group, name)) = action.trim_start_matches("view:").split_once(':') {
                    self.gui_state
                        .views
                        .insert(group.to_owned(), name.to_owned());
                }
            }
            _ if action.starts_with("guild_tab:") => {
                self.gui_state.guild_tab = action.trim_start_matches("guild_tab:").to_owned();
            }
            "trade_money" => {
                let amount = (self.gui_state.fields.remove("trade.amount"))
                    .and_then(|t| t.trim().parse().ok())
                    .unwrap_or(0);
                actions.push(UiAction::Game(c3_game::Input::TradeMoney(amount)));
            }
            "trade_accept" => actions.push(UiAction::Game(c3_game::Input::TradeAccept)),
            // The workbench's items worked together: `combine:<kind>[:<value>[:<slots>]]`, the
            // slots `0,1` (all when left out; none sent when one of them is empty).
            _ if action.starts_with("combine:") => {
                let mut parts = action
                    .strip_prefix("combine:")
                    .unwrap_or_default()
                    .split(':');
                let kind = parts.next().and_then(|k| k.parse().ok());
                let value = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                let work = &self.gui_state.work;
                let items: Option<Vec<u32>> = match parts.next() {
                    Some(slots) => (slots.split(','))
                        .map(|n| {
                            n.trim()
                                .parse::<usize>()
                                .ok()
                                .and_then(|n| work.get(n).copied())
                        })
                        .collect(),
                    None => Some(work.clone()),
                };
                if let (Some(kind), Some(items)) = (kind, items) {
                    actions.push(UiAction::Game(c3_game::Input::Combine {
                        kind,
                        items,
                        value,
                    }));
                }
            }
            // The team dialog's first button (CDlgGroup::OnGroupBtn1): make a team; a leader
            // invites the player clicked next; a member leaves.
            "team_main" if self.leads_team => self.run_action("pick:team_invite", actions),
            "team_main" => {
                let action = if self.in_team {
                    c3_game::TeamAction::Leave
                } else {
                    c3_game::TeamAction::Create
                };
                actions.push(UiAction::Game(c3_game::Input::Team(action)));
            }
            "guild_leave" => actions.push(UiAction::Game(c3_game::Input::Guild(
                c3_game::GuildAction::Leave,
            ))),
            "team_leave" => actions.push(UiAction::Game(c3_game::Input::Team(
                c3_game::TeamAction::Leave,
            ))),
            "trade_close" => actions.push(UiAction::Game(c3_game::Input::TradeClose)),
            "bag" => self.toggle_bag(),
            "quests" => self.toggle_quests(),
            "menu" => self.toggle_menu(),
            "chat" => self.open_chat(),
            "sound" => actions.push(UiAction::ToggleSound),
            "close_shop" => actions.push(UiAction::CloseShop),
            "close_dialogue" => actions.push(UiAction::CloseDialogue),
            "run" => actions.push(UiAction::ToggleRun),
            other => {
                let id = |prefix: &str| other.strip_prefix(prefix).and_then(|i| i.parse().ok());
                if let Some(gui) = &mut self.gui {
                    if let Some(dialog) = id("toggle:") {
                        let opened = gui.toggle(dialog);
                        let sound = if opened {
                            &gui.sounds.open
                        } else {
                            &gui.sounds.close
                        };
                        if !sound.is_empty() {
                            actions.push(UiAction::PlaySound(sound.clone()));
                        }
                    } else if let Some(page) = id("page:") {
                        gui.show_page(page);
                    }
                }
            }
        }
    }

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
        // A script opening one of the game UI's dialogs for the hero: the one answering to
        // that window number, else the dialog of that id.
        for event in events {
            if let Event::OpenWindow { player, dialog } = *event
                && player == hero
                && let Some(gui) = &mut self.gui
                && !gui.open_window(dialog)
            {
                gui.open(dialog);
            }
        }
        for event in events {
            if let Event::PlacementAsked { player, .. } = *event
                && player == hero
            {
                self.placing = true;
            }
        }
        for event in events {
            self.overlay.event(event, hero, now);
            match event {
                Event::Warning(text) => self.log.push(LogLine::Error(text.clone())),
                Event::ScriptLog(text) => self.log.push(LogLine::Info(format!("Script: {text}"))),
                // A game UI shows its own bag beside a shop (its dialogs' `when`).
                Event::Trade {
                    player,
                    partner,
                    news,
                } if *player == hero => {
                    if let c3_game::TradeNews::Asked = news {
                        let name = sim.player(*partner).map_or("Someone", |p| &p.name);
                        self.system(format!("{name} asks to trade with you."), style::GOLD);
                    }
                    self.gui_state.trade_news(*partner, news);
                }
                Event::StorageOpened { player, npc } if *player == hero => {
                    self.gui_state.storage = Some(*npc);
                }
                Event::ShopOpened { player, .. } if *player == hero && self.gui.is_none() => {
                    self.bag_open = true
                }
                _ => self.chat.event(event, sim, hero),
            }
        }
    }

    /// Draws the UI over `view.rect` and returns what the player asked for.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        view: &GameView,
        icons: &mut dyn gui::Textures,
    ) -> Vec<UiAction> {
        let mut actions = Vec::new();
        fonts::use_game_font(ui);
        self.overlay.show(ui, view);
        let Some(player) = view.player() else {
            return actions;
        };
        // A placement asked for: where the hero stands, as it faces.
        if std::mem::take(&mut self.placing) {
            actions.push(UiAction::Game(c3_game::Input::Place {
                cell: player.cell,
                direction: player.direction,
            }));
        }
        let typing = self.chat.typing;
        let free_keys = !typing && !ui.ctx().egui_wants_keyboard_input();
        hud::hint_line(ui, view);
        hud::status_row(ui, view, player);
        hud::team_list(ui, view, player);
        radar::show(ui, view);
        quests::npc_marks(&ui.painter_at(view.rect), view);
        self.in_team = player.team.is_some();
        self.leads_team = player.team == Some(player.id);
        match &self.gui {
            Some(gui) => {
                let data = gui_data::HeroData {
                    view,
                    player,
                    state: &self.gui_state,
                };
                let mut others = Vec::new();
                let working = gui.open_uses("work.");
                let mut work = Vec::new();
                for event in gui.show(ui.ctx(), view.rect, &data, icons) {
                    match event {
                        // A bag item onto an open workbench; a slot clicked empties it.
                        gui::GuiEvent::Item { list, index } if working && list == "hero.bag" => {
                            work.extend(player.bag.get(index).map(|i| (true, i.id)));
                        }
                        gui::GuiEvent::Item { list, .. } if list.starts_with("work.") => {
                            let n = list.strip_prefix("work.").and_then(|n| n.parse().ok());
                            work.extend(
                                n.and_then(|n: usize| data.state.work.get(n))
                                    .map(|id| (false, *id)),
                            );
                        }
                        gui::GuiEvent::Item { list, index } => {
                            actions.extend(grid_click(&data, player, &list, index));
                        }
                        other => others.push(other),
                    }
                }
                let bench = &mut self.gui_state.work;
                for (put, id) in work {
                    if !put {
                        bench.retain(|w| *w != id);
                    } else if !bench.contains(&id) && bench.len() < gui_data::WORK_SLOTS {
                        bench.push(id);
                    }
                }
                for event in others {
                    self.gui_event(event, &mut actions);
                }
                // What the hero offers shows in its side of the trade.
                if let Some(trade) = &mut self.gui_state.trade {
                    for action in &actions {
                        if let UiAction::Game(c3_game::Input::TradeItem(item)) = action
                            && !trade.mine.contains(item)
                        {
                            trade.mine.push(*item);
                        }
                    }
                }
            }
            None => {
                let mut open = hud::Open {
                    bag: &mut self.bag_open,
                    quests: &mut self.quests_open,
                };
                hud::bar(ui, view, player, &mut open, &mut actions);
            }
        }
        hud::skill_bar(ui, view, player, &mut actions);
        let lift = if player.skills.is_empty() {
            0.0
        } else {
            hud::SKILL_ROW
        };
        self.chat.show(ui, view.rect, lift, &mut actions);
        let shop = player.shop;
        let gui_shop = self
            .gui
            .as_ref()
            .is_some_and(|g| g.shows_while("hero.shop"));
        if let Some(npc) = shop.filter(|_| !gui_shop) {
            shop::window(ui, view, npc, icons, &mut actions);
        } else if self.quests_open {
            quests::window(ui, view, &mut self.quests_open);
        } else {
            quests::tracker(ui, view);
        }
        if self.bag_open {
            bag::window(ui, view, shop, icons, &mut self.bag_open, &mut actions);
        }
        let gui_talk = self
            .gui
            .as_ref()
            .is_some_and(|g| g.shows_while("hero.talking"));
        if let Some(conversation) = player.dialogue.as_ref().filter(|_| !gui_talk) {
            dialogue::window(ui, view.rect, conversation, free_keys, &mut actions);
        }
        menu::window(ui, view.rect, &mut self.menu, view.controls, &mut actions);
        actions
    }
}
