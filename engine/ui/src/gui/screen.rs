//! Drawing a [`Gui`] over the game and turning clicks into what the player asked for.
//!
//! The layout is made for one screen size (`[0-0]`); it is scaled to fit the game area, and
//! each dialog keeps to the side of the screen it was placed on (bottom, top, a corner).

use super::{Control, ControlKind, Fill, Gui, GuiRect, Skin};
use crate::Icons;
use egui::{Color32, Pos2, Rect, TextureId, Vec2};

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
}

/// What the player did on the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuiEvent {
    /// A control with `action=` was clicked.
    Action(String),
    /// Slot `index` of grid list `list` was clicked.
    Item { list: String, index: usize },
}

/// How layout pixels sit on the screen.
#[derive(Clone, Copy)]
struct Place {
    scale: f32,
    /// Screen point of the dialog's layout origin.
    origin: Pos2,
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
        let [bw, bh] = self.layout.base().map(|v| v as f32);
        let scale = (area.width() / bw).min(area.height() / bh);
        let mut events = Vec::new();
        for dialog in self.dialogs().filter(|d| self.is_open(d)) {
            let r = self.rect(dialog);
            let place = Place {
                scale,
                origin: anchored(area, [bw, bh], scale, r),
            };
            let size = Vec2::new(r.w.max(0) as f32, r.h.max(0) as f32) * scale;
            egui::Area::new(egui::Id::unique(("gui dialog", dialog.id)))
                .fixed_pos(place.origin)
                .order(egui::Order::Middle)
                .show(ctx, |ui| {
                    // The dialog takes the pointer, so clicks on it don't reach the world.
                    ui.allocate_exact_size(size, egui::Sense::click());
                    let mut painter = Painter {
                        ui,
                        textures: &mut *textures,
                        skin: &self.skin,
                        place,
                    };
                    painter.picture(dialog, Pos2::ZERO);
                    let controls = (self.controls.iter()).filter(|c| c.parent == dialog.id);
                    for control in controls {
                        let at = self.rect(control);
                        painter.control(control, at, data, &mut events);
                    }
                });
        }
        events
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
}

impl Painter<'_, '_> {
    fn screen(&self, x: f32, y: f32) -> Pos2 {
        self.place.origin + Vec2::new(x, y) * self.place.scale
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

    /// A dialog's or image's picture: its frame (or a piece of it) at `at`.
    fn picture(&mut self, control: &Control, at: Pos2) {
        if control.ani.is_empty() {
            return;
        }
        let frames = self.skin.frames(&control.ani);
        let Some(texture) = self.frame(frames, control.frame) else {
            return;
        };
        let part = control.src.map(|[l, t, r, b]| {
            Rect::from_min_max(Pos2::new(l as f32, t as f32), Pos2::new(r as f32, b as f32))
        });
        self.draw(texture, at, part, None);
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
            ControlKind::Dialog | ControlKind::Image => self.picture(control, origin),
            ControlKind::Button | ControlKind::Check => {
                let frames = self.skin.frames(&control.ani);
                let id = self
                    .ui
                    .scope_id()
                    .with(("gui control", control.parent, control.id));
                let response = self.ui.interact(rect, id, egui::Sense::click());
                let frame = if control.kind == ControlKind::Check {
                    usize::from(data.number(&control.value).is_some_and(|v| v != 0.0))
                } else if response.is_pointer_button_down_on() {
                    1
                } else if response.hovered() && frames.len() > 2 {
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
                self.ui.painter().text(
                    rect.min,
                    egui::Align2::LEFT_TOP,
                    text,
                    crate::fonts::font(control.size * self.place.scale),
                    Color32::from_rgb(r, g, b),
                );
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
                    let response = self.ui.interact(slot, id, egui::Sense::click());
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
