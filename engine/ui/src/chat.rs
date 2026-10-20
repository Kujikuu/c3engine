//! The chat log at the bottom left: system messages, notices and what players say, with an
//! input line (Enter to type, Enter to send, Esc to stop).

use crate::fonts;
use crate::style;
use crate::{UiAction, item_name};
use c3_game::{EntityId, Event, Sim};
use egui::{Color32, Rect};
use std::collections::VecDeque;

/// Lines kept in the log.
const KEEP: usize = 100;
/// Lines shown.
const SHOWN: usize = 7;
const WIDTH: f32 = 380.0;

/// How a game shows its chat log, as TQ's CGameMsg does: lines of plain shadowed text in one
/// colour, said lines worded `<prefix><speaker><to><hearer><says><text>` with the clock after,
/// the newest at the bottom a little above the screen's bottom; the game's own notes in a
/// system area at the top left that each go after 15 seconds. None: the engine's boxed lines.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatLook {
    /// Before a said line of each channel, and before a system note.
    pub talk: String,
    pub whisper: String,
    pub team: String,
    pub guild: String,
    pub friend: String,
    /// Before words a friend left (TQ's [Message]).
    pub leave_word: String,
    pub system: String,
    /// Between the speaker and the hearer; after the hearer; the hearer of a line to all.
    pub to: String,
    pub says: String,
    pub everyone: String,
    /// The clock (`[hour:minute]`) after each said line.
    pub clock: bool,
    pub color: [u8; 3],
    /// Said lines shown, and system notes.
    pub lines: usize,
    pub system_lines: usize,
    /// How far above the screen's bottom the newest said line ends, in the layout's pixels
    /// (`base` high).
    pub bottom: f32,
    pub base: [f32; 2],
    /// The colours a player may give each channel (TQ's eight), and each channel's first
    /// (`[talk, 7]`: by channel key, see [`channel_key`]).
    pub palette: Vec<[u8; 3]>,
    pub colors: Vec<(String, usize)>,
}

/// What the player chose to see of the chat (TQ's chat options and blacklist): channels hidden
/// (by [`channel_key`]), whispers from friends only, each channel's colour (an index into the
/// look's palette), names never heard.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatFilter {
    pub hidden: std::collections::BTreeSet<String>,
    pub friends_only: bool,
    pub colors: std::collections::BTreeMap<String, usize>,
    pub blacklist: Vec<String>,
}

/// The key a chat channel's options go by: `talk`, `whisper`, `team`, `guild`, `friend`,
/// `leave_word`, `board`, `broadcast`.
pub fn channel_key(channel: c3_game::ChatChannel) -> &'static str {
    use c3_game::ChatChannel as C;
    match channel {
        C::Local => "talk",
        C::Whisper => "whisper",
        C::Team => "team",
        C::Guild => "guild",
        C::Friend => "friend",
        C::LeaveWord => "leave_word",
        C::Board(_) => "board",
        C::Broadcast => "broadcast",
    }
}

/// How long a system note stays, in seconds.
const SYSTEM_SECONDS: f64 = 15.0;

#[derive(Default)]
pub(crate) struct Chat {
    lines: VecDeque<(String, Color32)>,
    /// The game's look, its system notes (with when they came) and the clock now.
    look: Option<ChatLook>,
    notes: VecDeque<(String, f64)>,
    now: f64,
    input: String,
    pub typing: bool,
    /// Opened this frame (by an Enter that mustn't also send).
    opening: bool,
    /// What the player chose to see.
    pub filter: ChatFilter,
}

impl Chat {
    /// Shows the log in a game's look from now on.
    pub fn set_look(&mut self, look: Option<ChatLook>) {
        self.look = look;
    }

    /// A note of the game's own (not said by anyone): a system note in a game's look.
    pub fn push(&mut self, text: String, color: Color32) {
        if let Some(look) = &self.look {
            self.notes
                .push_back((format!("{}{text}", look.system), self.now));
            while self.notes.len() > look.system_lines.max(1) {
                self.notes.pop_front();
            }
            return;
        }
        self.push_said(text, color);
    }

    /// A said line, at the bottom of the log.
    fn push_said(&mut self, text: String, color: Color32) {
        self.lines.push_back((text, color));
        if self.lines.len() > KEEP {
            self.lines.pop_front();
        }
    }

    /// Whether a line `name` said on `channel` is filtered out (a channel hidden, a name
    /// blacklisted, a whisper from someone not a friend while only friends' show).
    fn filtered(
        &self,
        sim: &Sim,
        hero: EntityId,
        channel: c3_game::ChatChannel,
        name: &str,
    ) -> bool {
        let f = &self.filter;
        let whisper = channel == c3_game::ChatChannel::Whisper;
        let me = sim.player(hero).map_or("", |p| p.name.as_str());
        f.hidden.contains(channel_key(channel))
            || f.blacklist.iter().any(|b| b.eq_ignore_ascii_case(name))
            || (whisper && f.friends_only && !sim.world_state().are_friends(me, name))
    }

    /// A line for what happened to the hero.
    pub fn event(&mut self, event: &Event, sim: &Sim, hero: EntityId) {
        if let Event::Chat { channel, name, .. } = event
            && self.filtered(sim, hero, *channel, name)
        {
            return;
        }
        let currency = &sim.settings().currency;
        let line = match event {
            Event::Died { id, .. } if *id == hero => Some((
                format!(
                    "You died. You come back in {} seconds.",
                    sim.settings().revive_seconds
                ),
                style::BAD,
            )),
            Event::Revived(id) if *id == hero => Some(("You are back.".to_owned(), style::GOOD)),
            Event::StatusEnded { target, status } if *target == hero => sim
                .rules()
                .status(*status)
                .map(|t| (format!("{} wore off.", t.name), style::DIM)),
            Event::LeveledUp { player, level } if *player == hero => {
                Some((format!("Level up! You are level {level}."), style::GOLD))
            }
            Event::PickedUp {
                player,
                money,
                kind,
            } if *player == hero => Some((
                match kind {
                    Some(k) => format!("You picked up {}.", item_name(sim, *k)),
                    None => format!("You picked up {money} {currency}."),
                },
                style::GOOD,
            )),
            Event::Bought {
                player,
                kind,
                price,
            } if *player == hero => Some((
                format!(
                    "You bought {} for {price} {currency}.",
                    item_name(sim, *kind)
                ),
                style::TEXT,
            )),
            Event::Sold {
                player,
                kind,
                price,
            } if *player == hero => Some((
                format!("You sold {} for {price} {currency}.", item_name(sim, *kind)),
                style::TEXT,
            )),
            Event::Used { player, kind } if *player == hero => {
                Some((format!("You used {}.", item_name(sim, *kind)), style::TEXT))
            }
            Event::Message { player, text } if *player == hero => Some((text.clone(), style::WARN)),
            Event::Notice { player, text } if *player == hero => Some((text.clone(), style::GOLD)),
            Event::Chat {
                name,
                text,
                channel,
                hearer,
                ..
            } if self.look.is_some() => {
                let look = self.look.clone().expect("checked");
                use c3_game::ChatChannel as C;
                let (prefix, to) = match channel {
                    C::Local => (&look.talk, &look.everyone),
                    C::Whisper => (&look.whisper, hearer),
                    C::Team => (&look.team, &look.everyone),
                    C::Guild => (&look.guild, &look.everyone),
                    C::Friend => (&look.friend, &look.everyone),
                    C::LeaveWord => (&look.leave_word, hearer),
                    // Board posts are read on their board, broadcasts on the game's banner.
                    C::Board(_) | C::Broadcast => return,
                };
                let clock = match look.clock {
                    true => {
                        let now = chrono::Local::now();
                        use chrono::Timelike;
                        format!("[{}:{}]", now.hour(), now.minute())
                    }
                    false => String::new(),
                };
                // The channel's colour as the player chose it, else the look's first.
                let key = channel_key(*channel);
                let index = (self.filter.colors.get(key).copied())
                    .or_else(|| look.colors.iter().find(|(k, _)| k == key).map(|(_, i)| *i));
                let [r, g, b] = index
                    .and_then(|i| look.palette.get(i).copied())
                    .unwrap_or(look.color);
                let line = format!("{prefix}{name}{}{to}{}{text}{clock}", look.to, look.says);
                self.push_said(line, Color32::from_rgb(r, g, b));
                return;
            }
            Event::Chat {
                name,
                text,
                channel,
                ..
            } => {
                let tag = match channel {
                    c3_game::ChatChannel::Local => "",
                    c3_game::ChatChannel::Whisper => "[Whisper] ",
                    c3_game::ChatChannel::Team => "[Team] ",
                    c3_game::ChatChannel::Guild => "[Guild] ",
                    c3_game::ChatChannel::Friend => "[Friend] ",
                    c3_game::ChatChannel::LeaveWord => "[Message] ",
                    c3_game::ChatChannel::Board(_) => "[Board] ",
                    c3_game::ChatChannel::Broadcast => "[Broadcast] ",
                };
                Some((format!("{tag}{name}: {text}"), style::TEXT))
            }
            Event::CommandReply { player, ok, text } if *player == hero => {
                Some((text.clone(), if *ok { style::GOOD } else { style::WARN }))
            }
            _ => None,
        };
        if let Some((text, color)) = line {
            self.push(text, color);
        }
    }

    /// The log in a game's look: system notes from the top left (going after 15 s), said
    /// lines up from `bottom`, shadowed, wrapped to the screen's width.
    fn show_look(&mut self, ui: &egui::Ui, rect: Rect, look: &ChatLook) {
        let scale = (rect.width() / look.base[0]).min(rect.height() / look.base[1]);
        let painter = ui.painter_at(rect);
        let font = fonts::font(12.0 * scale);
        let [r, g, b] = look.color;
        let color = Color32::from_rgb(r, g, b);
        let height = 14.0 * scale;
        let wrap = rect.width() - 16.0 * scale;
        let draw = |text: &str, y: f32| -> f32 {
            let shadow = painter.layout(text.to_owned(), font.clone(), Color32::BLACK, wrap);
            let galley = painter.layout(text.to_owned(), font.clone(), color, wrap);
            let at = egui::pos2(rect.left() + 2.0 * scale, y - galley.size().y);
            painter.galley(at + egui::vec2(1.0, 1.0), shadow, Color32::BLACK);
            let tall = galley.size().y;
            painter.galley(at, galley, color);
            tall
        };
        let now = self.now;
        self.notes.retain(|(_, at)| now - at < SYSTEM_SECONDS);
        // The system area: its newest note lowest, ending five lines down.
        let mut y = rect.top() + height * 5.0;
        for (text, _) in self.notes.iter().rev() {
            y -= draw(text, y) + 2.0 * scale;
        }
        let mut y = rect.bottom() - look.bottom * scale;
        for (text, _) in self.lines.iter().rev().take(look.lines) {
            y -= draw(text, y) + 2.0 * scale;
        }
    }

    /// Clears the lines shown.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.notes.clear();
    }

    /// Starts typing a line.
    pub fn open(&mut self) {
        self.typing = true;
        self.opening = true;
    }

    /// Draws the log and the line being typed, `lift` above its usual place (clear of the
    /// skill bar).
    pub fn show(&mut self, ui: &mut egui::Ui, rect: Rect, lift: f32, actions: &mut Vec<UiAction>) {
        self.now = ui.input(|i| i.time);
        if let Some(look) = self.look.clone() {
            self.show_look(ui, rect, &look);
            if !self.typing {
                return;
            }
        }
        let bottom = rect.bottom() - 74.0 - lift;
        let painter = ui.painter_at(rect);
        let font = fonts::font(13.0);
        let mut y = bottom - if self.typing { 28.0 } else { 0.0 };
        let shown = if self.look.is_some() { 0 } else { SHOWN };
        for (text, color) in self.lines.iter().rev().take(shown) {
            let galley = painter.layout(text.clone(), font.clone(), *color, WIDTH);
            y -= galley.size().y + 3.0;
            let pos = egui::pos2(rect.left() + 12.0, y);
            painter.rect_filled(
                Rect::from_min_size(
                    pos - egui::vec2(5.0, 1.0),
                    galley.size() + egui::vec2(10.0, 2.0),
                ),
                3.0,
                style::backing(0.8),
            );
            painter.galley(pos, galley, *color);
        }
        if !self.typing {
            return;
        }
        let id = egui::Id::unique("game chat input");
        egui::Area::new(id.with("area"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(rect.left() + 8.0, bottom - 24.0))
            .show(ui.ctx(), |ui| {
                fonts::use_game_font(ui);
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.input)
                        .id(id)
                        .hint_text("Say something, or /help")
                        .desired_width(WIDTH),
                );
                field.request_focus();
                let (send, cancel) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::Enter),
                        i.key_pressed(egui::Key::Escape),
                    )
                });
                // The Enter that opened the line isn't the one that sends it.
                let opening = std::mem::take(&mut self.opening);
                let send = send && !opening;
                if send {
                    let text = std::mem::take(&mut self.input);
                    if !text.trim().is_empty() {
                        actions.push(UiAction::Say(text.trim().to_owned()));
                    }
                    self.typing = false;
                } else if cancel {
                    self.typing = false;
                }
            });
    }
}
