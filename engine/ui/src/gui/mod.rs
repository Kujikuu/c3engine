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

pub use screen::{EditKey, GuiData, GuiEvent, GuiImages, Place, Textures};

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
    /// A thumb (`ani`) moved along `travel` pixels for `value`, 0 to `max` (TQ's CMySlider).
    Slider,
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
            "slider" => Self::Slider,
            _ => return None,
        })
    }
}

/// A dialog's width from a count (see [`Control::stretch`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stretch {
    pub count: String,
    pub step: i32,
    pub pad: i32,
    pub max: i32,
}

impl Stretch {
    fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split(',').map(str::trim);
        let count = parts.next().filter(|c| !c.is_empty())?.to_owned();
        let mut number = || parts.next().and_then(|n| n.parse().ok());
        Some(Self {
            count,
            step: number()?,
            pad: number().unwrap_or(0),
            max: number().unwrap_or(i32::MAX),
        })
    }
}

/// Where a text control's text sits in its width.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
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
    /// The skin section it shows, or an image file shown as it is (`file=`; TQ's Win32
    /// dialogs' bitmaps).
    pub ani: String,
    pub file: String,
    pub frame: usize,
    /// Its frames played in turn, one each this many milliseconds (`animate=300`); 0: still.
    pub animate: u32,
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
    /// A progress bar's image area, drawn as it is at the control's corner (`area=w,h`;
    /// TQ's `CMyProgress::SetWidthAndHeight`); none: the control's size.
    pub area: Option<[i32; 2]>,
    /// Text with `{binding}`s in it.
    pub text: String,
    /// A file of `value,name` (or `value=name`) lines (TQ's `ProfessionalName.ini`,
    /// `StrRes.ini`): the text, worked out, is shown as the name it maps to.
    pub names: String,
    /// The control of its dialog whose layout place it takes (`place=<id>`; a set of
    /// controls shown in turn where one control is laid out).
    pub place: Option<u32>,
    /// `value:key` pairs between spaces turning the text into the names file's key first
    /// (`*:key` for any other value).
    pub map: String,
    /// Text colour, `rrggbb`.
    pub color: [u8; 3],
    /// Where text sits in its width (`align=center|right`; TQ's statics' position types).
    pub align: Align,
    pub size: f32,
    /// What a click asks the game for.
    pub action: String,
    /// A sound (an asset path) a click plays in place of the UI's click.
    pub sound: String,
    /// The window number the game opens this dialog by (0: none).
    pub window: u32,
    /// More window numbers it answers to (`window=2,3`: TQ's oven modes).
    pub windows: Vec<u32>,
    /// A grid's columns and rows.
    pub cols: u32,
    pub rows: u32,
    /// A list's columns: this far apart (`step=`; 0: its width shared) and this wide
    /// (`cell=`; 0: the step); its rows (a text: its text) wrapped within the cell, or its
    /// width (`wrap=1`), and their text
    /// in this colour under the pointer (`hover=rrggbb`) in place of a lit box. A list shows
    /// `limit` rows at most.
    pub step: i32,
    pub cell: i32,
    pub wrap: bool,
    pub hover: Option<[u8; 3]>,
    /// What a list row double-clicked asks for (`double=`; TQ's CMyList OnLButtonDblClk).
    pub double: String,
    /// How far a slider's thumb travels, in layout pixels (`travel=80`).
    pub travel: i32,
    /// A slider whose thumb rises from its place by its value (`vertical=1`: TQ's line slider).
    pub vertical: bool,
    /// A stacked control comes no higher than this (`min_y=`; under an NPC's face).
    pub min_y: i32,
    /// A stacked control beside the one before it, at its height (`beside=1`; a button by
    /// its edit).
    pub beside: bool,
    /// A colour filled under it (`back=rrggbb`; a Win32 edit's brush).
    pub back: Option<[u8; 3]>,
    /// A list shown from its end, the newest rows lowest, the mouse wheel going back (`tail=1`;
    /// a log).
    pub tail: bool,
    /// Numbered documents a list shows in place of rows (`pages=Help/help{n}.htm`: HTML read
    /// as paragraphs), the one view group `page` holds.
    pub pages: String,
    pub page: String,
    /// A list's rows each on skin section `row` (frame 1 under the pointer, 2 pressed), this
    /// tall (`line=`; 0: its font's), their text this far in (`indent=`), a tip each from the
    /// rows of binding `tips`, and section `foot` under the last (TQ's auto-path menu).
    pub row: String,
    pub line: i32,
    pub indent: Option<i32>,
    pub tips: String,
    pub foot: String,
    /// An image showing its binding's number as piles of pictures, the largest unit first
    /// (`piles=Button435:10000,Button434:1000,Button433:100`: TQ's chips, a column each unit,
    /// one picture 8 pixels above another).
    pub piles: Vec<(String, u64)>,
    /// Moved down by what a binding reads, times a step (`dy=autorun.open_row*32`).
    pub dy: String,
    /// A dialog's opening: `always`, or empty when actions open it.
    pub open: String,
    /// The screen part a dialog takes (`left`, `middle`, `right`; TQ's CDlgMain owners):
    /// opening it closes the one there. None: it closes nothing (the HUD strips).
    pub slot: String,
    /// Other parts, or dialogs by number, a dialog clears as it opens (`clears=right`: a
    /// trade; `clears=131,160`: the setup hiding the status).
    pub clears: String,
    /// Esc closes the dialog too, with the right part's (the bag).
    pub esc: bool,
    /// Dialogs opened with it (`opens=153`: the bag beside a workbench) and closed with it
    /// (`hides=153`).
    pub opens: Vec<u32>,
    pub hides: Vec<u32>,
    /// Numbers in its text with thousands commas (`commas=1`; TQ's StringChange).
    pub commas: bool,
    /// What a `%s` in its text (a names-file string) becomes (`fill_in={prompt.name}`).
    pub fill_in: String,
    /// Dialogs that keep it from opening while they show (`blocked=152,161`: the status not
    /// over a shop or warehouse, as OnMainQuery checks the right part's owner).
    pub blocked: Vec<u32>,
    /// A dialog as wide as a count of what it shows (`stretch=skill_count.magic,50,18,10`:
    /// that many at most, `step` each, `pad` more) and against the layout's right edge (TQ's
    /// skill strips, MoveWindow by their skill count).
    pub stretch: Option<Stretch>,
    /// A grid showing its list from the number view group `<group>` holds (`offset=shop_page`;
    /// a shop's later pages).
    pub offset: String,
    /// A grid whose items are picked up with the left button and put down in another grid,
    /// the right button using one (`pick=1`; TQ's CMyGrid).
    pub pick: bool,
    /// Bindings (`|` between them): a dialog shows while one is set (not 0), as the game opens
    /// it (a shop: `hero.shop`), in place of the engine's own window; a control in a dialog
    /// shows only then (a text box a page asks for).
    pub when: String,
    /// The screen a dialog belongs to (`login`, `servers`, `characters`, `create`,
    /// `loading`); empty: the game's.
    pub screen: String,
    /// An edit hides what is typed.
    pub password: bool,
    /// An edit takes digits only (`digits=1`; an amount of money).
    pub digits: bool,
    /// A button that cannot be clicked while this `when`-style expression holds, drawn in
    /// its frame 2 as TQ's disabled buttons (`disabled=!team.picked_member`).
    pub disabled: String,
    /// A button held down once used (`latched=trade.i_agreed`): its frame 1, no clicks.
    pub latched: String,
    /// A tab button showing it is the one chosen (`chosen=board.tab.0`): its frame 2, still
    /// clickable (TQ's SetCurFrame(2)).
    pub chosen: String,
    /// The most characters an edit takes (0: any), or a binding that says (`limit_of=`).
    pub limit: usize,
    pub limit_of: String,
    /// A binding: while it is set, the text shows in `mark_color` (a listed name in red).
    pub mark: String,
    pub mark_color: [u8; 3],
    /// Its own place wins over the layout's (the client's code moves it, as TQ's does for
    /// screens it lays out per resolution).
    pub fixed: bool,
    /// A control stacked under the dialog's stacked control before it, as tall as what it
    /// shows (`stack=1`; an NPC's words, then its answers).
    pub stack: bool,
    /// A dialog that fits its height to its stacked controls: its least height (`fit=104`),
    /// its picture stretched to it.
    pub fit: i32,
}

impl Control {
    /// The frames it shows: its file, else skin section `section`'s.
    pub fn frames<'a>(&'a self, skin: &'a Skin, section: &str) -> &'a [String] {
        match self.file.is_empty() {
            true => skin.frames(section),
            false => std::slice::from_ref(&self.file),
        }
    }

    /// `value` as the names file's key, by the control's map (unchanged when it has none).
    pub fn map_value(&self, value: &str) -> String {
        let mut other = None;
        for pair in self.map.split_whitespace() {
            let Some((from, to)) = pair.split_once(':') else {
                continue;
            };
            if from == value {
                return to.to_owned();
            }
            if from == "*" {
                other = Some(to);
            }
        }
        other.unwrap_or(value).to_owned()
    }

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
        let area = section.get("area").and_then(|s| {
            let n: Vec<i32> = s.split(',').filter_map(|v| v.trim().parse().ok()).collect();
            <[i32; 2]>::try_from(n).ok()
        });
        let ids = |key: &str| -> Vec<u32> {
            (section.get(key).unwrap_or_default().split(','))
                .filter_map(|n| n.trim().parse().ok())
                .collect()
        };
        let rgb = |key: &str| {
            (section.get(key))
                .and_then(|c| u32::from_str_radix(c.trim_start_matches('#'), 16).ok())
                .map_or([255, 255, 255], |c| {
                    [(c >> 16) as u8, (c >> 8) as u8, c as u8]
                })
        };
        let color = rgb("color");
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
            file: text("file"),
            frame: section.int("frame").unwrap_or(0).max(0) as usize,
            animate: section.int("animate").unwrap_or(0).max(0) as u32,
            src,
            rect,
            value: text("value"),
            focus: text("focus"),
            max: text("max"),
            fill,
            area,
            text: text("text"),
            names: text("names"),
            map: text("map"),
            place: section.int("place").and_then(|p| u32::try_from(p).ok()),
            color,
            align: match section.get("align") {
                Some("center") => Align::Center,
                Some("right") => Align::Right,
                _ => Align::Left,
            },
            size: section.int("size").unwrap_or(12).max(4) as f32,
            action: text("action"),
            sound: text("sound"),
            window: section.int("window").map_or(0, |w| w.max(0) as u32),
            windows: (section.get("window").unwrap_or_default().split(','))
                .filter_map(|w| w.trim().parse().ok())
                .collect(),
            cols: section.int("cols").unwrap_or(1).max(1) as u32,
            rows: section.int("rows").unwrap_or(1).max(1) as u32,
            step: section.int("step").unwrap_or(0).max(0),
            cell: section.int("cell").unwrap_or(0).max(0),
            wrap: section.int("wrap").is_some_and(|w| w != 0),
            hover: section.get("hover").map(|_| rgb("hover")),
            double: text("double"),
            min_y: section.int("min_y").unwrap_or(0),
            travel: section.int("travel").unwrap_or(0).max(0),
            vertical: section.int("vertical").is_some_and(|v| v != 0),
            beside: section.int("beside").is_some_and(|b| b != 0),
            back: section.get("back").map(|_| rgb("back")),
            tail: section.int("tail").is_some_and(|t| t != 0),
            pages: text("pages"),
            page: text("page"),
            row: text("row"),
            line: section.int("line").unwrap_or(0),
            indent: section.int("indent"),
            tips: text("tips"),
            foot: text("foot"),
            dy: text("dy"),
            piles: (text("piles").split(','))
                .filter_map(|p| {
                    let (section, unit) = p.split_once(':')?;
                    Some((section.trim().to_owned(), unit.trim().parse().ok()?))
                })
                .collect(),
            open: text("open"),
            slot: text("slot"),
            clears: text("clears"),
            esc: section.int("esc").is_some_and(|e| e != 0),
            opens: ids("opens"),
            hides: ids("hides"),
            commas: section.int("commas").is_some_and(|c| c != 0),
            fill_in: text("fill_in"),
            blocked: ids("blocked"),
            pick: section.int("pick").is_some_and(|p| p != 0),
            offset: text("offset"),
            stretch: Stretch::parse(section.get("stretch").unwrap_or_default()),
            when: text("when"),
            screen: text("screen").to_ascii_lowercase(),
            password: section.int("password").is_some_and(|p| p != 0),
            digits: section.int("digits").is_some_and(|d| d != 0),
            disabled: text("disabled"),
            latched: text("latched"),
            chosen: text("chosen"),
            limit: section.int("limit").unwrap_or(0).max(0) as usize,
            limit_of: text("limit_of"),
            mark: text("mark"),
            mark_color: rgb("mark_color"),
            stack: section.int("stack").is_some_and(|s| s != 0),
            fit: section.int("fit").unwrap_or(0).max(0),
            fixed: section.int("fixed").is_some_and(|f| f != 0),
        })
    }
}

/// The sounds a game UI plays (asset paths; empty: none).
#[derive(Debug, Clone, Default)]
pub struct GuiSounds {
    pub open: String,
    pub close: String,
    pub click: String,
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
    /// Dialogs their `when` shows, as last worked out, and those of them the player closed
    /// (shown again once their `when` has ended).
    when_shown: BTreeSet<u32>,
    dismissed: BTreeSet<u32>,
    /// The screen shown (empty: the game).
    screen: String,
    /// The sounds of a dialog opening and closing and a button clicked (asset paths).
    pub sounds: GuiSounds,
    /// The name files text controls name, read (`value` → name).
    pub(crate) names: std::collections::HashMap<String, std::collections::HashMap<String, String>>,
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
            when_shown: BTreeSet::new(),
            dismissed: BTreeSet::new(),
            screen: String::new(),
            names: Default::default(),
            sounds: GuiSounds::default(),
        }
    }

    /// Reads the name files the controls name (`names=`), each through `read`.
    pub fn load_names(&mut self, read: impl Fn(&str) -> Option<Vec<u8>>) {
        let files: BTreeSet<String> = (self.controls.iter())
            .filter(|c| !c.names.is_empty())
            .map(|c| c.names.clone())
            .collect();
        for file in files {
            let Some(bytes) = read(&file) else {
                continue;
            };
            let table = (String::from_utf8_lossy(&bytes).lines())
                .filter_map(|line| {
                    let line = line.trim();
                    // `key=name` (StrRes.ini) or `key,name`: whichever comes first (a
                    // StrRes line's text may hold commas).
                    let at = line.find([',', '='])?;
                    let (value, name) = (&line[..at], &line[at + 1..]);
                    Some((value.trim().to_owned(), name.trim().to_owned()))
                })
                .collect();
            self.names.insert(file, table);
        }
        // The numbered documents, each under its pattern by its number.
        let patterns: BTreeSet<String> = (self.controls.iter())
            .filter(|c| !c.pages.is_empty())
            .map(|c| c.pages.clone())
            .collect();
        for pattern in patterns {
            let table = (0..PAGES)
                .filter_map(|n| {
                    let bytes = read(&pattern.replace("{n}", &n.to_string()))?;
                    Some((n.to_string(), html_paragraphs(&bytes).join("\n")))
                })
                .collect();
            self.names.insert(pattern, table);
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
    /// Whether any control is bound to something starting with `prefix` (the game's own
    /// skill strip: `skill.`).
    /// Whether a dialog shows questions of kind `kind` (`when=prompt.kind.<kind>`).
    pub fn asks(&self, kind: &str) -> bool {
        let shown = format!("prompt.kind.{kind}");
        self.controls.iter().any(|c| c.when.contains(&shown))
    }

    pub fn binds(&self, prefix: &str) -> bool {
        self.controls.iter().any(|c| c.value.starts_with(prefix))
    }

    /// Whether an open dialog has an edit of `field` (the game's own chat line).
    pub fn edits(&self, field: &str) -> bool {
        (self.controls.iter())
            .filter(|c| c.kind == ControlKind::Edit && c.value == field)
            .any(|c| {
                (self.controls.iter()).any(|d| d.id == c.parent && d.parent == 0 && self.is_open(d))
            })
    }

    pub fn is_open(&self, dialog: &Control) -> bool {
        dialog.screen == self.screen
            && (dialog.open.eq_ignore_ascii_case("always") || self.open.contains(&dialog.id))
    }

    /// Whether a dialog shows while `binding` is set, alone or with other terms (`a&!b`): the
    /// game's window is then this UI's.
    pub fn shows_while(&self, binding: &str) -> bool {
        self.dialogs()
            .any(|d| (d.when.split('|')).any(|alt| alt.split('&').any(|w| w == binding)))
    }

    /// Opens dialog `id`, closing the dialog in the part of the screen it takes and in the
    /// parts it clears (TQ's CloseCurDlg); a dialog without a part closes nothing.
    pub fn open(&mut self, id: u32) {
        let Some(dialog) = self.dialogs().find(|d| d.id == id) else {
            self.open.insert(id);
            return;
        };
        if dialog.blocked.iter().any(|&b| self.showing(b)) {
            return;
        }
        let cleared: Vec<&str> = std::iter::once(dialog.slot.as_str())
            .chain(dialog.clears.split(',').map(str::trim))
            .filter(|p| !p.is_empty())
            .collect();
        let taken: Vec<u32> = (self.dialogs())
            .filter(|d| d.id != id && self.showing(d.id))
            .filter(|d| {
                cleared.contains(&d.slot.as_str()) || cleared.contains(&d.id.to_string().as_str())
            })
            .map(|d| d.id)
            .collect();
        let opens = dialog.opens.clone();
        for other in taken {
            self.shut(other);
        }
        self.open.insert(id);
        self.dismissed.remove(&id);
        for with in opens {
            self.open(with);
        }
    }

    /// Whether dialog `id` shows: opened, or shown by its `when` and not closed.
    fn showing(&self, id: u32) -> bool {
        self.open.contains(&id) || (self.when_shown.contains(&id) && !self.dismissed.contains(&id))
    }

    /// Closes dialog `id` (dismissing it when its `when` shows it), and those it hides.
    fn shut(&mut self, id: u32) {
        self.open.remove(&id);
        if self.when_shown.contains(&id) {
            self.dismissed.insert(id);
        }
        let hides = (self.dialogs())
            .find(|d| d.id == id)
            .map(|d| d.hides.clone());
        for other in hides.unwrap_or_default() {
            if other != id {
                self.shut(other);
            }
        }
    }

    /// Notes which dialogs their `when` shows now; one the player closed shows again once
    /// its `when` has ended.
    pub fn update_shown(&mut self, data: &dyn GuiData) {
        let shown: BTreeSet<u32> = (self.dialogs())
            .filter(|d| !d.when.is_empty() && screen::holds(d, data))
            .map(|d| d.id)
            .collect();
        self.dismissed.retain(|id| shown.contains(id));
        // A dialog its `when` no longer shows closes those it hides (a booth's cry).
        let gone: Vec<u32> = self.when_shown.difference(&shown).copied().collect();
        self.when_shown = shown;
        for id in gone {
            let hides = (self.dialogs().find(|d| d.id == id)).map(|d| d.hides.clone());
            for other in hides.unwrap_or_default() {
                self.shut(other);
            }
        }
    }

    /// Whether the player closed dialog `id` while its `when` shows it.
    pub fn dismissed(&self, id: u32) -> bool {
        self.dismissed.contains(&id)
    }

    /// Esc: closes the right part's dialog and those Esc closes with it (the bag), as TQ's
    /// shell does; whether one was open.
    pub fn escape(&mut self) -> bool {
        let closing: Vec<u32> = (self.dialogs())
            .filter(|d| self.showing(d.id) && (d.slot == "right" || d.esc))
            .map(|d| d.id)
            .collect();
        for &id in &closing {
            self.shut(id);
        }
        !closing.is_empty()
    }

    /// Whether an open dialog shows a control bound to something starting with `prefix`
    /// (`work.`: a workbench taking bag items).
    pub fn open_uses(&self, prefix: &str) -> bool {
        let open: Vec<u32> = (self.dialogs())
            .filter(|d| self.is_open(d))
            .map(|d| d.id)
            .collect();
        (self.controls.iter()).any(|c| open.contains(&c.parent) && c.value.starts_with(prefix))
    }

    /// Opens the dialogs that answer to window number `window`; whether there was one.
    pub fn open_window(&mut self, window: u32) -> bool {
        let ids: Vec<u32> = (self.dialogs())
            .filter(|d| window != 0 && (d.window == window || d.windows.contains(&window)))
            .map(|d| d.id)
            .collect();
        for &id in &ids {
            self.open(id);
        }
        !ids.is_empty()
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

    /// Closes dialog `id`.
    pub fn close(&mut self, id: u32) {
        self.shut(id);
    }

    /// Closes the dialogs with a control bound to `binding` (a list hidden once picked from).
    pub fn close_holding(&mut self, binding: &str) {
        let holders: Vec<u32> = (self.controls.iter())
            .filter(|c| c.value == binding)
            .map(|c| c.parent)
            .collect();
        for id in holders {
            self.shut(id);
        }
    }

    /// Opens dialog `id`, or closes it when it shows; whether it shows now.
    pub fn toggle(&mut self, id: u32) -> bool {
        if self.showing(id) {
            self.shut(id);
            return false;
        }
        self.open(id);
        true
    }

    /// Where `control` sits: the layout's place, else its own.
    pub fn rect(&self, control: &Control) -> GuiRect {
        let laid = self
            .layout
            .rect(control.parent, control.place.unwrap_or(control.id));
        let placed = if control.fixed {
            control.rect.or(laid)
        } else {
            laid.or(control.rect)
        };
        placed.unwrap_or_default()
    }
}

/// The numbers a document pattern is read for (`{n}` from 0 below this).
const PAGES: u32 = 1000;

/// An HTML page's text as paragraphs (TQ's help pages, shown in a browser): its head, styles
/// and scripts left out, a line ended at each break, paragraph, row or heading, the other
/// tags dropped, the common entities read, each line's spaces made one.
pub fn html_paragraphs(bytes: &[u8]) -> Vec<String> {
    let html = String::from_utf8_lossy(bytes);
    let lower = html.to_ascii_lowercase();
    let mut text = String::new();
    let mut at = 0;
    while let Some(open) = lower[at..].find('<').map(|i| at + i) {
        // The page's own line ends are spaces, as a browser reads them.
        text.push_str(&html[at..open].replace(['\r', '\n'], " "));
        let Some(close) = lower[open..].find('>').map(|i| open + i) else {
            break;
        };
        let tag = &lower[open + 1..close];
        let name: String = (tag.trim_start_matches('/').chars())
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        at = close + 1;
        // A head, style or script is skipped to its end.
        if !tag.starts_with('/') && matches!(name.as_str(), "head" | "style" | "script") {
            let end = format!("</{name}");
            at = lower[at..].find(&end).map_or(lower.len(), |i| at + i);
            continue;
        }
        let breaks = match name.as_str() {
            "br" | "tr" | "li" | "div" => true,
            "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => tag.starts_with('/'),
            _ => false,
        };
        if breaks {
            text.push('\n');
        }
    }
    if at < html.len() {
        text.push_str(&html[at..].replace(['\r', '\n'], " "));
    }
    let text = (text
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">"))
    .replace("&quot;", "\"")
    .replace("&#39;", "'")
    .replace("&amp;", "&");
    (text.lines())
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_pages_read_as_paragraphs() {
        let page = b"<HTML><HEAD><TITLE>Help</TITLE><style>td {color: #999}</style></HEAD>\n\
            <BODY><p align=center><font color=\"#FFFF00\">Get  Promoted</font></p>\n\
            <TD><p>At level 15,\n   see the Job Center.<br><br>Warriors &amp; Archers</p></BODY>";
        assert_eq!(
            html_paragraphs(page),
            [
                "Get Promoted",
                "At level 15, see the Job Center.",
                "Warriors & Archers"
            ]
        );
    }

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

#[cfg(test)]
mod window_tests {
    use super::*;

    fn opened(gui: &Gui) -> Vec<u32> {
        (gui.dialogs())
            .filter(|d| gui.is_open(d))
            .map(|d| d.id)
            .collect()
    }

    #[test]
    fn a_dialog_closes_the_one_in_its_part_and_those_it_clears() {
        let controls = b"[0-1]
type=dialog
slot=right
[0-2]
type=dialog
slot=right
[0-3]
type=dialog
[0-4]
type=dialog
slot=left
clears=right
";
        let mut gui = Gui::new(b"", b"", controls);
        gui.open(1);
        gui.open(3);
        gui.toggle(2);
        assert_eq!(opened(&gui), [2, 3]);
        gui.open(4);
        assert_eq!(opened(&gui), [3, 4]);
    }

    struct Shop(f64);
    impl GuiData for Shop {
        fn number(&self, key: &str) -> Option<f64> {
            (key == "shop").then_some(self.0)
        }
    }

    #[test]
    fn a_dialog_its_when_shows_closes_until_the_when_ends() {
        let controls = b"[0-1]
type=dialog
when=shop
";
        let mut gui = Gui::new(b"", b"", controls);
        gui.update_shown(&Shop(1.0));
        assert!(!gui.toggle(1));
        assert!(gui.dismissed(1));
        gui.update_shown(&Shop(0.0));
        assert!(!gui.dismissed(1));
    }

    #[test]
    fn a_workbench_brings_the_bag_and_takes_it_away() {
        let controls = b"[0-1]
type=dialog
slot=right
opens=2
hides=2
[0-2]
type=dialog
";
        let mut gui = Gui::new(b"", b"", controls);
        gui.open(1);
        assert_eq!(opened(&gui), [1, 2]);
        gui.close(1);
        assert!(opened(&gui).is_empty());
    }

    #[test]
    fn escape_closes_the_right_part_and_the_bag() {
        let controls = b"[0-1]
type=dialog
slot=right
[0-2]
type=dialog
esc=1
[0-3]
type=dialog
slot=middle
";
        let mut gui = Gui::new(b"", b"", controls);
        for id in [1, 2, 3] {
            gui.open(id);
        }
        assert!(gui.escape());
        assert_eq!(opened(&gui), [3]);
        assert!(!gui.escape());
    }
}

#[cfg(test)]
mod names_tests {
    use super::*;

    #[test]
    fn name_files_the_controls_name_are_read() {
        let controls = b"[0-1]
type=dialog
[1-2]
type=text
text={hero.profession}
names=jobs.ini
";
        let mut gui = Gui::new(b"", b"", controls);
        gui.load_names(|path| {
            (path == "jobs.ini").then(|| {
                b"10,InternTrojan
11,Trojan
"
                .to_vec()
            })
        });
        assert_eq!(gui.names["jobs.ini"]["11"], "Trojan");
        assert_eq!(gui.names["jobs.ini"].len(), 2);
    }
}
