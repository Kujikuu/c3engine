//! Game UI from data, in TQ's own model: dialogs and their controls are placed by a layout
//! file (`GUI.ini`: `[dialog-control]` sections of `x`, `y`, `w`, `h`, relative to the dialog;
//! `[0-dialog]` on the screen, `[0-0]` the screen the layout was made for), drawn from a skin
//! (`Control.Ani`: `[Section]` with `FrameAmount` and `Frame<n>` images).
//!
//! What TQ's client says in code — each control's kind, the skin section it shows, the data it
//! shows and what a click does — is the controls file, in the same INI form:
//!
//! ```ini
//! [0-130]            ; the dialog: always open, its background is Dialog4's frame 0
//! type=dialog
//! open=always
//! [130-1018]         ; life: Progress40 filled from the bottom by hero.life / hero.life_max
//! type=progress
//! ani=Progress40
//! fill=up
//! value=hero.life
//! max=hero.life_max
//! [130-1007]         ; a button (frame 0 normal, 1 pressed, 2 hover when it has 3 frames)
//! type=button
//! ani=Button42
//! action=bag
//! ```
//!
//! A control the layout doesn't place takes `x`, `y`, `w`, `h` from the controls file, as
//! TQ's controls fall back to their dialog template. Pieces of a skin image are drawn with
//! `src=left,top,right,bottom`; controls draw in file order.

pub mod ini;
mod screen;

pub use screen::{GuiData, GuiEvent, GuiImages, Place, Textures};

use std::collections::{BTreeSet, HashMap};

/// The layout's screen when it has no `[0-0]` section.
const BASE_SIZE: [i32; 2] = [1024, 768];

/// A rectangle in layout pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GuiRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Where dialogs and controls sit (`GUI.ini`).
#[derive(Debug, Clone, Default)]
pub struct Layout {
    rects: HashMap<(u32, u32), GuiRect>,
}

impl Layout {
    pub fn parse(bytes: &[u8]) -> Self {
        let rects = (ini::parse(bytes).iter())
            .filter_map(|s| {
                let ids = ini::ids(&s.name)?;
                let rect = GuiRect {
                    x: s.int("x").unwrap_or(0),
                    y: s.int("y").unwrap_or(0),
                    w: s.int("w").unwrap_or(-1),
                    h: s.int("h").unwrap_or(-1),
                };
                Some((ids, rect))
            })
            .collect();
        Self { rects }
    }

    pub fn rect(&self, parent: u32, control: u32) -> Option<GuiRect> {
        self.rects.get(&(parent, control)).copied()
    }

    /// The screen the layout was made for.
    pub fn base(&self) -> [i32; 2] {
        match self.rect(0, 0) {
            Some(r) if r.w > 0 && r.h > 0 => [r.w, r.h],
            _ => BASE_SIZE,
        }
    }
}

/// The images controls show (`Control.Ani`): each section's frames, by section title.
#[derive(Debug, Clone, Default)]
pub struct Skin {
    sections: HashMap<String, Vec<String>>,
}

impl Skin {
    pub fn parse(bytes: &[u8]) -> Self {
        let mut sections = HashMap::new();
        for s in ini::parse(bytes) {
            let count = s.int("FrameAmount").unwrap_or(0).max(0) as usize;
            let frames = (0..count)
                .map(|i| s.get(&format!("Frame{i}")).unwrap_or_default().to_owned())
                .collect();
            // The first section of a title wins, as in the client.
            sections.entry(s.name).or_insert(frames);
        }
        Self { sections }
    }

    pub fn frames(&self, section: &str) -> &[String] {
        self.sections.get(section).map_or(&[], Vec::as_slice)
    }
}

/// What a control is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Dialog,
    Image,
    Button,
    Check,
    Progress,
    Text,
    Grid,
    /// A text field (`value` names the field; `password=1` hides what is typed).
    Edit,
    /// Rows to pick one of (`value` names the list).
    List,
    /// A 3D look drawn by the host (`value` names it: `front.look`, `front.look:1`).
    Model,
}

impl ControlKind {
    fn parse(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "dialog" => Self::Dialog,
            "image" => Self::Image,
            "button" => Self::Button,
            "check" => Self::Check,
            "progress" => Self::Progress,
            "text" => Self::Text,
            "grid" => Self::Grid,
            "edit" => Self::Edit,
            "list" => Self::List,
            "model" => Self::Model,
            _ => return None,
        })
    }
}

/// Which way a progress bar fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// From the bottom (TQ's `MY_PROGRESS_V`).
    Up,
    /// From the left (`MY_PROGRESS_HLEFT`).
    Right,
    /// From the right (`MY_PROGRESS_HRIGHT`).
    Left,
}

/// A dialog or control, as the controls file describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    /// The dialog it is in (0: a dialog on the screen).
    pub parent: u32,
    pub id: u32,
    pub kind: ControlKind,
    /// The skin section it shows.
    pub ani: String,
    pub frame: usize,
    /// The part of the image drawn: left, top, right, bottom.
    pub src: Option<[i32; 4]>,
    /// Its place when the layout has none.
    pub rect: Option<GuiRect>,
    /// A binding: what a progress bar, check or grid shows (an image: its skin section).
    pub value: String,
    /// A binding: when it is set, the skin's `ImageFocus` is drawn over the control (the
    /// picked server, as `CMyImage::ShowFocus`).
    pub focus: String,
    /// A progress bar's full value, a binding or a number.
    pub max: String,
    pub fill: Fill,
    /// Text with `{binding}`s in it.
    pub text: String,
    /// Text colour, `rrggbb`.
    pub color: [u8; 3],
    pub size: f32,
    /// What a click asks the game for.
    pub action: String,
    /// A grid's columns and rows.
    pub cols: u32,
    pub rows: u32,
    /// A dialog's opening: `always`, or empty when actions open it.
    pub open: String,
    /// The screen a dialog belongs to (`login`, `servers`, `characters`, `create`,
    /// `loading`); empty: the game's.
    pub screen: String,
    /// An edit hides what is typed.
    pub password: bool,
    /// Its own place wins over the layout's (the client's code moves it, as TQ's does for
    /// screens it lays out per resolution).
    pub fixed: bool,
}

impl Control {
    fn parse(section: &ini::Section) -> Option<Self> {
        let (parent, id) = ini::ids(&section.name)?;
        let text = |key: &str| section.get(key).unwrap_or_default().to_owned();
        let rect = section.int("x").map(|x| GuiRect {
            x,
            y: section.int("y").unwrap_or(0),
            w: section.int("w").unwrap_or(-1),
            h: section.int("h").unwrap_or(-1),
        });
        let src = section.get("src").and_then(|s| {
            let n: Vec<i32> = s.split(',').filter_map(|v| v.trim().parse().ok()).collect();
            <[i32; 4]>::try_from(n).ok()
        });
        let color = section
            .get("color")
            .and_then(|c| u32::from_str_radix(c.trim_start_matches('#'), 16).ok())
            .map_or([255, 255, 255], |c| {
                [(c >> 16) as u8, (c >> 8) as u8, c as u8]
            });
        let fill = match section.get("fill").unwrap_or("right") {
            "up" => Fill::Up,
            "left" => Fill::Left,
            _ => Fill::Right,
        };
        Some(Self {
            parent,
            id,
            kind: ControlKind::parse(section.get("type")?)?,
            ani: text("ani"),
            frame: section.int("frame").unwrap_or(0).max(0) as usize,
            src,
            rect,
            value: text("value"),
            focus: text("focus"),
            max: text("max"),
            fill,
            text: text("text"),
            color,
            size: section.int("size").unwrap_or(12).max(4) as f32,
            action: text("action"),
            cols: section.int("cols").unwrap_or(1).max(1) as u32,
            rows: section.int("rows").unwrap_or(1).max(1) as u32,
            open: text("open"),
            screen: text("screen").to_ascii_lowercase(),
            password: section.int("password").is_some_and(|p| p != 0),
            fixed: section.int("fixed").is_some_and(|f| f != 0),
        })
    }
}

/// A game UI: its layout, skin and controls, and which dialogs are open.
#[derive(Debug, Clone, Default)]
pub struct Gui {
    pub layout: Layout,
    pub skin: Skin,
    /// Dialogs and controls, in drawing order.
    pub controls: Vec<Control>,
    /// Dialogs opened by actions.
    open: BTreeSet<u32>,
    /// The screen shown (empty: the game).
    screen: String,
}

impl Gui {
    /// A UI from a layout, skin and controls file's bytes.
    pub fn new(layout: &[u8], skin: &[u8], controls: &[u8]) -> Self {
        Self {
            layout: Layout::parse(layout),
            skin: Skin::parse(skin),
            controls: (ini::parse(controls).iter())
                .filter_map(Control::parse)
                .collect(),
            open: BTreeSet::new(),
            screen: String::new(),
        }
    }

    /// Shows the dialogs of `screen` (empty: the game's).
    pub fn set_screen(&mut self, screen: &str) {
        self.screen = screen.to_ascii_lowercase();
    }

    /// Whether the UI has dialogs for `screen`.
    pub fn has_screen(&self, screen: &str) -> bool {
        self.dialogs()
            .any(|d| d.screen.eq_ignore_ascii_case(screen))
    }

    /// The dialogs on the screen, in drawing order.
    pub fn dialogs(&self) -> impl Iterator<Item = &Control> {
        (self.controls.iter()).filter(|c| c.parent == 0 && c.kind == ControlKind::Dialog)
    }

    /// Whether `dialog` shows: on the screen shown, open from the start or opened.
    pub fn is_open(&self, dialog: &Control) -> bool {
        dialog.screen == self.screen
            && (dialog.open.eq_ignore_ascii_case("always") || self.open.contains(&dialog.id))
    }

    /// Opens dialog `id`.
    pub fn open(&mut self, id: u32) {
        self.open.insert(id);
    }

    /// Shows page `id` of its dialog, hiding the dialog's other pages.
    pub fn show_page(&mut self, id: u32) {
        let Some(parent) = (self.controls.iter()).find(|c| c.id == id && c.parent != 0) else {
            return;
        };
        let parent = parent.parent;
        for sibling in self.pages(parent).map(|c| c.id).collect::<Vec<_>>() {
            self.open.remove(&sibling);
        }
        self.open.insert(id);
    }

    /// The pages of dialog `id`: dialogs placed inside it.
    pub fn pages(&self, id: u32) -> impl Iterator<Item = &Control> {
        (self.controls.iter()).filter(move |c| c.parent == id && c.kind == ControlKind::Dialog)
    }

    /// The page of dialog `id` that shows: the one opened, else its first.
    pub fn shown_page(&self, id: u32) -> Option<&Control> {
        let mut pages = self.pages(id).peekable();
        let first = pages.peek().copied();
        self.pages(id).find(|p| self.open.contains(&p.id)).or(first)
    }

    /// Opens dialog `id`, or closes it when it is open.
    pub fn toggle(&mut self, id: u32) {
        if !self.open.remove(&id) {
            self.open.insert(id);
        }
    }

    /// Where `control` sits: the layout's place, else its own.
    pub fn rect(&self, control: &Control) -> GuiRect {
        let laid = self.layout.rect(control.parent, control.id);
        let placed = if control.fixed {
            control.rect.or(laid)
        } else {
            laid.or(control.rect)
        };
        placed.unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_places_and_controls_fall_back_to_their_own_rect() {
        let gui = Gui::new(
            b"[0-0]\nw=800\nh=600\n[0-130]\nx=0\ny=500\nw=800\nh=100\n[130-7]\nx=5\ny=6\nw=40\nh=20\n",
            b"[Button42]\nFrameAmount=2\nFrame0=a.dds\nFrame1=b.dds\n",
            b"[0-130]\ntype=dialog\nopen=always\n[130-7]\ntype=button\nani=Button42\naction=bag\n\
              [130-9]\ntype=text\nx=1\ny=2\ntext=Lv {hero.level}\ncolor=ffff00\n[130-10]\ntype=nothing\n",
        );
        assert_eq!(gui.layout.base(), [800, 600]);
        assert_eq!(gui.controls.len(), 3);
        assert_eq!(gui.skin.frames("Button42").len(), 2);
        let dialog = gui.dialogs().next().unwrap();
        assert!(gui.is_open(dialog));
        assert_eq!(gui.rect(dialog).y, 500);
        assert_eq!(gui.rect(&gui.controls[1]).w, 40);
        let text = &gui.controls[2];
        assert_eq!((gui.rect(text).x, text.color), (1, [255, 255, 0]));
    }
}
