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

/// What bindings (`hero.life`) read.
pub trait GuiData {
    fn number(&self, key: &str) -> Option<f64>;
    fn text(&self, key: &str) -> Option<String> {
        self.number(key).map(|n| format!("{n}"))
    }
    /// Item kinds of a list (a bag), in slot order.
    fn items(&self, _key: &str) -> Vec<u32> {
        Vec::new()
    }
    /// A picture the host drew for `key` (a 3D look), as a texture.
    fn image(&self, _key: &str) -> Option<TextureId> {
        None
    }
    /// The rows of a list control and which one is picked.
    fn rows(&self, _key: &str) -> (Vec<String>, Option<usize>) {
        (Vec::new(), None)
    }
}

/// What the player did on the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuiEvent {
    /// A control with `action=` was clicked.
    Action(String),
    /// Slot `index` of grid list `list` (or row `index` of a list control) was clicked.
    Item { list: String, index: usize },
    /// Edit `field` now reads `text`.
    Edited { field: String, text: String },
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
            let set = !d.when.is_empty() && data.number(&d.when).is_some_and(|v| v != 0.0);
            d.screen == self.screen && set
        };
        for dialog in self.dialogs().filter(|d| self.is_open(d) || shown(d)) {
            let place = self.place(dialog, area);
            let r = self.rect(dialog);
            let size = Vec2::new(r.w.max(0) as f32, r.h.max(0) as f32) * place.scale;
            egui::Area::new(egui::Id::unique(("gui dialog", dialog.id)))
                .fixed_pos(place.origin)
                .order(egui::Order::Middle)
                .show(ctx, |ui| {
                    // The dialog takes the pointer, so clicks on it don't reach the world.
                    ui.allocate_exact_size(size, egui::Sense::click());
                    events.extend(self.paint_dialog(ui, dialog, place, data, textures, true));
                });
        }
        events
    }

    /// Where `dialog` sits when the UI is drawn over `area`.
    pub fn place(&self, dialog: &Control, area: Rect) -> Place {
        let [bw, bh] = self.layout.base().map(|v| v as f32);
        let scale = (area.width() / bw).min(area.height() / bh);
        Place {
            scale,
            origin: anchored(area, [bw, bh], scale, self.rect(dialog)),
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
            place,
            live,
        };
        painter.picture(dialog, &dialog.ani, Pos2::ZERO, None);
        for control in (self.controls.iter()).filter(|c| c.parent == dialog.id) {
            if control.kind != ControlKind::Dialog {
                painter.control(control, self.rect(control), data, &mut events);
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
            let frames = self.skin.frames(&control.ani);
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

struct Painter<'a, 'u> {
    ui: &'u mut egui::Ui,
    textures: &'a mut dyn Textures,
    skin: &'a Skin,
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
        if section.is_empty() {
            return None;
        }
        let frames = self.skin.frames(section);
        let texture = self.frame(frames, control.frame)?;
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
        let section = match control.value.is_empty() {
            true => control.ani.clone(),
            false => data.text(&control.value).unwrap_or_default(),
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
                events.push(GuiEvent::Action(control.action.clone()));
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
        let origin = Pos2::new(at.x as f32, at.y as f32);
        let size = Vec2::new(at.w.max(0) as f32, at.h.max(0) as f32);
        let rect = Rect::from_min_size(self.screen(origin.x, origin.y), size * self.place.scale);
        match control.kind {
            ControlKind::Dialog => {
                self.picture(control, &control.ani, origin, None);
            }
            ControlKind::Image => self.image(control, origin, size, data, events),
            ControlKind::Button | ControlKind::Check => {
                let frames = self.skin.frames(&control.ani);
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
                let response = self.ui.interact(rect, id, sense);
                let frame = if control.kind == ControlKind::Check {
                    usize::from(data.number(&control.value).is_some_and(|v| v != 0.0))
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
                if response.clicked() && !control.action.is_empty() {
                    events.push(GuiEvent::Action(control.action.clone()));
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
                // The control's size, else the image's.
                let full = Vec2::new(
                    if at.w > 0 { size.x } else { texture.1.x },
                    if at.h > 0 { size.y } else { texture.1.y },
                );
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
            }
            ControlKind::Text => {
                let text = expand(&control.text, data);
                let [r, g, b] = control.color;
                let font = crate::fonts::font(control.size * self.place.scale);
                // Wrapped within its width when it has one (an NPC's words).
                let wrap = if rect.width() > 1.0 {
                    rect.width()
                } else {
                    f32::INFINITY
                };
                let galley =
                    (self.ui.painter()).layout(text, font, Color32::from_rgb(r, g, b), wrap);
                self.ui.painter().galley(rect.min, galley, Color32::WHITE);
            }
            ControlKind::Model => {
                if let Some(texture) = data.image(&control.value) {
                    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                    self.ui.painter().image(texture, rect, uv, Color32::WHITE);
                }
            }
            ControlKind::Edit => {
                let mut text = data.text(&control.value).unwrap_or_default();
                let [r, g, b] = control.color;
                let font = crate::fonts::font(control.size * self.place.scale);
                let edit = egui::TextEdit::singleline(&mut text)
                    .id_salt(("gui edit", control.parent, control.id))
                    .password(control.password)
                    .frame(egui::Frame::NONE)
                    .font(font)
                    .text_color(Color32::from_rgb(r, g, b))
                    .interactive(self.live);
                if self.ui.put(rect, edit).changed() {
                    events.push(GuiEvent::Edited {
                        field: control.value.clone(),
                        text,
                    });
                }
            }
            ControlKind::List => {
                let (rows, picked) = data.rows(&control.value);
                let height = (control.size + 6.0) * self.place.scale;
                let font = crate::fonts::font(control.size * self.place.scale);
                let [r, g, b] = control.color;
                // Rows fill `cols` columns, left to right.
                let cols = control.cols.max(1) as usize;
                let width = rect.width() / cols as f32;
                for (index, row) in rows.iter().enumerate() {
                    let top = rect.top() + (index / cols) as f32 * height;
                    if top + height > rect.bottom() + 0.5 {
                        break;
                    }
                    let left = rect.left() + (index % cols) as f32 * width;
                    let line = Rect::from_min_size(Pos2::new(left, top), Vec2::new(width, height));
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
                    if picked == Some(index) {
                        self.ui.painter().rect_filled(
                            line,
                            2.0,
                            Color32::from_rgba_unmultiplied(255, 220, 120, 60),
                        );
                    } else if self.live && response.hovered() {
                        self.ui
                            .painter()
                            .rect_filled(line, 2.0, Color32::from_white_alpha(20));
                    }
                    self.ui.painter().text(
                        line.left_center() + Vec2::new(4.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        row,
                        font.clone(),
                        Color32::from_rgb(r, g, b),
                    );
                    if response.clicked() {
                        events.push(GuiEvent::Item {
                            list: control.value.clone(),
                            index,
                        });
                    }
                }
            }
            ControlKind::Grid => {
                let items = data.items(&control.value);
                let cell = Vec2::new(
                    rect.width() / control.cols as f32,
                    rect.height() / control.rows as f32,
                );
                for index in 0..(control.cols * control.rows) as usize {
                    let (col, row) = (index as u32 % control.cols, index as u32 / control.cols);
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
                    let Some(&kind) = items.get(index) else {
                        continue;
                    };
                    if let Some(texture) = self.textures.item(self.ui.ctx(), kind) {
                        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                        self.ui
                            .painter()
                            .image(texture, slot.shrink(1.0), uv, Color32::WHITE);
                    }
                    if response.clicked() {
                        events.push(GuiEvent::Item {
                            list: control.value.clone(),
                            index,
                        });
                    }
                }
            }
        }
    }
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
