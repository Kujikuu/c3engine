//! The game's mouse cursors (Project Settings > Cursors): one for each thing the mouse can
//! be over, drawn by the game over everything (the system pointer is hidden) so the client
//! and the Studio's Game view show the same. A cursor without an image is the system's.

use crate::module::GameModule;
use c3_assets::cursor::{self, Cursor};
use c3_core::settings::{self, CursorSettings};
use egui::{Color32, Pos2, Rect, TextureHandle, TextureOptions};

/// What the mouse is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorKind {
    Normal,
    /// A monster.
    Attack,
    /// An NPC.
    Talk,
    /// Something on the ground.
    PickUp,
    /// A portal.
    Portal,
}

impl CursorKind {
    const ALL: [Self; 5] = [
        Self::Normal,
        Self::Attack,
        Self::Talk,
        Self::PickUp,
        Self::Portal,
    ];

    fn setting(self, s: &CursorSettings) -> &settings::Cursor {
        match self {
            Self::Normal => &s.normal,
            Self::Attack => &s.attack,
            Self::Talk => &s.talk,
            Self::PickUp => &s.pick_up,
            Self::Portal => &s.portal,
        }
    }

    /// The system pointer shown when the game has no cursor for it.
    fn system(self) -> egui::CursorIcon {
        match self {
            Self::Normal => egui::CursorIcon::Default,
            Self::Attack => egui::CursorIcon::Crosshair,
            Self::Talk => egui::CursorIcon::PointingHand,
            Self::PickUp => egui::CursorIcon::Grab,
            Self::Portal => egui::CursorIcon::Alias,
        }
    }
}

/// A cursor read from the project, its frames uploaded when first drawn.
struct Loaded {
    cursor: Cursor,
    textures: Vec<Option<TextureHandle>>,
}

/// The game's cursors.
#[derive(Default)]
pub struct Cursors {
    loaded: [Option<Loaded>; 5],
    /// The settings they were read from.
    from: CursorSettings,
}

impl Cursors {
    /// Reads the cursors `settings` name through `game`; what couldn't be read is reported
    /// and shows the system pointer.
    pub fn load(game: &dyn GameModule, settings: &CursorSettings) -> (Self, Vec<String>) {
        let mut problems = Vec::new();
        let loaded = CursorKind::ALL.map(|kind| {
            let setting = kind.setting(settings);
            if setting.image.is_empty() {
                return None;
            }
            match read(game, setting) {
                Ok(cursor) => Some(Loaded {
                    textures: vec![None; cursor.frames.len()],
                    cursor,
                }),
                Err(e) => {
                    problems.push(format!("Cursor {}: {e}", setting.image));
                    None
                }
            }
        });
        let cursors = Self {
            loaded,
            from: settings.clone(),
        };
        (cursors, problems)
    }

    /// The settings these cursors were read from.
    pub fn settings(&self) -> &CursorSettings {
        &self.from
    }

    /// Shows the cursor for `kind` at `pointer` (`now` in seconds); over game UI windows the
    /// normal one.
    pub fn show(&mut self, ctx: &egui::Context, kind: CursorKind, pointer: Option<Pos2>, now: f64) {
        let Some(pointer) = pointer else { return };
        let kind = if self.loaded[kind as usize].is_some() || kind == CursorKind::Normal {
            kind
        } else if self.loaded[CursorKind::Normal as usize].is_some() {
            // A game with its own cursors shows its arrow rather than a system one.
            CursorKind::Normal
        } else {
            kind
        };
        let Some(loaded) = &mut self.loaded[kind as usize] else {
            ctx.set_cursor_icon(kind.system());
            return;
        };
        let ms = (now * 1000.0).max(0.0) as u64;
        let frame = loaded.cursor.frame_at(ms);
        let index = loaded
            .cursor
            .frames
            .iter()
            .position(|f| std::ptr::eq(f, frame))
            .unwrap_or(0);
        let texture = loaded.textures[index].get_or_insert_with(|| {
            let image = &frame.image;
            let size = [image.width as usize, image.height as usize];
            ctx.load_texture(
                format!("game cursor {kind:?} {index}"),
                egui::ColorImage::from_rgba_unmultiplied(size, &image.rgba),
                TextureOptions::NEAREST,
            )
        });
        let hotspot = egui::vec2(frame.hotspot[0] as f32, frame.hotspot[1] as f32);
        let size = egui::vec2(frame.image.width as f32, frame.image.height as f32);
        let min = pointer - hotspot;
        ctx.set_cursor_icon(egui::CursorIcon::None);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::unique("game cursor"),
        ));
        painter.image(
            texture.id(),
            Rect::from_min_size(min, size),
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
        ctx.request_repaint();
    }
}

/// Reads one cursor: a Windows cursor file, or any image the game reads.
pub fn read(game: &dyn GameModule, setting: &settings::Cursor) -> Result<Cursor, String> {
    let path = setting.image.replace('\\', "/");
    let lower = path.to_ascii_lowercase();
    let mut cursor = if [".cur", ".ani", ".ico"].iter().any(|e| lower.ends_with(e)) {
        let bytes = game.read_file(&path).ok_or("the file can't be found")?;
        cursor::decode(&bytes, &crate::native::decode_image)?
    } else {
        let image = game
            .assets()
            .image(&path)
            .ok_or("the image can't be found or read")?;
        Cursor::still(image, [0, 0])
    };
    if let Some(hotspot) = setting.hotspot {
        for frame in &mut cursor.frames {
            frame.hotspot = hotspot;
        }
    }
    Ok(cursor)
}
