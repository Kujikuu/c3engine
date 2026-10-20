//! Drawing a [`Gui`] over the game and turning clicks into what the player asked for.
//!
//! The layout is made for one screen size (`[0-0]`); it is scaled to fit the game area, and
//! each dialog keeps to the side of the screen it was placed on (bottom, top, a corner).

use super::{Control, ControlKind, Fill, Gui, GuiRect, Skin};
use crate::Icons;
use egui::{Color32, Pos2, Rect, TextureId, Vec2};

/// The skin section drawn over a focused image.
const FOCUS: &str = "ImageFocus";
/// The part of the focus picture drawn (`CMyImage::ShowFocus` takes 100 by 100).
const FOCUS_PART: Rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 100.0));

/// The images a UI shows, by path, as textures of their size in pixels.
pub trait GuiImages {
    fn image(&mut self, ctx: &egui::Context, path: &str) -> Option<(TextureId, Vec2)>;
}

/// Item icons and UI images: what a UI is drawn with.
pub trait Textures: Icons + GuiImages {}

impl<T: Icons + GuiImages> Textures for T {}

/// Space between stacked controls, and under the last in a fitted dialog.
const STACK_GAP: i32 = 4;
const FIT_PAD: i32 = 10;

/// What bindings (`hero.life`) read.
pub trait GuiData {
    fn number(&self, key: &str) -> Option<f64>;
    fn text(&self, key: &str) -> Option<String> {
        self.number(key).map(|n| format!("{n}"))
    }
    /// The looks of a list's items (`Sim::item_look`), in slot order; none: each its usual.
    fn item_looks(&self, _key: &str) -> Vec<u32> {
        Vec::new()
    }
    /// Item kinds of a list (a bag), in slot order.
    fn items(&self, _key: &str) -> Vec<u32> {
        Vec::new()
    }
    /// A picture the host drew for `key` (a 3D look), as a texture.
    fn image(&self, _key: &str) -> Option<TextureId> {
        None
    }
    /// The field whose edit should take the keyboard now (the chat line on Enter).
    fn focus(&self) -> Option<&str> {
        None
    }
    /// The rows of a list control and which one is picked.
    fn rows(&self, _key: &str) -> (Vec<String>, Option<usize>) {
        (Vec::new(), None)
    }
    /// What item `index` of list `key` is, for its tooltip.
    fn item_tip(&self, _key: &str, _index: usize) -> Option<String> {
        None
    }
    /// Whether an item is picked up (a click on a slot puts it there).
    fn holding(&self) -> bool {
        false
    }
    /// The number view group `group` holds (a grid's page offset; 0 when none).
    fn view_number(&self, _group: &str) -> usize {
        0
    }
}

/// What the player did on the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuiEvent {
    /// A control with `action=` was clicked.
    Action(String),
    /// The pointer is over a control with `pointed=` (its value).
    Pointed(String),
    /// The clicked control's own sound (played in place of the UI's click).
    Sound(String),
    /// Slot `index` of grid list `list` (or row `index` of a list control) was clicked.
    Item {
        list: String,
        index: usize,
    },
    /// Edit `field` now reads `text`.
    Edited {
        field: String,
        text: String,
    },
    /// Slider `field` moved to `value`.
    Slid {
        field: String,
        value: u32,
    },
    /// Item `index` of grid `list` picked up, and the item held put on slot `index` of
    /// `list` (a `pick` grid).
    Pick {
        list: String,
        index: usize,
    },
    Put {
        list: String,
        index: usize,
    },
    /// A key pressed while edit `field` has the keyboard.
    EditKey {
        field: String,
        key: EditKey,
    },
}

/// The keys an edit tells the game about (the chat line's history, channels and names).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKey {
    Up,
    Down,
    CtrlUp,
    CtrlDown,
    CtrlLeft,
    CtrlRight,
}

/// How a dialog's layout pixels sit on the screen.
#[derive(Debug, Clone, Copy)]
pub struct Place {
    pub scale: f32,
    /// Screen point of the dialog's layout origin.
    pub origin: Pos2,
}

impl Place {
    /// The screen point of layout point `x`, `y` in the dialog.
    pub fn screen(&self, x: f32, y: f32) -> Pos2 {
        self.origin + Vec2::new(x, y) * self.scale
    }
}

impl Gui {
    /// Draws the open dialogs over `area`; returns what was clicked.
    pub fn show(
        &self,
        ctx: &egui::Context,
        area: Rect,
        data: &dyn GuiData,
        textures: &mut dyn Textures,
    ) -> Vec<GuiEvent> {
        let mut events = Vec::new();
        let shown = |d: &Control| {
            d.screen == self.screen && !d.when.is_empty() && set(d, data) && !self.dismissed(d.id)
        };
        let now = ctx.input(|i| i.time);
        let showing: Vec<&Control> = (self.dialogs())
            .filter(|d| self.is_open(d) || shown(d))
            .collect();
        let mut opened = self.opened_at.borrow_mut();
        // A sliding dialog sounds as it comes and goes (TQ's DlgOpen / DlgClose).
        let sliding = |id: u32| self.dialogs().any(|d| d.id == id && d.slide);
        let gone = (opened.keys()).filter(|id| !showing.iter().any(|d| d.id == **id));
        if gone.copied().any(sliding) && !self.sounds.close.is_empty() {
            events.push(GuiEvent::Sound(self.sounds.close.clone()));
        }
        opened.retain(|id, _| showing.iter().any(|d| d.id == *id));
        let came = showing
            .iter()
            .any(|d| d.slide && !opened.contains_key(&d.id));
        if came && !self.sounds.open.is_empty() {
            events.push(GuiEvent::Sound(self.sounds.open.clone()));
        }
        for dialog in showing {
            let r = self.fitted(ctx, dialog, data).0;
            let mut place = self.place_rect(r, area);
            let since = *opened.entry(dialog.id).or_insert(now);
            if dialog.slide {
                place.origin.x += self.slide_offset(r, place.scale, now - since);
                ctx.request_repaint();
            }
            let size = Vec2::new(r.w.max(0) as f32, r.h.max(0) as f32) * place.scale;
            egui::Area::new(egui::Id::unique(("gui dialog", dialog.id)))
                .fixed_pos(place.origin)
                .order(egui::Order::Middle)
                .show(ctx, |ui| {
                    // The dialog takes the pointer, so clicks on it don't reach the world; a
                    // right click asks for its `right=`.
                    let (_, response) = ui.allocate_exact_size(size, egui::Sense::click());
                    if response.secondary_clicked() && !dialog.right.is_empty() {
                        events.push(GuiEvent::Action(dialog.right.clone()));
                    }
                    events.extend(self.paint_dialog(ui, dialog, place, data, textures, true));
                });
        }
        events
    }

    /// The screen point of the top-left of the control whose action is `action` (the map
    /// check the radar grows out of), when the UI is drawn over `area`.
    pub fn action_spot(&self, action: &str, area: Rect) -> Option<Pos2> {
        let control = (self.controls.iter()).find(|c| c.parent != 0 && c.action == action)?;
        let dialog = self.dialogs().find(|d| d.id == control.parent)?;
        let r = self.rect(control);
        Some(self.place(dialog, area).screen(r.x as f32, r.y as f32))
    }

    /// How far (screen pixels) a sliding dialog laid out at `r` still is from its place
    /// `seconds` after it opened: off its side of the screen at first, a twentieth of its
    /// width nearer every 15 ms (TQ's frame mode 1).
    fn slide_offset(&self, r: GuiRect, scale: f32, seconds: f64) -> f32 {
        const STEPS: f64 = 20.0;
        let steps = (seconds * 1000.0 / 15.0).floor().min(STEPS);
        let left = (1.0 - steps / STEPS) as f32 * r.w.max(0) as f32 * scale;
        let base = self.layout.base()[0] as f32;
        match (r.x as f32 + r.w as f32 / 2.0) < base / 2.0 {
            true => -left,
            false => left,
        }
    }

    /// Where `dialog` sits when the UI is drawn over `area`.
    pub fn place(&self, dialog: &Control, area: Rect) -> Place {
        self.place_rect(self.rect(dialog), area)
    }

    /// Where a dialog laid out at `r` sits when the UI is drawn over `area`.
    fn place_rect(&self, r: GuiRect, area: Rect) -> Place {
        let [bw, bh] = self.layout.base().map(|v| v as f32);
        let scale = (area.width() / bw).min(area.height() / bh);
        Place {
            scale,
            origin: anchored(area, [bw, bh], scale, r),
        }
    }

    /// Draws `dialog` and its controls into `ui`; `live` controls answer the pointer (the
    /// game), others are only drawn (an editor). Returns what was clicked.
    pub fn paint_dialog(
        &self,
        ui: &mut egui::Ui,
        dialog: &Control,
        place: Place,
        data: &dyn GuiData,
        textures: &mut dyn Textures,
        live: bool,
    ) -> Vec<GuiEvent> {
        let mut events = Vec::new();
        let mut painter = Painter {
            ui,
            textures,
            skin: &self.skin,
            names: &self.names,
            place,
            live,
        };
        let (fitted, stacked) = self.fitted(painter.ui.ctx(), dialog, data);
        let stretch = (dialog.fit > 0).then(|| Vec2::new(fitted.w as f32, fitted.h as f32));
        painter.picture(dialog, &dialog.ani, Pos2::ZERO, stretch);
        for control in (self.controls.iter()).filter(|c| c.parent == dialog.id) {
            // A control with `when` shows only while one of its bindings is set.
            if control.kind != ControlKind::Dialog
                && (control.when.is_empty() || set(control, data))
            {
                let rect = (stacked.iter())
                    .find(|(id, _)| *id == control.id)
                    .map_or_else(|| self.rect(control), |(_, r)| *r);
                painter.control(control, rect, data, &mut events);
                if live && !control.pointed.is_empty() {
                    let at = place.screen(rect.x as f32, rect.y as f32);
                    let size = Vec2::new(rect.w.max(0) as f32, rect.h.max(0) as f32) * place.scale;
                    let pointer = painter.ui.input(|i| i.pointer.hover_pos());
                    if pointer.is_some_and(|p| Rect::from_min_size(at, size).contains(p)) {
                        events.push(GuiEvent::Pointed(control.pointed.clone()));
                    }
                }
            }
        }
        // The page that shows, drawn where the dialog places it.
        if let Some(page) = self.shown_page(dialog.id) {
            let r = self.rect(page);
            let inner = Place {
                origin: place.screen(r.x as f32, r.y as f32),
                ..place
            };
            events.extend(self.paint_dialog(painter.ui, page, inner, data, painter.textures, live));
        }
        events
    }

    /// `dialog`'s place and its stacked controls' (`stack=1`, each under the one before,
    /// as tall as what it shows: wrapped text, list rows, an edit while shown); a `fit`
    /// dialog's height follows them, 10 below the last, `fit` at least.
    pub fn fitted(
        &self,
        ctx: &egui::Context,
        dialog: &Control,
        data: &dyn GuiData,
    ) -> (GuiRect, Vec<(u32, GuiRect)>) {
        let mut place = self.rect(dialog);
        let mut stacked = Vec::new();
        let mut next: Option<i32> = None;
        let mut last_y: Option<i32> = None;
        for control in (self.controls.iter()).filter(|c| c.parent == dialog.id && c.stack) {
            let shows = control.when.is_empty() || set(control, data);
            let mut r = self.rect(control);
            r.y = match (control.beside, last_y) {
                (true, Some(y)) => y,
                _ => next.map_or(r.y, |n| n.max(control.min_y)),
            };
            r.h = match (shows, control.kind) {
                (false, _) => 0,
                (true, ControlKind::Text) => {
                    let text = expand(&control.text, data);
                    let wrap = if control.wrap && r.w > 0 {
                        r.w as f32
                    } else {
                        f32::INFINITY
                    };
                    let font = crate::fonts::font(control.size);
                    let galley = ctx.fonts_mut(|f| f.layout(text, font, Color32::WHITE, wrap));
                    galley.size().y.ceil() as i32
                }
                (true, ControlKind::List) => {
                    let rows = data.rows(&control.value).0;
                    let laid = list_rows(ctx, control, &rows, r.w as f32);
                    laid.iter().map(|l| l.bottom()).fold(0.0, f32::max).ceil() as i32
                }
                (true, _) => r.h,
            };
            let below = r.y + r.h + if r.h > 0 { STACK_GAP } else { 0 };
            next = Some(next.map_or(below, |n| n.max(below)));
            if r.h > 0 {
                last_y = Some(r.y);
            }
            stacked.push((control.id, r));
        }
        if dialog.fit > 0 {
            let bottom = stacked.iter().map(|(_, r)| r.y + r.h).max().unwrap_or(0);
            place.h = (bottom + FIT_PAD).max(dialog.fit);
        }
        if let Some(s) = &dialog.stretch {
            let count = data.number(&s.count).unwrap_or(0.0).max(0.0) as i32;
            place.w = count.min(s.max) * s.step + s.pad;
            place.x = self.layout.base()[0] - place.w;
        }
        (place, stacked)
    }

    /// The screen rectangle `control` covers in a dialog placed at `place`: its layout size,
    /// else its picture's.
    pub fn bounds(
        &self,
        ctx: &egui::Context,
        control: &Control,
        place: Place,
        textures: &mut dyn Textures,
    ) -> Rect {
        let r = if control.parent == 0 {
            GuiRect {
                x: 0,
                y: 0,
                ..self.rect(control)
            }
        } else {
            self.rect(control)
        };
        let mut picture = || {
            if let Some([l, t, r, b]) = control.src {
                return Some(Vec2::new((r - l) as f32, (b - t) as f32));
            }
            let frames = control.frames(&self.skin, &control.ani);
            let path = frames.get(control.frame).or(frames.first())?;
            textures.image(ctx, path).map(|(_, size)| size)
        };
        let size = if r.w > 0 && r.h > 0 {
            Vec2::new(r.w as f32, r.h as f32)
        } else {
            picture().unwrap_or(Vec2::splat(12.0))
        };
        Rect::from_min_size(place.screen(r.x as f32, r.y as f32), size * place.scale)
    }
}

/// A click on `control`: its own sound, then its action.
fn clicked(control: &Control, events: &mut Vec<GuiEvent>) {
    if !control.sound.is_empty() {
        events.push(GuiEvent::Sound(control.sound.clone()));
    }
    events.push(GuiEvent::Action(control.action.clone()));
}

/// The arrow keys pressed in edit `field`, plain or with Ctrl.
fn edit_keys(ui: &egui::Ui, field: &str, events: &mut Vec<GuiEvent>) {
    use egui::Key;
    let keys = ui.input(|i| {
        let ctrl = i.modifiers.ctrl;
        [
            (Key::ArrowUp, !ctrl, EditKey::Up),
            (Key::ArrowDown, !ctrl, EditKey::Down),
            (Key::ArrowUp, ctrl, EditKey::CtrlUp),
            (Key::ArrowDown, ctrl, EditKey::CtrlDown),
            (Key::ArrowLeft, ctrl, EditKey::CtrlLeft),
            (Key::ArrowRight, ctrl, EditKey::CtrlRight),
        ]
        .into_iter()
        .filter(|(key, when, _)| *when && i.key_pressed(*key))
        .map(|(_, _, k)| k)
        .collect::<Vec<_>>()
    });
    for key in keys {
        events.push(GuiEvent::EditKey {
            field: field.to_owned(),
            key,
        });
    }
}

/// Whether `control`'s `when` holds (see [`set`]).
pub(super) fn holds(control: &Control, data: &dyn GuiData) -> bool {
    set(control, data)
}

/// Whether `control`'s `when` holds: alternatives between `|`, each of terms all holding
/// between `&`; a term is a binding set (not 0), `!` one unset, or a comparison of a binding
/// with a number or another binding (`hero.level>=70`).
fn set(control: &Control, data: &dyn GuiData) -> bool {
    expression(&control.when, data)
}

/// Whether a `when`-style expression holds (a check's lit frame takes one too). A binding is
/// set when its number is not 0, or, without a number, when its text is not empty.
fn expression(text: &str, data: &dyn GuiData) -> bool {
    let is_set = |w: &str| match w.parse::<f64>().ok().or_else(|| data.number(w)) {
        Some(v) => v != 0.0,
        None => data.text(w).is_some_and(|t| !t.is_empty()),
    };
    let holds = |w: &str| match compare(w, data) {
        Some(holds) => holds,
        None => match w.strip_prefix('!') {
            Some(w) => !is_set(w),
            None => is_set(w),
        },
    };
    (text.split('|').filter(|w| !w.is_empty())).any(|w| w.split('&').all(holds))
}

/// `text` with its first printf conversion (`%s`, `%d`, `%u`, `%0.2f`, ...) replaced by `with`
/// (a names-file string's blank filled).
fn fill_first(text: &str, with: &str) -> String {
    let bytes = text.as_bytes();
    let mut at = 0;
    while let Some(start) = text[at..].find('%').map(|i| at + i) {
        let mut end = start + 1;
        while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
            end += 1;
        }
        if end < bytes.len() && b"sdufx".contains(&bytes[end]) {
            return format!("{}{with}{}", &text[..start], &text[end + 1..]);
        }
        at = start + 1;
    }
    text.to_owned()
}

/// A comparison term (`a>=b`, `<=`, `!=`, `>`, `<`, `=`), each side a number or a binding
/// (unknown: 0); none when the term has no comparison.
fn compare(term: &str, data: &dyn GuiData) -> Option<bool> {
    const OPS: [&str; 6] = [">=", "<=", "!=", ">", "<", "="];
    let (at, op) = OPS
        .iter()
        .filter_map(|op| term.find(op).filter(|&i| i > 0).map(|i| (i, *op)))
        .min_by_key(|(i, op)| (*i, std::cmp::Reverse(op.len())))?;
    let value = |side: &str| {
        let side = side.trim();
        // A number, a numeric binding, or a text binding holding a number.
        (side.parse().ok())
            .or_else(|| data.number(side))
            .or_else(|| data.text(side)?.trim().parse().ok())
            .unwrap_or(0.0)
    };
    let (a, b) = (value(&term[..at]), value(&term[at + op.len()..]));
    Some(match op {
        ">=" => a >= b,
        "<=" => a <= b,
        "!=" => a != b,
        ">" => a > b,
        "<" => a < b,
        _ => a == b,
    })
}

/// Where a dialog's origin lands: scaled, and kept to its side of the screen.
fn anchored(area: Rect, base: [f32; 2], scale: f32, r: GuiRect) -> Pos2 {
    let (x, y, w, h) = (r.x as f32, r.y as f32, r.w.max(0) as f32, r.h.max(0) as f32);
    let centre = [x + w / 2.0, y + h / 2.0];
    let side = |c: f32, base: f32, at: f32, len: f32, screen: f32| {
        if c < base / 3.0 {
            at * scale
        } else if c > base * 2.0 / 3.0 {
            screen - (base - at) * scale
        } else {
            (screen - base * scale) / 2.0 + at * scale
        }
        .max(0.0)
        .min((screen - len * scale).max(0.0))
    };
    Pos2::new(
        area.left() + side(centre[0], base[0], x, w, area.width()),
        area.top() + side(centre[1], base[1], y, h, area.height()),
    )
}

impl Painter<'_, '_> {
    /// A control's tooltip: a line of a name file, or its text with its bindings read.
    fn tip_text(&self, control: &Control, data: &dyn GuiData) -> Option<String> {
        let tip = expand(&control.tip, data);
        match super::tip_line(&tip) {
            Some((file, key)) => self.names.get(file).and_then(|t| t.get(key)).cloned(),
            None => Some(tip).filter(|t| !t.is_empty()),
        }
    }
}

struct Painter<'a, 'u> {
    ui: &'u mut egui::Ui,
    textures: &'a mut dyn Textures,
    skin: &'a Skin,
    names: &'a std::collections::HashMap<String, std::collections::HashMap<String, String>>,
    place: Place,
    /// Controls answer the pointer.
    live: bool,
}

impl Painter<'_, '_> {
    fn screen(&self, x: f32, y: f32) -> Pos2 {
        self.place.screen(x, y)
    }

    /// `frame` of a skin section's frames (its first when it has fewer): texture and size.
    fn frame(&mut self, frames: &[String], frame: usize) -> Option<(TextureId, Vec2)> {
        let path = frames.get(frame).or(frames.first())?;
        self.textures.image(self.ui.ctx(), path)
    }

    /// Draws `part` (pixels of the image, its whole when `None`) at layout point `at`, sized
    /// `size` (layout pixels; the part's own size when `None`).
    fn draw(
        &mut self,
        texture: (TextureId, Vec2),
        at: Pos2,
        part: Option<Rect>,
        size: Option<Vec2>,
    ) {
        let (id, full) = texture;
        let part = part.unwrap_or(Rect::from_min_size(Pos2::ZERO, full));
        let size = size.unwrap_or(part.size());
        let uv = Rect::from_min_max(
            Pos2::new(part.min.x / full.x, part.min.y / full.y),
            Pos2::new(part.max.x / full.x, part.max.y / full.y),
        );
        let min = self.screen(at.x, at.y);
        let rect = Rect::from_min_size(min, size * self.place.scale);
        self.ui.painter().image(id, rect, uv, Color32::WHITE);
    }

    /// A dialog's or image's picture: its frame (or a piece of it) at `at`, sized `size`
    /// when given. Its size in layout pixels, when it has a picture.
    fn picture(
        &mut self,
        control: &Control,
        section: &str,
        at: Pos2,
        size: Option<Vec2>,
    ) -> Option<Vec2> {
        if section.is_empty() && control.file.is_empty() {
            return None;
        }
        let frames = control.frames(self.skin, section);
        let frame = match control.animate {
            0 => control.frame,
            ms => {
                let at = self.ui.input(|i| i.time) * 1000.0 / f64::from(ms);
                at as usize % frames.len().max(1)
            }
        };
        let texture = self.frame(frames, frame)?;
        let part = control.src.map(|[l, t, r, b]| {
            Rect::from_min_max(Pos2::new(l as f32, t as f32), Pos2::new(r as f32, b as f32))
        });
        let drawn = size.or(part.map(|p| p.size())).unwrap_or(texture.1);
        self.draw(texture, at, part, Some(drawn));
        Some(drawn)
    }

    /// An image: its skin section (named by its binding when it has one), clickable when
    /// it has an action, framed by the skin's focus picture when its focus binding is set.
    fn image(
        &mut self,
        control: &Control,
        at: Pos2,
        size: Vec2,
        data: &dyn GuiData,
        events: &mut Vec<GuiEvent>,
    ) {
        // A number shown as piles (CMyImage::ShowMoney): a column each unit, a picture each.
        if !control.piles.is_empty() {
            let mut left = data.number(&control.value).unwrap_or(0.0).max(0.0) as u64;
            let mut column = 0.0;
            for (section, unit) in &control.piles {
                let count = left / unit.max(&1);
                left -= count * unit;
                let Some(picture) = self.frame(self.skin.frames(section), 0) else {
                    continue;
                };
                if count == 0 {
                    continue;
                }
                for line in 0..count {
                    let y = line as f32 * 8.0;
                    if y + 16.0 > size.y.max(16.0) {
                        break;
                    }
                    self.draw(picture, at + Vec2::new(column * 16.0, y), None, None);
                }
                column += 1.0;
            }
            if self.live && !control.action.is_empty() {
                let rect = Rect::from_min_size(self.screen(at.x, at.y), size * self.place.scale);
                let id = (self.ui.scope_id()).with(("gui piles", control.parent, control.id));
                if self.ui.interact(rect, id, egui::Sense::click()).clicked() {
                    clicked(control, events);
                }
            }
            return;
        }
        // A bound picture: its section is `ani` followed by what the binding reads (`Face`
        // and a face number).
        let section = match control.value.is_empty() {
            true => control.ani.clone(),
            false => match data.text(&control.value) {
                Some(value) => format!("{}{value}", control.ani),
                None => return,
            },
        };
        // The frame a binding reads.
        let bound;
        let control = match control.frame_by.is_empty() {
            true => control,
            false => {
                let frame = data.number(&control.frame_by).unwrap_or(0.0).max(0.0) as usize;
                bound = Control {
                    frame,
                    ..control.clone()
                };
                &bound
            }
        };
        // A bound picture fills the control's place (as the login scales server art).
        let sized = (!control.value.is_empty() && size.x > 0.0 && size.y > 0.0).then_some(size);
        let Some(drawn) = self.picture(control, &section, at, sized) else {
            return;
        };
        if data.number(&control.focus).is_some_and(|v| v != 0.0)
            && let Some(focus) = self.frame(self.skin.frames(FOCUS), 0)
        {
            self.draw(focus, at, Some(FOCUS_PART), Some(drawn));
        }
        if self.live && !control.action.is_empty() {
            let rect = Rect::from_min_size(self.screen(at.x, at.y), drawn * self.place.scale);
            let id = (self.ui.scope_id()).with(("gui image", control.parent, control.id));
            if self.ui.interact(rect, id, egui::Sense::click()).clicked() {
                clicked(control, events);
            }
        }
    }

    fn control(
        &mut self,
        control: &Control,
        at: GuiRect,
        data: &dyn GuiData,
        events: &mut Vec<GuiEvent>,
    ) {
        // Moved down by its binding (`dy=key*step`).
        let dy = match control.dy.split_once('*') {
            Some((key, step)) => {
                data.number(key).unwrap_or(0.0) as f32 * step.trim().parse::<f32>().unwrap_or(1.0)
            }
            None => data.number(&control.dy).unwrap_or(0.0) as f32,
        };
        let origin = Pos2::new(at.x as f32, at.y as f32 + dy);
        let size = Vec2::new(at.w.max(0) as f32, at.h.max(0) as f32);
        let rect = Rect::from_min_size(self.screen(origin.x, origin.y), size * self.place.scale);
        if let Some([r, g, b]) = control.back {
            (self.ui.painter()).rect_filled(rect, 0.0, Color32::from_rgb(r, g, b));
        }
        match control.kind {
            ControlKind::Dialog => {
                self.picture(control, &control.ani, origin, None);
            }
            ControlKind::Image => self.image(control, origin, size, data, events),
            ControlKind::Button | ControlKind::Check => {
                let frames = control.frames(self.skin, &control.ani);
                let id = self
                    .ui
                    .scope_id()
                    .with(("gui control", control.parent, control.id));
                let sense = if self.live {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                };
                // Without a size it is as big as its picture.
                let rect = match self.frame(frames, 0) {
                    Some((_, picture)) if at.w <= 0 || at.h <= 0 => {
                        Rect::from_min_size(rect.min, picture * self.place.scale)
                    }
                    _ => rect,
                };
                let disabled = !control.disabled.is_empty() && expression(&control.disabled, data);
                let latched = !control.latched.is_empty() && expression(&control.latched, data);
                let sense = if disabled || latched {
                    egui::Sense::hover()
                } else {
                    sense
                };
                let response = self.ui.interact(rect, id, sense);
                let chosen = !control.chosen.is_empty() && expression(&control.chosen, data);
                let frame = if disabled {
                    2
                } else if latched {
                    1
                } else if chosen {
                    2
                } else if control.kind == ControlKind::Check {
                    usize::from(expression(&control.value, data))
                } else if response.is_pointer_button_down_on() {
                    1
                } else if self.live && response.hovered() && frames.len() > 2 {
                    2
                } else {
                    0
                };
                if let Some(texture) = self.frame(frames, frame) {
                    self.draw(texture, origin, None, None);
                }
                let response = match self.tip_text(control, data) {
                    Some(tip) if self.live => response.on_hover_text(tip),
                    _ => response,
                };
                if response.clicked() && !control.action.is_empty() {
                    clicked(control, events);
                }
            }
            ControlKind::Progress => {
                let frames = self.skin.frames(&control.ani);
                let value = data.number(&control.value).unwrap_or(0.0);
                let max = (control.max.parse().ok())
                    .or_else(|| data.number(&control.max))
                    .unwrap_or(0.0);
                let f = if max > 0.0 {
                    (value / max).clamp(0.0, 1.0) as f32
                } else {
                    0.0
                };
                let Some(texture) = self.frame(frames, control.frame) else {
                    return;
                };
                // Its image area, else the control's size, else the image's.
                let full = match control.area {
                    Some([w, h]) => Vec2::new(w as f32, h as f32),
                    None => Vec2::new(
                        if at.w > 0 { size.x } else { texture.1.x },
                        if at.h > 0 { size.y } else { texture.1.y },
                    ),
                };
                let (offset, shown) = match control.fill {
                    Fill::Up => (
                        Vec2::new(0.0, full.y * (1.0 - f)),
                        Vec2::new(full.x, full.y * f),
                    ),
                    Fill::Right => (Vec2::ZERO, Vec2::new(full.x * f, full.y)),
                    Fill::Left => (
                        Vec2::new(full.x * (1.0 - f), 0.0),
                        Vec2::new(full.x * f, full.y),
                    ),
                };
                if shown.x >= 0.5 && shown.y >= 0.5 {
                    let part = Rect::from_min_size(offset.to_pos2(), shown);
                    self.draw(texture, origin + offset, Some(part), Some(shown));
                }
                // Its tooltip over the whole meter (an orb's `current/most`).
                if let Some(tip) = self.tip_text(control, data).filter(|_| self.live) {
                    let area = Rect::from_min_size(
                        self.screen(origin.x, origin.y),
                        full * self.place.scale,
                    );
                    let id = (self.ui.scope_id()).with(("gui tip", control.parent, control.id));
                    self.ui
                        .interact(area, id, egui::Sense::hover())
                        .on_hover_text(tip);
                }
            }
            ControlKind::Text => {
                let names = self.names.get(&control.names);
                let mut text = expand(&named(&control.text, names), data);
                // Shown as its name when the control names a file of them (the value first
                // turned into the file's key by the control's map).
                let key = control.map_value(&text);
                if let Some(name) = names.and_then(|t| t.get(&key)) {
                    text = name.clone();
                }
                if !control.fill_in.is_empty() {
                    // Each blank in turn (`fill_in={a}||{b}`), then printf's `%%`.
                    for part in control.fill_in.split("||") {
                        text = fill_first(&text, &expand(part, data));
                    }
                    text = text.replace("%%", "%");
                }
                if control.commas {
                    text = with_commas(&text);
                }
                let [r, g, b] = control.color;
                let font = crate::fonts::font(control.size * self.place.scale);
                // Wrapped within its width when it asks to be (`wrap=1`: an NPC's words); TQ's
                // statics otherwise run on past their often tiny boxes.
                let wrap = if control.wrap && rect.width() > 1.0 {
                    rect.width()
                } else {
                    f32::INFINITY
                };
                let edged = control.edge.map(|[r, g, b]| {
                    (self.ui.painter()).layout(
                        text.clone(),
                        font.clone(),
                        Color32::from_rgb(r, g, b),
                        wrap,
                    )
                });
                let galley =
                    (self.ui.painter()).layout(text, font, Color32::from_rgb(r, g, b), wrap);
                let left = match control.align {
                    super::Align::Left => rect.left(),
                    super::Align::Center => rect.center().x - galley.size().x / 2.0,
                    super::Align::Right => rect.right() - galley.size().x,
                };
                let at = Pos2::new(left, rect.top());
                if let Some(edge) = edged {
                    for d in [
                        Vec2::new(-1.0, 0.0),
                        Vec2::new(1.0, 0.0),
                        Vec2::new(0.0, -1.0),
                        Vec2::new(0.0, 1.0),
                    ] {
                        (self.ui.painter()).galley(at + d, edge.clone(), Color32::WHITE);
                    }
                }
                (self.ui.painter()).galley(at, galley, Color32::WHITE);
            }
            ControlKind::Slider => {
                let max: f32 = (control.max.parse().ok())
                    .or_else(|| data.number(&control.max).map(|m| m as f32))
                    .unwrap_or(100.0)
                    .max(1.0);
                let value = data.number(&control.value).unwrap_or(0.0) as f32;
                let travel = control.travel as f32 * self.place.scale;
                let frames = self.skin.frames(&control.ani);
                let Some(thumb) = self.frame(frames, 0) else {
                    return;
                };
                let along = (value / max).clamp(0.0, 1.0) * travel;
                let thumb_size = thumb.1 * self.place.scale;
                let track = match control.vertical {
                    true => {
                        Rect::from_min_max(rect.min - Vec2::new(0.0, travel), rect.min + thumb_size)
                    }
                    false => Rect::from_min_size(
                        rect.min,
                        Vec2::new(travel + thumb_size.x, thumb_size.y),
                    ),
                };
                let id = (self.ui.scope_id()).with(("gui slider", control.parent, control.id));
                let sense = if self.live {
                    egui::Sense::click_and_drag()
                } else {
                    egui::Sense::hover()
                };
                let response = self.ui.interact(track, id, sense);
                if let Some(at) = response.interact_pointer_pos()
                    && (response.dragged() || response.clicked())
                    && travel > 0.0
                {
                    let moved = match control.vertical {
                        true => rect.top() + thumb_size.y / 2.0 - at.y,
                        false => at.x - rect.left() - thumb_size.x / 2.0,
                    };
                    events.push(GuiEvent::Slid {
                        field: control.value.clone(),
                        value: (moved.clamp(0.0, travel) / travel * max).round() as u32,
                    });
                }
                let shift = along / self.place.scale;
                let at = match control.vertical {
                    true => origin - Vec2::new(0.0, shift),
                    false => origin + Vec2::new(shift, 0.0),
                };
                self.draw(thumb, at, None, None);
            }
            ControlKind::Model => {
                if let Some(texture) = data.image(&control.value) {
                    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                    self.ui.painter().image(texture, rect, uv, Color32::WHITE);
                }
            }
            ControlKind::Edit => {
                // Its box (TQ's edit background: Edit230).
                if !control.ani.is_empty() {
                    self.picture(control, &control.ani, origin, Some(size));
                }
                let mut text = data.text(&control.value).unwrap_or_default();
                let marked = !control.mark.is_empty()
                    && data.number(&control.mark).is_some_and(|v| v != 0.0);
                let [r, g, b] = if marked {
                    control.mark_color
                } else {
                    control.color
                };
                let font = crate::fonts::font(control.size * self.place.scale);
                let bound = (!control.limit_of.is_empty())
                    .then(|| data.number(&control.limit_of))
                    .flatten()
                    .map(|n| n.max(0.0) as usize);
                let limit = match bound.unwrap_or(control.limit) {
                    0 => usize::MAX,
                    n => n,
                };
                // A `wrap` edit takes several lines (TQ's SetMultiLine).
                let edit = match control.wrap {
                    true => egui::TextEdit::multiline(&mut text).desired_width(rect.width()),
                    false => egui::TextEdit::singleline(&mut text),
                };
                let edit = edit
                    .id_salt(("gui edit", control.parent, control.id))
                    .char_limit(limit)
                    .password(control.password)
                    .frame(egui::Frame::NONE)
                    .font(font)
                    .text_color(Color32::from_rgb(r, g, b))
                    .interactive(self.live);
                let response = self.ui.put(rect, edit);
                if self.live && data.focus() == Some(control.value.as_str()) {
                    response.request_focus();
                }
                if response.changed() {
                    let text = match control.digits {
                        true => text.chars().filter(char::is_ascii_digit).collect(),
                        false => text,
                    };
                    events.push(GuiEvent::Edited {
                        field: control.value.clone(),
                        text,
                    });
                }
                if self.live && response.has_focus() {
                    edit_keys(self.ui, &control.value, events);
                }
                // Enter in an edit with an action runs it (the chat line sends).
                let entered = self.ui.input(|i| i.key_pressed(egui::Key::Enter));
                let one_line = !control.wrap;
                if self.live
                    && one_line
                    && !control.action.is_empty()
                    && response.lost_focus()
                    && entered
                {
                    clicked(control, events);
                }
            }
            ControlKind::List => {
                // A document's paragraphs (the page its view group holds), else the binding's rows.
                let page = data.view_number(&control.page);
                let (rows, picked) = match control.pages.is_empty() {
                    true => data.rows(&control.value),
                    false => {
                        let doc =
                            (self.names.get(&control.pages)).and_then(|t| t.get(&page.to_string()));
                        (
                            doc.map_or_else(Vec::new, |d| d.lines().map(str::to_owned).collect()),
                            None,
                        )
                    }
                };
                let scale = self.place.scale;
                let font = crate::fonts::font(control.size * scale);
                let laid = list_rows(self.ui.ctx(), control, &rows, size.x);
                let tips = data.rows(&control.tips).0;
                // The foot under the last row.
                if let Some(last) = laid.last()
                    && let Some(foot) = self.frame(self.skin.frames(&control.foot), 0)
                {
                    self.draw(foot, origin + Vec2::new(0.0, last.bottom()), None, None);
                }
                // Rows past its foot are reached with the wheel; a log shows its end, the wheel
                // going back as far as its first row.
                let total = laid.last().map_or(0.0, |l| l.bottom());
                let over = (total - size.y).max(0.0);
                let id = (self.ui.scope_id()).with(("gui list", control.parent, control.id, page));
                let mut moved: f32 = self.ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
                if self.live && over > 0.0 && self.ui.rect_contains_pointer(rect) {
                    let wheel = self.ui.input(|i| i.smooth_scroll_delta.y) / scale;
                    moved += if control.tail { wheel } else { -wheel };
                }
                moved = moved.clamp(0.0, over);
                self.ui.data_mut(|d| d.insert_temp(id, moved));
                let lift = if control.tail { over - moved } else { moved };
                for (index, (row, at)) in rows.iter().zip(laid).enumerate() {
                    let at = at.translate(Vec2::new(0.0, -lift));
                    let line =
                        Rect::from_min_size(rect.min + at.min.to_vec2() * scale, at.size() * scale);
                    if line.top() < rect.top() - 0.5 {
                        continue;
                    }
                    if line.bottom() > rect.bottom() + 0.5 {
                        break;
                    }
                    let id =
                        self.ui
                            .scope_id()
                            .with(("gui row", control.parent, control.id, index));
                    let sense = if self.live {
                        egui::Sense::click()
                    } else {
                        egui::Sense::hover()
                    };
                    let response = self.ui.interact(line, id, sense);
                    let lit = self.live && response.hovered();
                    // Its row picture: lit under the pointer, pressed while held.
                    let frames = self.skin.frames(&control.row);
                    let frame = match (lit, response.is_pointer_button_down_on()) {
                        (_, true) => 2,
                        (true, false) => 1,
                        _ => 0,
                    };
                    if let Some(picture) = self.frame(frames, frame) {
                        let corner = origin + at.min.to_vec2();
                        self.draw(picture, corner, None, None);
                    }
                    let response = match tips.get(index).filter(|t| !t.is_empty()) {
                        Some(tip) => response.on_hover_text(tip.as_str()),
                        None => response,
                    };
                    if picked == Some(index) {
                        self.ui.painter().rect_filled(
                            line,
                            2.0,
                            Color32::from_rgba_unmultiplied(255, 220, 120, 60),
                        );
                    } else if lit && control.hover.is_none() && control.row.is_empty() {
                        self.ui
                            .painter()
                            .rect_filled(line, 2.0, Color32::from_white_alpha(20));
                    }
                    let [r, g, b] = match control.hover {
                        Some(color) if lit => color,
                        _ => control.color,
                    };
                    let wrap = match control.wrap {
                        true => line.width() - 4.0 * scale,
                        false => f32::INFINITY,
                    };
                    let galley = (self.ui.painter()).layout(
                        row.clone(),
                        font.clone(),
                        Color32::from_rgb(r, g, b),
                        wrap,
                    );
                    let indent = control.indent.unwrap_or(4) as f32;
                    let at = Pos2::new(
                        line.left() + indent * scale,
                        line.center().y - galley.size().y / 2.0,
                    );
                    // Kept inside its row (a long name is cut, as TQ's list boxes do).
                    (self.ui.painter().with_clip_rect(line)).galley(at, galley, Color32::WHITE);
                    if response.clicked() {
                        events.push(GuiEvent::Item {
                            list: control.value.clone(),
                            index,
                        });
                        // A list with an action runs it for the row clicked too.
                        if !control.action.is_empty() {
                            clicked(control, events);
                        }
                    }
                    if response.double_clicked() && !control.double.is_empty() {
                        events.push(GuiEvent::Action(control.double.clone()));
                    }
                }
            }
            ControlKind::Grid => {
                let items = data.items(&control.value);
                let looks = data.item_looks(&control.value);
                // A paged grid shows its list from the page's first item.
                let first = match control.offset.as_str() {
                    "" => 0,
                    group => data.view_number(group),
                };
                let cell = Vec2::new(
                    rect.width() / control.cols as f32,
                    rect.height() / control.rows as f32,
                );
                for cell_index in 0..(control.cols * control.rows) as usize {
                    let index = first + cell_index;
                    let (col, row) = (
                        cell_index as u32 % control.cols,
                        cell_index as u32 / control.cols,
                    );
                    let slot = Rect::from_min_size(
                        rect.min + Vec2::new(col as f32 * cell.x, row as f32 * cell.y),
                        cell,
                    );
                    let id =
                        self.ui
                            .scope_id()
                            .with(("gui slot", control.parent, control.id, index));
                    let sense = if self.live {
                        egui::Sense::click()
                    } else {
                        egui::Sense::hover()
                    };
                    let response = self.ui.interact(slot, id, sense);
                    let list = control.value.clone();
                    let holding = data.holding();
                    // A `pick` grid: the left button picks up or puts down, the right uses.
                    if control.pick && response.clicked() && holding {
                        events.push(GuiEvent::Put { list, index });
                        continue;
                    }
                    let Some(&kind) = items.get(index) else {
                        continue;
                    };
                    let look = looks.get(index).copied().unwrap_or(0);
                    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                    // Its glow under it (TQ's quality lights).
                    let time = self.ui.input(|i| i.time);
                    if let Some(glow) = self.textures.item_glow(self.ui.ctx(), kind, time) {
                        (self.ui.painter()).image(glow, slot.shrink(1.0), uv, Color32::WHITE);
                    }
                    if let Some(texture) = self.textures.item(self.ui.ctx(), kind, look) {
                        self.ui
                            .painter()
                            .image(texture, slot.shrink(1.0), uv, Color32::WHITE);
                    }
                    let used = match control.pick {
                        true => response.secondary_clicked(),
                        false => response.clicked(),
                    };
                    if control.pick && response.clicked() {
                        events.push(GuiEvent::Pick {
                            list: list.clone(),
                            index,
                        });
                    } else if used {
                        events.push(GuiEvent::Item {
                            list: list.clone(),
                            index,
                        });
                    }
                    // What it is, under the pointer (not while one is held).
                    if self.live
                        && !holding
                        && let Some(tip) = data.item_tip(&list, index)
                    {
                        response.on_hover_text(tip);
                    }
                }
            }
        }
    }
}

/// Where a list's rows sit, from its corner in layout pixels: row `i` in column `i % cols`,
/// `step` apart (else `width` shared), `cell` wide (else the step); each line of columns as
/// tall as its tallest row, a row as tall as its text wrapped in the cell (`wrap`). At most
/// `limit` rows.
fn list_rows(ctx: &egui::Context, control: &Control, rows: &[String], width: f32) -> Vec<Rect> {
    let cols = control.cols.max(1) as usize;
    let step = match control.step {
        0 => width / cols as f32,
        s => s as f32,
    };
    let cell = match control.cell {
        0 => step,
        c => c as f32,
    };
    let line = match control.line {
        0 => control.size + 6.0,
        tall => tall as f32,
    };
    let shown = match control.limit {
        0 => rows.len(),
        n => rows.len().min(n),
    };
    let heights: Vec<f32> = (rows[..shown].iter())
        .map(|row| match control.wrap {
            true => {
                let font = crate::fonts::font(control.size);
                let galley =
                    ctx.fonts_mut(|f| f.layout(row.clone(), font, Color32::WHITE, cell - 4.0));
                (galley.size().y + 6.0).max(line)
            }
            false => line,
        })
        .collect();
    let mut out = Vec::new();
    let mut top = 0.0;
    for chunk in heights.chunks(cols) {
        let tall = chunk.iter().copied().fold(0.0, f32::max);
        for col in 0..chunk.len() {
            let corner = Pos2::new(col as f32 * step, top);
            out.push(Rect::from_min_size(corner, Vec2::new(cell, tall)));
        }
        top += tall;
    }
    out
}

/// `text` with commas between the thousands of each run of digits ("12345" "12,345").
fn with_commas(text: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let end = (i..chars.len())
            .find(|&j| !chars[j].is_ascii_digit())
            .unwrap_or(chars.len());
        for (k, c) in chars[i..end].iter().enumerate() {
            if k > 0 && (end - i - k) % 3 == 0 {
                out.push(',');
            }
            out.push(*c);
        }
        i = end;
    }
    out
}

/// `text` with each `{=key}` turned into that key's name in `names` (a string of the game's
/// own, `{=10001}` from TQ's StrRes.ini).
fn named(text: &str, names: Option<&std::collections::HashMap<String, String>>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find("{=") {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let key = &rest[open + 2..open + close];
        // TQ's strings break lines with a written `\n`.
        let found = names.and_then(|n| n.get(key)).map_or("", String::as_str);
        out.push_str(&found.replace("\\n", "\n"));
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

/// `text` with each `{binding}` replaced by what it reads.
pub(super) fn expand(text: &str, data: &dyn GuiData) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let key = &rest[open + 1..open + close];
        out.push_str(&data.text(key).unwrap_or_default());
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod when_tests {
    use super::*;

    struct Hero;
    impl GuiData for Hero {
        fn number(&self, key: &str) -> Option<f64> {
            match key {
                "level" => Some(70.0),
                "cps" => Some(0.0),
                _ => None,
            }
        }
    }

    fn holds(when: &str) -> bool {
        let control = Control {
            when: when.into(),
            ..Control::parse(
                &super::super::ini::parse(
                    b"[1-2]
type=text",
                )[0],
            )
            .unwrap()
        };
        set(&control, &Hero)
    }

    #[test]
    fn digits_take_commas() {
        assert_eq!(with_commas("1234567 and 12"), "1,234,567 and 12");
        assert_eq!(with_commas("999"), "999");
    }

    #[test]
    fn fill_first_fills_printf_blanks_in_turn() {
        let text = "To be %s, you need to donate %s silver at least.";
        let once = fill_first(text, "Prince");
        assert_eq!(
            fill_first(&once, "300"),
            "To be Prince, you need to donate 300 silver at least."
        );
        assert_eq!(
            fill_first("EXP = %0.2f EXP Ball", "1.5"),
            "EXP = 1.5 EXP Ball"
        );
    }

    #[test]
    fn terms_compare_bindings_with_numbers() {
        assert!(holds("level>=70"));
        assert!(!holds("level>70"));
        assert!(holds("level>69&cps=0"));
        assert!(holds("cps>0|level!=1"));
        assert!(!holds("cps"));
        assert!(holds("!cps"));
        assert!(holds("level>=cps"));
    }
}
