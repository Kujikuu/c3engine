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
pub use chat::ChatLook;
mod chat_bar;
mod dialogue;
pub mod fonts;
pub mod front;
pub mod gui;
mod gui_data;
mod hud;
pub mod icons;
mod menu;
mod prompt;
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
        // Of the answers picked by a click, the `index`th.
        ("dialogue.choices", _) => {
            let page = &player.dialogue.as_ref()?.page;
            let options = page
                .options
                .iter()
                .enumerate()
                .filter(|(_, o)| o.input.is_none());
            Some(UiAction::Answer(options.map(|(i, _)| i).nth(index)?))
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

/// What putting the item held (slot `from.1` of list `from.0`) on list `to` asks the game,
/// as TQ's grids take an item dropped on them: sold to the shop, stored in or taken from the
/// warehouse, worn, taken off, bought, offered in the trade.
fn move_item(
    data: &gui_data::HeroData,
    player: &Player,
    (from, at): (&str, usize),
    to: &str,
) -> Option<UiAction> {
    use c3_game::Input;
    let item = || Some(data.own_items(from)?.get(at)?.id);
    Some(match (from, to) {
        ("hero.bag", "shop.items") => UiAction::Sell {
            npc: player.shop?,
            item: item()?,
        },
        ("hero.bag", "storage.items") => UiAction::Game(Input::Store {
            npc: data.state.storage?,
            item: item()?,
        }),
        ("storage.items", "hero.bag") => UiAction::Game(Input::Retrieve {
            npc: data.state.storage?,
            item: item()?,
        }),
        ("hero.bag", "trade.mine") => UiAction::Game(Input::TradeItem(item()?)),
        ("shop.items", "hero.bag") => UiAction::Buy {
            npc: player.shop?,
            kind: *gui::GuiData::items(data, from).get(at)?,
        },
        ("hero.bag", worn) if worn.starts_with("hero.worn.") => UiAction::Use(item()?),
        (worn, "hero.bag") if worn.starts_with("hero.worn.") => UiAction::Unequip(item()?),
        _ => return None,
    })
}

/// The item held drawn at the pointer.
fn draw_held(
    ui: &egui::Ui,
    data: &gui_data::HeroData,
    list: &str,
    index: usize,
    icons: &mut dyn gui::Textures,
) {
    use gui::GuiData;
    let (Some(&kind), Some(at)) = (data.items(list).get(index), ui.ctx().pointer_hover_pos())
    else {
        return;
    };
    let look = data.item_looks(list).get(index).copied().unwrap_or(0);
    let Some(texture) = icons.item(ui.ctx(), kind, look) else {
        return;
    };
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::unique("held item"),
    ));
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    painter.image(
        texture,
        Rect::from_center_size(at, egui::vec2(40.0, 40.0)),
        uv,
        egui::Color32::WHITE,
    );
}

/// A PK mode's name in bindings and actions.
pub(crate) fn pk_mode_name(mode: c3_core::settings::PkMode) -> &'static str {
    use c3_core::settings::PkMode as M;
    match mode {
        M::Free => "free",
        M::Peace => "peace",
        M::Team => "team",
        M::Arrest => "arrest",
    }
}

/// The typed answer of the NPC's page, when something is typed (CDlgTask sends none empty).
fn typed_answer(state: &gui_data::GuiState, player: &Player) -> Option<UiAction> {
    let page = &player.dialogue.as_ref()?.page;
    let index = page.options.iter().position(|o| o.input.is_some())?;
    let text = state.fields.get("dialogue.typed")?.trim().to_owned();
    (!text.is_empty()).then_some(UiAction::Game(c3_game::Input::AnswerText { index, text }))
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

/// The game UI's chat line and whisper name fields.
const CHAT_TEXT: &str = "chat.text";
pub(crate) const CHAT_NAME: &str = "chat.name";
/// The announcement a guild's leader types.
const GUILD_NOTICE: &str = "guild.notice";
/// The amount typed in the yes/no prompt (dropping money).
const PROMPT_AMOUNT: &str = "prompt.amount";
/// The words typed for a friend in the leave-word dialog.
const WORD_TEXT: &str = "word.text";
/// A message board post as typed.
const BOARD_TEXT: &str = "board.text";
/// What the hero's booth cries out, as typed.
const BOOTH_CRY: &str = "booth.cry";
/// The chat bar's list of names (TQ's CDlgListBox, hidden once a name is picked).
const CHAT_TARGETS: &str = "chat.targets";

/// A line for the host's console (script output, data problems).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogLine {
    Info(String),
    Error(String),
}

/// Item pictures, loaded by the host.
pub trait Icons {
    /// The icon of item type `kind` in look `look` (0: its usual), if the game has one.
    fn item(&mut self, ctx: &egui::Context, kind: u32, look: u32) -> Option<TextureId>;
    /// The frame of item type `kind`'s glow showing at `time` seconds, if it has one.
    fn item_glow(&mut self, _ctx: &egui::Context, _kind: u32, _time: f64) -> Option<TextureId> {
        None
    }
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
    /// The picture's size in pixels, and the world it covers.
    pub size: egui::Vec2,
    pub bounds: [f32; 4],
    /// Shown as a box this size (layout pixels) in the top-right corner; none: the circle.
    pub boxed: Option<egui::Vec2>,
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
    /// Inputs the UI makes by itself (auto-joining a team), sent next frame.
    auto_inputs: Vec<c3_game::Input>,
    /// The player's volumes as last shown, and the music's level before it was switched off.
    prefs: c3_core::settings::PlayerPrefs,
    music_before: f32,
    /// The hero's team's options as last drawn (closed, keeping money, sharing items).
    team_options: (bool, bool, bool),
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
            "guild_apply" => Input::Guild(G::Apply(player)),
            "friend_add" => Input::Friend(c3_game::FriendAction::Ask(player)),
            "partner_add" => Input::Partner(c3_game::PartnerAction::Ask(player)),
            "mentor_teach" => Input::Mentor(c3_game::MentorAction::Teach(player)),
            "mentor_learn" => Input::Mentor(c3_game::MentorAction::Learn(player)),
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
    /// Runs a game UI action as a click would (`;` between several); what it asks the game.
    pub fn run_gui_action(&mut self, action: &str) -> Vec<UiAction> {
        let mut actions = Vec::new();
        for part in action.split(';') {
            self.run_action(part, &mut actions);
        }
        actions
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
            gui::GuiEvent::Item { .. } | gui::GuiEvent::Pick { .. } | gui::GuiEvent::Put { .. } => {
                return;
            }
            // The player's volumes moved on the game UI's sliders.
            gui::GuiEvent::Slid { field, value } => {
                let mut prefs = self.prefs;
                let level = (value as f32 / 100.0).clamp(0.0, 1.0);
                match field.as_str() {
                    "prefs.music" => prefs.music = level,
                    "prefs.sound" => prefs.sound = level,
                    _ => return,
                }
                self.prefs = prefs;
                actions.push(UiAction::SetPrefs(prefs));
                return;
            }
            gui::GuiEvent::EditKey { field, key } => {
                if field == CHAT_TEXT {
                    self.chat_key(key);
                }
                return;
            }
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
            // View group `<group>`'s number moved by `<step>`, kept from 0 to below what
            // binding `<max>` reads (`view_step:shop_page:20:count.shop.items`: a shop's next
            // page); worked out with the game's data.
            _ if action.starts_with("view_step:") => {
                let mut parts = action["view_step:".len()..].splitn(3, ':');
                if let (Some(group), Some(step)) = (parts.next(), parts.next()) {
                    let step = step.parse().unwrap_or(0);
                    let max = parts.next().unwrap_or_default().to_owned();
                    self.gui_state.steps.push((group.to_owned(), step, max));
                }
            }
            // A line of the game's own told the player (`tell:ini/StrRes.ini:10341`).
            _ if action.starts_with("tell:") => {
                let (file, key) = action["tell:".len()..].rsplit_once(':').unwrap_or_default();
                let line = (self.gui.as_ref())
                    .and_then(|g| g.names.get(file)?.get(key).cloned())
                    .unwrap_or_default();
                if !line.is_empty() {
                    self.system(line, style::BAD);
                }
            }
            // An edit given the keyboard (`focus:board.text`).
            _ if action.starts_with("focus:") => {
                self.gui_state.focus = Some(action["focus:".len()..].to_owned());
            }
            // An edit's text cleared (`clear:guild.notice`).
            _ if action.starts_with("clear:") => {
                self.gui_state.fields.remove(&action["clear:".len()..]);
            }
            // The guild: money given (the prompt's amount), its announcement set, the member
            // picked sent out.
            "guild_donate" => {
                let typed = self.gui_state.fields.remove(PROMPT_AMOUNT);
                if let Some(amount) = typed.and_then(|t| t.trim().parse::<u64>().ok()) {
                    let donate = c3_game::GuildAction::Donate(amount);
                    actions.push(UiAction::Game(c3_game::Input::Guild(donate)));
                }
            }
            "guild_notice" => {
                let text = self
                    .gui_state
                    .fields
                    .remove(GUILD_NOTICE)
                    .unwrap_or_default();
                if !text.trim().is_empty() {
                    actions.push(UiAction::Game(c3_game::Input::GuildNotice(text)));
                }
            }
            "guild_kick_picked" => self.gui_state.guild_kicking = true,
            // The hotkey page: the action picked waits for its new key; every key as it was.
            "hotkey_change" => self.gui_state.rebinding = true,
            "hotkey_revert" => actions.push(UiAction::ResetControls),
            // The friend or enemy picked let go (`contact_drop:friends`, `contact_drop:enemies`).
            _ if action.starts_with("contact_drop:") => {
                self.gui_state.dropping = Some(action["contact_drop:".len()..].to_owned());
            }
            // The money typed offered: none is refused (StrRes 10045).
            "trade_money" => {
                let amount: u64 = (self.gui_state.fields.get("trade.amount"))
                    .and_then(|t| t.trim().parse().ok())
                    .unwrap_or(0);
                if amount == 0 {
                    self.system("Invalid amount.", style::BAD);
                    return;
                }
                actions.push(UiAction::Game(c3_game::Input::TradeMoney(amount)));
            }
            // Agreeing waits for money typed to be offered first (StrRes 10046).
            "trade_accept" => {
                let typed = (self.gui_state.fields.get("trade.amount"))
                    .and_then(|t| t.trim().parse::<u64>().ok())
                    .unwrap_or(0);
                let Some(own) = self.gui_state.trade.as_ref().map(|t| t.own_money) else {
                    return;
                };
                if typed > 0 && own == 0 {
                    self.system("Please confirm the amount of silver to trade.", style::BAD);
                    return;
                }
                if let Some(trade) = &mut self.gui_state.trade {
                    trade.i_agreed = true;
                }
                actions.push(UiAction::Game(c3_game::Input::TradeAccept));
            }
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
            // The team menu's checks (MenuGroup): each turns its option the other way.
            "team_close" | "team_money" | "team_items" => {
                let (closed, keeps, shares) = self.team_options;
                use c3_game::TeamAction as T;
                let action = match action {
                    "team_close" if closed => T::Open,
                    "team_close" => T::Close,
                    "team_money" if keeps => T::ShareMoney,
                    "team_money" => T::KeepMoney,
                    _ if shares => T::KeepItems,
                    _ => T::ShareItems,
                };
                actions.push(UiAction::Game(c3_game::Input::Team(action)));
            }
            "team_auto" => self.gui_state.auto_team ^= true,
            // The minimap shown or rolled up (the main bar's map check).
            "radar" => self.gui_state.radar_hidden ^= true,
            // The chat bar: its line said on the picked channel (`view:chat:<channel>`;
            // talk when none), a whisper to the name typed; the shown lines cleared.
            // Lines are a half second apart at least (StrRes 10044); each is kept for Up/Down.
            "chat_send" => {
                let text = self.gui_state.fields.remove(CHAT_TEXT).unwrap_or_default();
                let text = text.replace(['\r', '\n'], " ");
                if text.trim().is_empty() {
                    return;
                }
                let to = self
                    .gui_state
                    .fields
                    .get(CHAT_NAME)
                    .map(|n| n.trim().to_owned())
                    .unwrap_or_default();
                if text.starts_with('/') {
                    actions.push(UiAction::Game(c3_game::Input::Say(text)));
                    return;
                }
                if self.gui_state.chat_bar.too_soon(std::time::Instant::now()) {
                    self.system(
                        "You have sent too many messages. Please wait a moment to send again.",
                        style::BAD,
                    );
                    return;
                }
                let picked = self.chat_channel().to_owned();
                use c3_game::ChatChannel as C;
                let channel = match picked.as_str() {
                    "talk" => C::Local,
                    "whisper" => C::Whisper,
                    "team" => C::Team,
                    "guild" => C::Guild,
                    "friend" => C::Friend,
                    // A broadcast: checked for the hero's level, then asked (CDlgPigeonPrompt).
                    "broadcast" => {
                        self.gui_state.broadcast = Some(text);
                        return;
                    }
                    _ => {
                        self.system("That channel isn't open yet.", style::BAD);
                        return;
                    }
                };
                self.gui_state.chat_bar.said(&text, &picked, &to);
                if channel == C::Whisper && self.gui.is_some() {
                    self.gui_state.whisper = Some((to, text));
                    return;
                }
                actions.push(UiAction::Game(c3_game::Input::Chat { channel, to, text }));
            }
            // The name typed listed, or taken off when it is (the Add button).
            "chat_list" => {
                let name = (self.gui_state.fields.get(CHAT_NAME)).cloned();
                (self.gui_state.chat_bar).toggle_listed(name.unwrap_or_default().trim());
            }
            "chat_clear" => self.chat.clear(),
            // The question shown answered: Yes does what it asks, No drops it.
            "prompt_yes" => match self.gui_state.prompt.take().map(|p| p.yes) {
                Some(prompt::Yes::Game(input)) => actions.push(UiAction::Game(input)),
                Some(prompt::Yes::Actions(then)) => {
                    for part in then.split(';') {
                        self.run_action(part, actions);
                    }
                }
                None => {}
            },
            "prompt_no" => {
                self.gui_state.prompt = None;
                self.gui_state.booth_item = None;
                (self.gui_state.fields).retain(|k, _| !k.starts_with(PROMPT_AMOUNT));
            }
            // Booths: the bag item put on the hero's booth set out at the price typed (in the
            // prompt's money, else its other currency `prompt.amount.<currency>`), the booth's
            // cry, the hero's booth closed, another's left, a slot's ware taken back or bought.
            "booth_add" => {
                let typed = |k: &str| {
                    (self.gui_state.fields.get(k)).and_then(|t| t.trim().parse::<u64>().ok())
                };
                let other = (self.gui_state.fields.keys())
                    .filter_map(|k| k.strip_prefix(PROMPT_AMOUNT)?.strip_prefix('.'))
                    .map(str::to_owned)
                    .find(|c| typed(&format!("{PROMPT_AMOUNT}.{c}")).is_some_and(|v| v > 0));
                let (price, currency) = match other {
                    Some(c) => (typed(&format!("{PROMPT_AMOUNT}.{c}")).unwrap_or(0), c),
                    None => (typed(PROMPT_AMOUNT).unwrap_or(0), String::new()),
                };
                (self.gui_state.fields).retain(|k, _| !k.starts_with(PROMPT_AMOUNT));
                if let Some(item) = self.gui_state.booth_item.take()
                    && price > 0
                {
                    let add = c3_game::BoothAction::Add {
                        item,
                        price,
                        currency,
                    };
                    actions.push(UiAction::Game(c3_game::Input::Booth(add)));
                }
            }
            "booth_cry" => {
                let text = self.gui_state.fields.remove(BOOTH_CRY).unwrap_or_default();
                if !text.trim().is_empty() {
                    let cry = c3_game::BoothAction::Cry(text);
                    actions.push(UiAction::Game(c3_game::Input::Booth(cry)));
                }
            }
            "booth_close" => {
                let close = c3_game::Input::Booth(c3_game::BoothAction::Close);
                actions.push(UiAction::Game(close));
            }
            "booth_leave" => self.gui_state.booth = None,
            // Leave words: sent to the dialog's friend; the sender of those shown answered;
            // those received forgotten; a contact row double-clicked (`contact_word:<list>:
            // <dialog>`: whisper it online, leave it words in that dialog offline).
            "word_send" => {
                let text = self.gui_state.fields.remove(WORD_TEXT).unwrap_or_default();
                let to = self.gui_state.word_to.clone();
                if !text.trim().is_empty() && !to.is_empty() {
                    let channel = c3_game::ChatChannel::LeaveWord;
                    actions.push(UiAction::Game(c3_game::Input::Chat { channel, to, text }));
                }
            }
            "word_reply" => {
                let at = (self.gui_state.views.get("word")).and_then(|v| v.parse().ok());
                if let Some((from, _)) = at.and_then(|at: usize| self.gui_state.words.get(at)) {
                    self.gui_state.word_to = from.clone();
                }
            }
            // The message board: a post written (its lines made one, as CDlgLevWord::OnBtnOk
            // does), the picked one read (else StrRes 10363), taken down, its author whispered.
            "board_post" => {
                let text = self.gui_state.fields.remove(BOARD_TEXT).unwrap_or_default();
                let text = text.replace(['\r', '\n'], " ");
                let board = (self.gui_state.views.get("board")).and_then(|v| v.parse().ok());
                if !text.trim().is_empty() {
                    let channel = c3_game::ChatChannel::Board(board.unwrap_or(0));
                    let to = String::new();
                    actions.push(UiAction::Game(c3_game::Input::Chat { channel, to, text }));
                    self.gui_state.views.insert("board_mode".into(), "0".into());
                }
            }
            "board_watch" => {
                let picked =
                    (self.gui_state.views.get("board_pick")).and_then(|v| v.parse::<usize>().ok());
                match picked {
                    Some(_) => {
                        self.gui_state.views.insert("board_mode".into(), "2".into());
                    }
                    None => self.run_action("tell:ini/StrRes.ini:10363", actions),
                }
            }
            "board_del" | "board_whisper" => self.gui_state.board_pending = Some(action.to_owned()),
            // Mentors: leaving the hero's mentor; claiming what students gave
            // (`mentor_claim:exp|bless|item`).
            "mentor_leave" => {
                let leave = c3_game::Input::Mentor(c3_game::MentorAction::Leave);
                actions.push(UiAction::Game(leave));
            }
            _ if action.starts_with("mentor_claim:") => {
                use c3_game::MentorGift as G;
                let gift = match &action["mentor_claim:".len()..] {
                    "exp" => G::Exp,
                    "bless" => G::Bless,
                    _ => G::Item,
                };
                let claim = c3_game::Input::Mentor(c3_game::MentorAction::Claim(gift));
                actions.push(UiAction::Game(claim));
            }
            // The broadcast board: a waiting line moved to the front or up 5 places (asked
            // first, StrRes 11033 / 11032).
            // Another's line's name: the chat bar whispers its author (`pigeon_whisper:<row>`).
            _ if action.starts_with("pigeon_") && action.contains(':') => {
                let (kind, row) = action.split_once(':').expect("checked");
                if let Ok(row) = row.parse() {
                    self.gui_state.pigeon_move = Some((kind.to_owned(), row));
                }
            }
            // Nobility: the amount typed given (in the other currency when the money falls short,
            // asked first, StrRes 11090; less than the least: 11098).
            "nobility_donate" => self.gui_state.donating = true,
            "words_clear" => {
                self.gui_state.words.clear();
                self.gui_state.views.remove("word");
            }
            _ if action.starts_with("contact_word:") => {
                let mut parts = action["contact_word:".len()..].splitn(2, ':');
                let list = parts.next().unwrap_or_default().to_owned();
                let dialog = parts.next().and_then(|d| d.parse().ok()).unwrap_or(0);
                self.gui_state.contact_word = Some((list, dialog));
            }
            _ if action.starts_with("booth_remove:") || action.starts_with("booth_buy:") => {
                let (kind, n) = action.split_once(':').expect("checked");
                let whose = if kind == "booth_buy" {
                    "theirs"
                } else {
                    "mine"
                };
                if let Ok(n) = n.parse() {
                    self.gui_state.booth_slot = Some((whose.to_owned(), n));
                }
            }
            // The money typed in the prompt dropped (its drop-money question).
            "drop_money" => {
                let typed = self.gui_state.fields.remove(PROMPT_AMOUNT);
                if let Some(amount) = typed.and_then(|t| t.trim().parse::<u64>().ok()) {
                    actions.push(UiAction::Game(c3_game::Input::DropMoney(amount)));
                }
            }
            // A dead hero comes back to life where the map sends it.
            "revive" => actions.push(UiAction::Game(c3_game::Input::Revive)),
            // The hero shows emote `<n>` (`emote:250`: TQ's sitting down).
            _ if action.starts_with("emote:") => {
                if let Ok(emote) = action["emote:".len()..].parse() {
                    actions.push(UiAction::Game(c3_game::Input::Emote(emote)));
                }
            }
            // The hero's PK mode set (`pk_mode:free|peace|team|arrest`).
            _ if action.starts_with("pk_mode:") => {
                use c3_core::settings::PkMode as M;
                let mode = match &action["pk_mode:".len()..] {
                    "peace" => M::Peace,
                    "team" => M::Team,
                    "arrest" => M::Arrest,
                    _ => M::Free,
                };
                actions.push(UiAction::Game(c3_game::Input::SetPkMode(mode)));
            }
            // The member picked in the team dialog sent out.
            "team_kick_picked" => self.gui_state.kicking = true,
            // The typed answer sent (its button, Enter in its box).
            "answer_typed" => self.gui_state.answering = true,
            // The skill a binding names picked for the right button (`pick_skill:skill.3`), or
            // used: cast at once when aimed at its caster, else picked (`use_skill:`).
            _ if action.starts_with("pick_skill:") || action.starts_with("use_skill:") => {
                let (how, binding) = action.split_once(':').unwrap_or_default();
                self.gui_state.picking = Some((binding.to_owned(), how == "use_skill"));
            }
            // An action asked first (`ask:<kind>:<actions>`; leaving a guild).
            _ if action.starts_with("ask:") => {
                self.gui_state.prompt = prompt::Prompt::confirm(&action["ask:".len()..]);
            }
            "trade_close" => actions.push(UiAction::Game(c3_game::Input::TradeClose)),
            // The workbench emptied (a workbench closed).
            "work_clear" => self.gui_state.work.clear(),
            // Workbench slot `a`'s item used on slot `b`'s (`use_on:1:0`: a DragonBall on
            // the gear, TQ's improve).
            _ if action.starts_with("use_on:") => {
                let mut slots = action["use_on:".len()..].split(':');
                let mut at = || {
                    slots
                        .next()?
                        .parse::<usize>()
                        .ok()
                        .and_then(|n| self.gui_state.work.get(n).copied())
                };
                if let (Some(item), Some(target)) = (at(), at()) {
                    actions.push(UiAction::Game(c3_game::Input::UseOn { item, target }));
                }
            }
            "bag" => self.toggle_bag(),
            "quests" => self.toggle_quests(),
            "menu" => self.toggle_menu(),
            "chat" => self.open_chat(),
            "sound" => actions.push(UiAction::ToggleSound),
            "close_shop" => actions.push(UiAction::CloseShop),
            "close_dialogue" => actions.push(UiAction::CloseDialogue),
            "run" => actions.push(UiAction::ToggleRun),
            // The window full screen or not (`fullscreen:on|off`; the setup's mode buttons).
            _ if action.starts_with("fullscreen:") => {
                let mut prefs = self.prefs;
                prefs.fullscreen = action.ends_with(":on");
                self.prefs = prefs;
                actions.push(UiAction::SetPrefs(prefs));
            }
            // Leaving the game (the setup's Exit).
            "leave" => actions.push(UiAction::Leave),
            // The music off, or back at the level it had (the setup's music switch).
            "music_switch" => {
                let mut prefs = self.prefs;
                if prefs.music > 0.0 {
                    self.music_before = prefs.music;
                    prefs.music = 0.0;
                } else {
                    prefs.music = if self.music_before > 0.0 {
                        self.music_before
                    } else {
                        1.0
                    };
                }
                self.prefs = prefs;
                actions.push(UiAction::SetPrefs(prefs));
            }
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
                    } else if let Some(dialog) = id("close:") {
                        gui.close(dialog);
                    } else if let Some(dialog) = id("open:") {
                        gui.open(dialog);
                    }
                }
            }
        }
    }

    /// A contact row double-clicked (TQ's ON_FRIEND_CHAT / ON_FRIEND_LEAVE): an online friend
    /// is whispered, an offline one gets the leave-word dialog; and a whisper typed: sent, or
    /// to an offline friend asked to be left as words (StrRes 11022, CDlgOfflineMessage).
    fn contact_word(&mut self, view: &GameView, player: &Player, actions: &mut Vec<UiAction>) {
        let data = gui_data::HeroData {
            view,
            player,
            state: &self.gui_state,
        };
        let board_author = (self.gui_state.board_pending.as_ref())
            .and_then(|_| Some(data.board_picked()?.author.clone()));
        let board = (self.gui_state.views.get("board")).and_then(|v| v.parse().ok());
        match (self.gui_state.board_pending.take().as_deref(), board_author) {
            (Some("board_del"), Some(author)) => {
                let board = board.unwrap_or(0);
                actions.push(UiAction::Game(c3_game::Input::TakeDown { board, author }));
            }
            (Some("board_whisper"), Some(author)) => {
                self.gui_state.fields.insert(CHAT_NAME.into(), author);
                self.gui_state.views.insert("chat".into(), "whisper".into());
                self.gui_state.focus = Some(CHAT_TEXT.into());
            }
            _ => {}
        }
        let pigeon_move = self.gui_state.pigeon_move.take();
        let data = gui_data::HeroData {
            view,
            player,
            state: &self.gui_state,
        };
        let moving = pigeon_move
            .and_then(|(kind, row)| Some((kind, data.pigeon_rows().into_iter().nth(row)?)));
        if let Some((kind, (place, name, _, _, id))) = moving {
            use c3_game::BroadcastAction as B;
            match kind.as_str() {
                "pigeon_first" => {
                    self.ask(&kind, "1", c3_game::Input::Broadcast(B::First(id)));
                }
                "pigeon_bump" => {
                    let to = place.saturating_sub(5).max(1).to_string();
                    self.ask(&kind, &to, c3_game::Input::Broadcast(B::Bump(id)));
                }
                _ => {
                    self.gui_state.fields.insert(CHAT_NAME.into(), name);
                    self.gui_state.views.insert("chat".into(), "whisper".into());
                    self.gui_state.focus = Some(CHAT_TEXT.into());
                }
            }
        }
        if std::mem::take(&mut self.gui_state.donating) {
            let typed = self.gui_state.fields.remove("nobility.amount");
            let amount: u64 = typed.and_then(|t| t.trim().parse().ok()).unwrap_or(0);
            let n = &view.sim.settings().nobility;
            let rate = n.rate.max(1);
            let in_currency = player.wallet.get(&n.currency).copied().unwrap_or(0);
            let give = c3_game::Input::Donate(amount);
            if amount < n.min_donation {
                self.system("You need to donate at least 3,000,000 silver.", style::BAD);
            } else if amount <= player.money {
                actions.push(UiAction::Game(give));
            } else if amount / rate <= in_currency {
                let ask = format!("{amount}||{}", amount / rate);
                self.ask("nobility_cps", &ask, give);
            } else {
                self.system("Sorry, you don`t have enough silver.", style::BAD);
            }
        }
        if let Some(text) = self.gui_state.broadcast.take() {
            let low = player.level < view.sim.settings().broadcast.min_level;
            let line = c3_game::Input::Chat {
                channel: c3_game::ChatChannel::Broadcast,
                to: String::new(),
                text: text.clone(),
            };
            let kind = if low { "broadcast_low" } else { "broadcast" };
            self.ask(kind, &text, line);
        }
        let data = gui_data::HeroData {
            view,
            player,
            state: &self.gui_state,
        };
        let double = (self.gui_state.contact_word.as_ref())
            .and_then(|(list, dialog)| Some((data.picked_contact(list)?, *dialog)))
            .map(|(name, dialog)| (data.offline_friend(&name), name, dialog));
        let whisper = (self.gui_state.whisper.as_ref())
            .map(|(to, text)| (data.offline_friend(to), to.clone(), text.clone()));
        self.gui_state.contact_word = None;
        self.gui_state.whisper = None;
        match double {
            Some((true, name, dialog)) => {
                self.gui_state.word_to = name;
                if let Some(gui) = &mut self.gui {
                    gui.open(dialog);
                }
            }
            Some((false, name, _)) => {
                self.gui_state.fields.insert(CHAT_NAME.into(), name);
                self.gui_state.views.insert("chat".into(), "whisper".into());
                self.gui_state.focus = Some(CHAT_TEXT.into());
            }
            None => {}
        }
        use c3_game::{ChatChannel as C, Input};
        match whisper {
            Some((true, to, text)) => {
                let word = Input::Chat {
                    channel: C::LeaveWord,
                    to: to.clone(),
                    text,
                };
                self.ask("offline_word", &to, word);
            }
            Some((false, to, text)) => actions.push(UiAction::Game(Input::Chat {
                channel: C::Whisper,
                to,
                text,
            })),
            None => {}
        }
    }

    /// The skill picked for the right button.
    pub fn picked_skill(&self) -> Option<u32> {
        self.gui_state.picked_skill
    }

    /// Answers Yes to the question shown (Enter): what it asks the game; none when no
    /// question shows.
    pub fn answer_prompt(&mut self) -> Option<Vec<UiAction>> {
        self.gui_state.prompt.as_ref()?;
        Some(self.run_gui_action("prompt_yes"))
    }

    /// Another player's ask of the hero (`event`) put to it as a question.
    fn ask_event(&mut self, event: &Event, sim: &Sim, hero: EntityId) {
        use c3_game::{GuildAction as G, Input, TeamAction as T};
        let (kind, from, input) = match *event {
            Event::TeamInvited { player, leader } if player == hero => {
                ("team_invite", leader, Input::Team(T::Join(leader)))
            }
            Event::TeamApplied { player, applicant } if player == hero => {
                ("team_apply", applicant, Input::Team(T::Accept(applicant)))
            }
            Event::GuildInvited { player, inviter } if player == hero => {
                ("guild_invite", inviter, Input::Guild(G::Join(inviter)))
            }
            Event::GuildApplied { player, applicant } if player == hero => {
                ("guild_apply", applicant, Input::Guild(G::Invite(applicant)))
            }
            Event::Proposed { player, from } if player == hero => {
                ("marriage", from, Input::AcceptProposal(from))
            }
            Event::MentorAsked {
                player,
                from,
                teach,
            } if player == hero => {
                use c3_game::MentorAction as M;
                match teach {
                    true => ("mentor_offer", from, Input::Mentor(M::Learn(from))),
                    false => ("mentor_ask", from, Input::Mentor(M::Teach(from))),
                }
            }
            Event::PartnerAsked { player, from } if player == hero => (
                "partner",
                from,
                Input::Partner(c3_game::PartnerAction::Ask(from)),
            ),
            Event::FriendAsked { player, from } if player == hero => (
                "friend",
                from,
                Input::Friend(c3_game::FriendAction::Accept(from)),
            ),
            _ => return,
        };
        let name = sim.player(from).map_or("", |p| p.name.as_str()).to_owned();
        self.ask(kind, &name, input);
    }

    /// Puts `name`'s ask of `kind` to the hero when the game UI has a prompt; whether it
    /// did. A trade asked while another is asked is turned away (StrRes 10433).
    fn ask(&mut self, kind: &str, name: &str, input: c3_game::Input) -> bool {
        if !self
            .gui
            .as_ref()
            .is_some_and(|g| g.shows_while("prompt.open"))
        {
            return false;
        }
        if kind == "trade"
            && self
                .gui_state
                .prompt
                .as_ref()
                .is_some_and(|p| p.kind == "trade")
        {
            self.system(
                "You received another trade request! Close this one first.",
                style::BAD,
            );
            return true;
        }
        let now = std::time::Instant::now();
        self.gui_state.prompt = Some(prompt::Prompt::asked(kind, name, input, now));
        true
    }

    /// The chat bar's channel picked (`view:chat:<channel>`; talk at first).
    fn chat_channel(&self) -> &str {
        self.gui_state
            .views
            .get("chat")
            .map_or("talk", String::as_str)
    }

    /// A key in the chat line: Down / Up step through the lines said, Ctrl+Up / Down
    /// cycle the channel, Ctrl+Left / Right the recent channel-and-name pairs.
    fn chat_key(&mut self, key: gui::EditKey) {
        use gui::EditKey as K;
        let state = &mut self.gui_state;
        let current = state.fields.get(CHAT_TEXT).cloned().unwrap_or_default();
        let line = match key {
            K::Down => state.chat_bar.older(&current),
            K::Up => state.chat_bar.newer(),
            K::CtrlUp | K::CtrlDown => {
                let now = state.views.get("chat").map_or("talk", String::as_str);
                let next = chat_bar::next_channel(now, key == K::CtrlUp);
                state.views.insert("chat".into(), next.into());
                None
            }
            K::CtrlLeft | K::CtrlRight => {
                if let Some((channel, name)) = state.chat_bar.cycle_recent(key == K::CtrlLeft) {
                    state.views.insert("chat".into(), channel);
                    state.fields.insert(CHAT_NAME.into(), name);
                }
                None
            }
        };
        if let Some(line) = line {
            state.fields.insert(CHAT_TEXT.into(), line);
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

    /// Starts typing a chat line: in the game UI's chat bar when it has one.
    pub fn open_chat(&mut self) {
        match &self.gui {
            Some(gui) if gui.edits(CHAT_TEXT) => self.gui_state.focus = Some(CHAT_TEXT.into()),
            _ => self.chat.open(),
        }
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
        // Esc lets an item held go.
        if self.gui_state.held.take().is_some() {
            return (true, None);
        }
        // Esc answers No to a question shown.
        if self.gui_state.prompt.take().is_some() {
            return (true, None);
        }
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
        if self.gui_state.storage.take().is_some() {
            return (true, None);
        }
        if self.gui.as_mut().is_some_and(gui::Gui::escape) {
            return (true, None);
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

    /// Shows the chat log in a game's look (TQ's CGameMsg), or the engine's.
    pub fn set_chat_look(&mut self, look: Option<ChatLook>) {
        self.chat.set_look(look);
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
            // Asks of the hero, put to it when the game UI has a prompt (and not auto-taken).
            if !self.gui_state.auto_team {
                self.ask_event(event, sim, hero);
            }
            // Auto-joining: an invite joins, a leader takes in whoever asks.
            if self.gui_state.auto_team {
                use c3_game::{Input, TeamAction};
                match *event {
                    Event::TeamInvited { player, leader } if player == hero => {
                        self.auto_inputs.push(Input::Team(TeamAction::Join(leader)));
                    }
                    Event::TeamApplied { player, applicant } if player == hero => {
                        self.auto_inputs
                            .push(Input::Team(TeamAction::Accept(applicant)));
                    }
                    _ => {}
                }
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
                        if !self.ask("trade", name, c3_game::Input::TradeAsk(*partner)) {
                            self.system(format!("{name} asks to trade with you."), style::GOLD);
                        }
                    }
                    self.gui_state.trade_news(*partner, news);
                }
                Event::StorageOpened { player, npc } if *player == hero => {
                    self.gui_state.storage = Some(*npc);
                }
                // A booth shown from its first page: another's (CDlgNpcBooth), or the hero's.
                Event::BoothShown { player, owner } if *player == hero => {
                    let whose = if *owner == hero { "mine" } else { "theirs" };
                    self.gui_state.views.remove(&format!("booth_{whose}_page"));
                    if *owner != hero {
                        self.gui_state.booth = Some(*owner);
                    }
                }
                // A broadcast heard: the banner's line and the board's history.
                Event::Chat {
                    name,
                    text,
                    channel: c3_game::ChatChannel::Broadcast,
                    to: Some(to),
                    ..
                } if *to == hero => {
                    self.gui_state
                        .broadcasts_heard
                        .push((name.clone(), text.clone()));
                    if self.gui.is_none() {
                        self.chat.event(event, sim, hero);
                    }
                }
                // Words a friend left for the hero, kept for the received-words dialog.
                Event::Chat {
                    name,
                    text,
                    channel: c3_game::ChatChannel::LeaveWord,
                    to: Some(to),
                    ..
                } if *to == hero => {
                    self.gui_state.words.push((name.clone(), text.clone()));
                    self.chat.event(event, sim, hero);
                }
                // A free booth spot: whether to open a booth there (TQ's StrRes 10364).
                Event::BoothOffered { player, spot } if *player == hero => {
                    let open = c3_game::Input::Booth(c3_game::BoothAction::Open(*spot));
                    self.ask("booth_open", "", open);
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
        self.prefs = view.controls.prefs;
        self.overlay.show(ui, view);
        let Some(player) = view.player() else {
            return actions;
        };
        actions.extend(self.auto_inputs.drain(..).map(UiAction::Game));
        self.team_options = (player.team)
            .map(|leader| view.sim.team_options(leader))
            .unwrap_or_default();
        // A placement asked for: where the hero stands, as it faces.
        if std::mem::take(&mut self.placing) {
            actions.push(UiAction::Game(c3_game::Input::Place {
                cell: player.cell,
                direction: player.direction,
            }));
        }
        // An ask not answered in time goes.
        if (self.gui_state.prompt.as_ref()).is_some_and(|p| p.lapsed(std::time::Instant::now())) {
            self.gui_state.prompt = None;
        }
        let typing = self.chat.typing;
        let free_keys = !typing && !ui.ctx().egui_wants_keyboard_input();
        // The engine's hint line and team list, unless the game draws its own UI.
        if self.gui.is_none() {
            hud::hint_line(ui, view);
            hud::team_list(ui, view, player);
        }
        hud::status_row(ui, view, player);
        if !self.gui_state.radar_hidden {
            // Layout pixels to the screen's, as the game UI is drawn.
            let scale = self.gui.as_ref().map_or(1.0, |g| {
                let [w, h] = g.layout.base().map(|v| v as f32);
                (view.rect.width() / w).min(view.rect.height() / h)
            });
            radar::show(ui, view, scale);
        }
        quests::npc_marks(&ui.painter_at(view.rect), view);
        self.in_team = player.team.is_some();
        self.leads_team = player.team == Some(player.id);
        if let Some(gui) = &mut self.gui {
            gui.update_shown(&gui_data::HeroData {
                view,
                player,
                state: &self.gui_state,
            });
        }
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
                let mut picked_target = None;
                let mut picked_guild_row = None;
                let mut picked_contact = None;
                let mut booth_put = None;
                // What happens to the item held: picked up, put down (or dropped).
                let mut held = data.state.held.clone();
                for event in gui.show(ui.ctx(), view.rect, &data, icons) {
                    match event {
                        // A booth's wares stay where they are (a slot's buttons act on them).
                        gui::GuiEvent::Pick { list, .. } if list.starts_with("booth.") => {}
                        gui::GuiEvent::Pick { list, index } => held = Some((list, index)),
                        gui::GuiEvent::Put { list, .. } => {
                            let Some((from, at)) = held.take() else {
                                continue;
                            };
                            match (from.as_str(), list.strip_prefix("work.")) {
                                // A bag item onto a workbench slot.
                                ("hero.bag", Some(_)) => {
                                    work.extend(player.bag.get(at).map(|i| (true, i.id)))
                                }
                                // A bag item onto the hero's booth: its price asked.
                                ("hero.bag", None) if list.starts_with("booth.mine") => {
                                    booth_put = player.bag.get(at).map(|i| i.id);
                                }
                                _ => actions.extend(move_item(&data, player, (&from, at), &list)),
                            }
                        }
                        // The right button while one is held lets it go.
                        gui::GuiEvent::Item { .. } if held.is_some() => held = None,
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
                        // A guild member's row picked (for Delete).
                        gui::GuiEvent::Item { list, index } if list == "guild.member_rows" => {
                            picked_guild_row = Some(index);
                        }
                        // The hotkey page's group or action picked.
                        gui::GuiEvent::Item { list, index }
                            if list == "hotkey.groups" || list == "hotkey.actions" =>
                        {
                            let key = if list == "hotkey.groups" {
                                "hotkey_group"
                            } else {
                                "hotkey_pick"
                            };
                            picked_contact = Some((key.to_owned(), index));
                        }
                        // A friend or enemy picked (its details open by the list's action).
                        gui::GuiEvent::Item { list, index }
                            if list == "friends.rows"
                                || list == "enemies.rows"
                                || list == "partners.rows"
                                || list == "students.rows"
                                || list == "board.rows" =>
                        {
                            let key = format!("{}_pick", list.trim_end_matches(".rows"));
                            picked_contact = Some((key, index));
                        }
                        // A listed name picked fills the chat bar's name; the list hides.
                        gui::GuiEvent::Item { list, index } if list == CHAT_TARGETS => {
                            picked_target = data.state.chat_bar.targets.get(index).cloned();
                        }
                        gui::GuiEvent::Item { list, index } => {
                            actions.extend(grid_click(&data, player, &list, index));
                        }
                        other => others.push(other),
                    }
                }
                // The edit asked to take the keyboard has had this frame to.
                if let Some((list, index)) = &held {
                    draw_held(ui, &data, list, *index, icons);
                }
                let steps: Vec<(String, String)> = (data.state.steps.iter())
                    .filter_map(|(group, step, max)| {
                        let now = gui::GuiData::view_number(&data, group) as i64;
                        let next = now + step;
                        let limit = gui::GuiData::number(&data, max).map(|m| m as i64);
                        let fits = next >= 0 && limit.is_none_or(|m| next < m);
                        fits.then(|| (group.clone(), next.to_string()))
                    })
                    .collect();
                self.gui_state.focus = None;
                self.gui_state.held = held;
                if std::mem::take(&mut self.gui_state.rebinding) {
                    let data = gui_data::HeroData {
                        view,
                        player,
                        state: &self.gui_state,
                    };
                    actions.extend(data.hotkey_picked().map(|r| UiAction::Rebind(r.action)));
                }
                if let Some(list) = self.gui_state.dropping.take() {
                    let data = gui_data::HeroData {
                        view,
                        player,
                        state: &self.gui_state,
                    };
                    if let Some(name) = data.picked_contact(&list) {
                        use c3_game::{FriendAction as F, Input, PartnerAction as P};
                        let drop = match list.as_str() {
                            "enemies" => Input::Friend(F::Forget(name)),
                            "students" => Input::Mentor(c3_game::MentorAction::Expel(name)),
                            "partners" => Input::Partner(P::Break(name)),
                            _ => Input::Friend(F::Remove(name)),
                        };
                        actions.push(UiAction::Game(drop));
                    }
                }
                if std::mem::take(&mut self.gui_state.guild_kicking) {
                    let data = gui_data::HeroData {
                        view,
                        player,
                        state: &self.gui_state,
                    };
                    let name = data.picked_guild_member();
                    let here = name.and_then(|n| view.sim.players().find(|p| p.name == n));
                    match here {
                        Some(p) => {
                            let kick = c3_game::GuildAction::Kick(p.id);
                            actions.push(UiAction::Game(c3_game::Input::Guild(kick)));
                        }
                        None => self.system("They aren't here.", style::BAD),
                    }
                }
                if std::mem::take(&mut self.gui_state.kicking) {
                    let data = gui_data::HeroData {
                        view,
                        player,
                        state: &self.gui_state,
                    };
                    if let Some(mate) = data.picked_member() {
                        let kick = c3_game::TeamAction::Kick(mate.id);
                        actions.push(UiAction::Game(c3_game::Input::Team(kick)));
                    }
                }
                if let Some(item) = booth_put {
                    self.gui_state.booth_item = Some(item);
                    self.gui_state.prompt = prompt::Prompt::confirm("booth_add:booth_add");
                }
                if let Some((whose, n)) = self.gui_state.booth_slot.take() {
                    let data = gui_data::HeroData {
                        view,
                        player,
                        state: &self.gui_state,
                    };
                    let item = data.booth_ware(&whose, n).map(|w| w.item.id);
                    let action = match (whose.as_str(), self.gui_state.booth, item) {
                        ("mine", _, Some(item)) => Some(c3_game::BoothAction::Remove(item)),
                        ("theirs", Some(owner), Some(item)) => {
                            Some(c3_game::BoothAction::Buy { owner, item })
                        }
                        _ => None,
                    };
                    actions.extend(action.map(|a| UiAction::Game(c3_game::Input::Booth(a))));
                }
                if std::mem::take(&mut self.gui_state.answering) {
                    actions.extend(typed_answer(&self.gui_state, player));
                }
                self.gui_state.steps.clear();
                self.gui_state.views.extend(steps);
                if let Some((binding, cast)) = self.gui_state.picking.take() {
                    let data = gui_data::HeroData {
                        view,
                        player,
                        state: &self.gui_state,
                    };
                    let skill = gui::GuiData::text(&data, &binding).and_then(|t| t.parse().ok());
                    let mine = |s: u32| view.sim.rules().skill(s).map(|t| t.target);
                    match skill.map(|s| (s, mine(s))) {
                        Some((s, Some(c3_game::SkillTarget::Myself))) if cast => {
                            let slot = player.skills.iter().position(|&k| k == s);
                            actions.extend(slot.map(UiAction::Cast));
                        }
                        Some((s, _)) => self.gui_state.picked_skill = Some(s),
                        None => {}
                    }
                }
                if let Some((key, row)) = picked_contact {
                    // Another group picked: none of its actions is yet.
                    if key == "hotkey_group" {
                        self.gui_state.views.remove("hotkey_pick");
                    }
                    self.gui_state.views.insert(key, row.to_string());
                }
                if let Some(row) = picked_guild_row {
                    self.gui_state
                        .views
                        .insert("guild_pick".into(), row.to_string());
                }
                if let Some(name) = picked_target {
                    self.gui_state.fields.insert(CHAT_NAME.into(), name);
                    if let Some(gui) = &mut self.gui {
                        gui.close_holding(CHAT_TARGETS);
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
                self.contact_word(view, player, &mut actions);
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
        // The engine's skill bar, unless the game UI shows the hero's skills itself.
        let own_skills = self.gui.as_ref().is_some_and(|g| g.binds("skill."));
        if !own_skills {
            hud::skill_bar(ui, view, player, &mut actions);
        }
        let lift = if player.skills.is_empty() || own_skills {
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
