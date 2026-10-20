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
    /// The emotions a line may show (`#05`: the sixth), each the skin section `emote_section`
    /// and its number; none when 0.
    pub emote_section: String,
    pub emotes: usize,
}

/// A frame of an emotion each this many seconds in the log (CMyBitmap::ShowString's 200 ms).
const EMOTE_FRAME: f64 = 0.2;

/// The emotions of `text` (`#` and two digits below `count`, as CMyBitmap::ShowString reads
/// them): the text with each one's three letters made one space, and where each is (in
/// letters of that text) with its number.
fn emote_spots(text: &str, count: usize) -> (String, Vec<(usize, usize)>) {
    let chars: Vec<char> = text.chars().collect();
    let (mut out, mut spots) = (String::new(), Vec::new());
    let mut i = 0;
    while i < chars.len() {
        let digits = (chars.get(i + 1).and_then(|c| c.to_digit(10)))
            .zip(chars.get(i + 2).and_then(|c| c.to_digit(10)));
        match digits.map(|(a, b)| (a * 10 + b) as usize) {
            Some(n) if chars[i] == '#' && n < count => {
                spots.push((out.chars().count(), n));
                out.push(' ');
                i += 3;
            }
            _ => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    (out, spots)
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
    /// lines up from `bottom` in their channels' colours, shadowed, wrapped to the screen's
    /// width; emotions drawn in place of their `#NN`, a font and a half wide.
    fn show_look(
        &mut self,
        ui: &egui::Ui,
        rect: Rect,
        look: &ChatLook,
        skin: Option<&crate::gui::Skin>,
        textures: &mut dyn crate::gui::Textures,
    ) {
        use egui::text::{CCursor, LayoutJob, TextFormat};
        let scale = (rect.width() / look.base[0]).min(rect.height() / look.base[1]);
        let painter = ui.painter_at(rect);
        let font = fonts::font(12.0 * scale);
        let [r, g, b] = look.color;
        let note_color = Color32::from_rgb(r, g, b);
        let height = 14.0 * scale;
        let wrap = rect.width() - 16.0 * scale;
        let cell = 18.0 * scale;
        let space = (painter.layout_no_wrap(" ".into(), font.clone(), Color32::WHITE))
            .size()
            .x;
        let frame = (self.now / EMOTE_FRAME) as usize;
        let count = if skin.is_some() { look.emotes } else { 0 };
        let mut draw = |text: &str, color: Color32, y: f32| -> f32 {
            let (text, spots) = emote_spots(text, count);
            let job = |color: Color32| {
                let mut job = LayoutJob::default();
                job.wrap.max_width = wrap;
                let plain = TextFormat::simple(font.clone(), color);
                let gap = TextFormat {
                    extra_letter_spacing: cell - space,
                    ..plain.clone()
                };
                let mut from = 0;
                for (at, _) in &spots {
                    let start = text.char_indices().nth(*at).map_or(text.len(), |(b, _)| b);
                    job.append(&text[from..start], 0.0, plain.clone());
                    job.append(" ", 0.0, gap.clone());
                    from = start + 1;
                }
                job.append(&text[from..], 0.0, plain);
                job
            };
            let shadow = painter.layout_job(job(Color32::BLACK));
            let galley = painter.layout_job(job(color));
            let at = egui::pos2(rect.left() + 2.0 * scale, y - galley.size().y);
            painter.galley(at + egui::vec2(1.0, 1.0), shadow, Color32::BLACK);
            for (letter, n) in &spots {
                let place = galley.pos_from_cursor(CCursor::new(*letter));
                let section = format!("{}{n}", look.emote_section);
                let frames = skin.map_or(&[][..], |s| s.frames(&section));
                let path = frames.get(frame % frames.len().max(1));
                if let Some((texture, _)) = path.and_then(|p| textures.image(ui.ctx(), p)) {
                    let middle = at + egui::vec2(place.left() + cell / 2.0, place.center().y);
                    let square = Rect::from_center_size(middle, egui::vec2(cell, cell));
                    let whole = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                    painter.image(texture, square, whole, Color32::WHITE);
                }
            }
            let tall = galley.size().y;
            painter.galley(at, galley, color);
            tall
        };
        let now = self.now;
        self.notes.retain(|(_, at)| now - at < SYSTEM_SECONDS);
        // The system area: its newest note lowest, ending five lines down.
        let mut y = rect.top() + height * 5.0;
        for (text, _) in self.notes.iter().rev() {
            y -= draw(text, note_color, y) + 2.0 * scale;
        }
        let mut y = rect.bottom() - look.bottom * scale;
        for (text, color) in self.lines.iter().rev().take(look.lines) {
            y -= draw(text, *color, y) + 2.0 * scale;
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
    /// skill bar); a game's look draws its emotions from `skin`.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        lift: f32,
        skin: Option<&crate::gui::Skin>,
        textures: &mut dyn crate::gui::Textures,
        actions: &mut Vec<UiAction>,
    ) {
        self.now = ui.input(|i| i.time);
        if let Some(look) = self.look.clone() {
            self.show_look(ui, rect, &look, skin, textures);
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

#[cfg(test)]
mod tests {
    use super::emote_spots;

    #[test]
    fn emotions_are_hash_and_two_digits_below_the_count() {
        let (text, spots) = emote_spots("hi #05 #7 #61 #60!", 61);
        assert_eq!(text, "hi   #7 #61  !");
        assert_eq!(spots, vec![(3, 5), (12, 60)]);
    }
}
