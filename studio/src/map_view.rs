//! Map viewport: streams puzzle ground tiles and overlays the DMap cell grid.
//! World space matches the client: cell (x, y) centre =
//! (CELL_W*(x-y)/2 + origin_x, CELL_H*(x+y)/2 + origin_y), origin = (CELL_W*width/2, CELL_H/2).
//! The puzzle is placed at `bg_origin()` (client `GetBgWorldPos`).

use c3_formats_tq::{
    ani::Ani,
    dmap::{Cell, DMap, InteractiveObj},
    pul::{EMPTY, GRID_SIZE, Puzzle},
    texture::Image,
    vfs::ClientFs,
};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, TextureHandle, Vec2};
use std::collections::HashMap;

pub const CELL_W: f32 = 64.0;
pub const CELL_H: f32 = 32.0;
pub const MIN_ZOOM: f32 = 0.03;
pub const MAX_ZOOM: f32 = 4.0;
/// Tiles decoded per frame, so panning stays smooth while streaming.
const DECODE_BUDGET: usize = 6;

/// Overlay palette, shared by the viewport and the legend.
pub mod colors {
    use eframe::egui::Color32;
    pub const BLOCKED: Color32 = Color32::from_rgba_premultiplied(64, 10, 10, 64);
    pub const PORTAL: Color32 = Color32::from_rgb(70, 150, 255);
    pub const COVER: Color32 = Color32::from_rgb(80, 220, 120);
    pub const TERRAIN: Color32 = Color32::from_rgb(240, 200, 60);
    pub const SOUND: Color32 = Color32::from_rgb(200, 120, 255);
    pub const EFFECT: Color32 = Color32::from_rgb(255, 140, 60);
    pub const HOVER: Color32 = Color32::from_rgb(255, 214, 90);
}

pub struct MapDoc {
    pub path: String,
    pub dmap: DMap,
    pub puzzle: Puzzle,
    pub ani: Ani,
    pub used_tiles: usize,
    tiles: HashMap<u16, Option<TextureHandle>>,
}

impl MapDoc {
    pub fn load(fs: &ClientFs, path: &str) -> Result<Self, String> {
        let read = |p: &str| fs.read(p).ok_or_else(|| format!("File not found: {p}"));
        let dmap = DMap::parse(&read(path)?).map_err(|e| format!("{path}: {e}"))?;
        let pul_path = dmap.puzzle_file.as_str();
        let puzzle = Puzzle::parse(&read(&pul_path)?).map_err(|e| format!("{pul_path}: {e}"))?;
        let ani_path = puzzle.ani_file.as_str();
        let ani = Ani::parse(&read(&ani_path)?).map_err(|e| format!("{ani_path}: {e}"))?;
        let mut used: Vec<u16> = puzzle
            .tiles
            .iter()
            .copied()
            .filter(|&t| t != EMPTY)
            .collect();
        used.sort_unstable();
        used.dedup();
        Ok(Self {
            path: path.to_owned(),
            dmap,
            puzzle,
            ani,
            used_tiles: used.len(),
            tiles: HashMap::new(),
        })
    }

    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    pub fn loaded_tiles(&self) -> usize {
        self.tiles.len()
    }

    pub fn origin(&self) -> Vec2 {
        Vec2::new(CELL_W * self.dmap.width as f32 / 2.0, CELL_H / 2.0)
    }

    pub fn cell_to_world(&self, x: i32, y: i32) -> Pos2 {
        let o = self.origin();
        Pos2::new(
            CELL_W * (x - y) as f32 / 2.0 + o.x,
            CELL_H * (x + y) as f32 / 2.0 + o.y,
        )
    }

    pub fn world_to_cell(&self, p: Pos2) -> (i32, i32) {
        let o = self.origin();
        let a = (p.x - o.x) / (CELL_W / 2.0);
        let b = (p.y - o.y) / (CELL_H / 2.0);
        (
            ((a + b) / 2.0).round() as i32,
            ((b - a) / 2.0).round() as i32,
        )
    }

    pub fn cell(&self, (x, y): (i32, i32)) -> Option<Cell> {
        (x >= 0 && y >= 0)
            .then(|| self.dmap.cell(x as u32, y as u32).copied())
            .flatten()
    }

    /// Puzzle size in world pixels.
    pub fn bg_size(&self) -> Vec2 {
        Vec2::new(self.puzzle.width as f32, self.puzzle.height as f32) * GRID_SIZE as f32
    }

    /// World position of the puzzle's top-left. Port of `CGameMap::GetBgWorldPos`
    /// (integer math as in the client).
    pub fn bg_origin(&self) -> Pos2 {
        let (bw, bh) = (
            self.puzzle.width as i32 * GRID_SIZE as i32,
            self.puzzle.height as i32 * GRID_SIZE as i32,
        );
        let (w, h) = (self.dmap.width as i32, self.dmap.height as i32);
        let (cw, ch) = (CELL_W as i32, CELL_H as i32);
        let x = cw * w / 2 - bw / 2;
        let y = ch / 2 + ch * h / 2 - bh / 2 - ((h + 1) % 2) * ch / 2;
        Pos2::new(x as f32, y as f32)
    }

    pub fn bg_rect(&self) -> Rect {
        Rect::from_min_size(self.bg_origin(), self.bg_size())
    }

    fn tile_texture(
        &mut self,
        ctx: &egui::Context,
        fs: &ClientFs,
        idx: u16,
        budget: &mut usize,
    ) -> Option<TextureHandle> {
        if let Some(t) = self.tiles.get(&idx) {
            return t.clone();
        }
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        let tex = (|| {
            let frame = self.ani.get(&Puzzle::tile_title(idx))?.frame_str(0)?;
            let img = Image::decode(&fs.read(&frame)?).ok()?;
            let color = egui::ColorImage::from_rgba_unmultiplied(
                [img.width as usize, img.height as usize],
                &img.rgba,
            );
            Some(ctx.load_texture(format!("puzzle{idx}"), color, egui::TextureOptions::LINEAR))
        })();
        self.tiles.insert(idx, tex.clone());
        tex
    }
}

pub struct Camera {
    /// World point at the viewport centre.
    pub center: Pos2,
    pub zoom: f32,
    /// Size of the viewport last frame (for fit / keyboard zoom).
    pub viewport: Vec2,
}

impl Camera {
    pub fn fit(&mut self, world: Rect) {
        let vp = if self.viewport.x > 1.0 {
            self.viewport
        } else {
            Vec2::new(1100.0, 800.0)
        };
        self.center = world.center();
        self.zoom = (vp.x / world.width())
            .min(vp.y / world.height())
            .clamp(MIN_ZOOM, MAX_ZOOM)
            * 0.95;
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    }
}

#[derive(Default, Clone, Copy)]
pub struct Overlays {
    pub grid: bool,
    pub blocked: bool,
    pub portals: bool,
    pub objects: bool,
}

pub struct ViewResult {
    pub hover_cell: Option<(i32, i32)>,
    /// Tiles still waiting to stream in.
    pub streaming: bool,
}

pub fn show(
    ui: &mut egui::Ui,
    fs: &ClientFs,
    doc: &mut MapDoc,
    cam: &mut Camera,
    ov: Overlays,
) -> ViewResult {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    cam.viewport = rect.size();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_gray(14));

    if resp.dragged() {
        cam.center -= resp.drag_delta() / cam.zoom;
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let mouse = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());
            let before = cam.center + (mouse - rect.center()) / cam.zoom;
            cam.zoom_by((scroll * 0.002).exp());
            cam.center = before - (mouse - rect.center()) / cam.zoom;
        }
    }

    let to_screen = |w: Pos2| rect.center() + (w - cam.center) * cam.zoom;
    let to_world = |s: Pos2| cam.center + (s - rect.center()) / cam.zoom;
    let view = Rect::from_two_pos(to_world(rect.min), to_world(rect.max));

    // Ground tiles, in puzzle-local coordinates.
    let g = GRID_SIZE as f32;
    let bg = doc.bg_origin().to_vec2();
    let (lmin, lmax) = (view.min - bg, view.max - bg);
    let (x0, y0) = (
        (lmin.x / g).floor().max(0.0) as u32,
        (lmin.y / g).floor().max(0.0) as u32,
    );
    let x1 = ((lmax.x / g).ceil().max(0.0) as u32).min(doc.puzzle.width);
    let y1 = ((lmax.y / g).ceil().max(0.0) as u32).min(doc.puzzle.height);
    let mut budget = DECODE_BUDGET;
    let mut streaming = false;
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    for gy in y0..y1 {
        for gx in x0..x1 {
            let idx = doc.puzzle.tile(gx, gy);
            if idx == EMPTY {
                continue;
            }
            // +0.5px overlap hides seams between tiles when zoomed.
            let r = Rect::from_min_size(
                to_screen(Pos2::new(gx as f32 * g, gy as f32 * g) + bg),
                Vec2::splat(g * cam.zoom + 0.5),
            );
            match doc.tile_texture(ui.ctx(), fs, idx, &mut budget) {
                Some(t) => {
                    painter.image(t.id(), r, uv, Color32::WHITE);
                }
                None if !doc.tiles.contains_key(&idx) => {
                    streaming = true;
                    painter.rect_filled(r, 0.0, Color32::from_gray(26));
                }
                None => {
                    // Tile failed to load: hatch it so broken content is obvious.
                    painter.rect_filled(r, 0.0, Color32::from_rgb(60, 0, 60));
                }
            }
        }
    }
    if streaming {
        ui.ctx().request_repaint();
    }

    // Visible cell range: invert the four view corners.
    let corners = [
        view.min,
        Pos2::new(view.max.x, view.min.y),
        view.max,
        Pos2::new(view.min.x, view.max.y),
    ];
    let cells: Vec<_> = corners.iter().map(|&p| doc.world_to_cell(p)).collect();
    let (w, h) = (doc.dmap.width as i32, doc.dmap.height as i32);
    let cx0 = (cells.iter().map(|c| c.0).min().unwrap() - 1).max(0);
    let cx1 = (cells.iter().map(|c| c.0).max().unwrap() + 1).min(w - 1);
    let cy0 = (cells.iter().map(|c| c.1).min().unwrap() - 1).max(0);
    let cy1 = (cells.iter().map(|c| c.1).max().unwrap() + 1).min(h - 1);
    let cell_px = CELL_W * cam.zoom;

    let diamond = |c: Pos2| {
        let (hw, hh) = (CELL_W / 2.0 * cam.zoom, CELL_H / 2.0 * cam.zoom);
        vec![
            c + Vec2::new(0.0, -hh),
            c + Vec2::new(hw, 0.0),
            c + Vec2::new(0.0, hh),
            c + Vec2::new(-hw, 0.0),
        ]
    };

    // Grid fades in between 24 and 48 px per cell so it never turns into noise.
    let grid_alpha = ((cell_px - 24.0) / 24.0).clamp(0.0, 1.0);
    if (ov.blocked && cell_px >= 3.0) || (ov.grid && grid_alpha > 0.0) {
        let grid_stroke = Stroke::new(1.0, Color32::from_white_alpha((45.0 * grid_alpha) as u8));
        for y in cy0..=cy1 {
            for x in cx0..=cx1 {
                let c = to_screen(doc.cell_to_world(x, y));
                if !rect.expand(cell_px).contains(c) {
                    continue;
                }
                let pts = diamond(c);
                if ov.blocked
                    && doc
                        .dmap
                        .cell(x as u32, y as u32)
                        .is_some_and(|c| c.mask != 0)
                {
                    painter.add(Shape::convex_polygon(
                        pts.clone(),
                        colors::BLOCKED,
                        Stroke::NONE,
                    ));
                }
                if ov.grid && grid_alpha > 0.0 {
                    painter.add(Shape::closed_line(pts, grid_stroke));
                }
            }
        }
    }

    if ov.objects {
        for o in &doc.dmap.interactive {
            let (pos, color) = match o {
                InteractiveObj::Cover { cell, .. } => {
                    (doc.cell_to_world(cell.0, cell.1), colors::COVER)
                }
                InteractiveObj::Terrain { cell, .. } => {
                    (doc.cell_to_world(cell.0, cell.1), colors::TERRAIN)
                }
                InteractiveObj::Sound { world, .. } => {
                    (Pos2::new(world.0 as f32, world.1 as f32), colors::SOUND)
                }
                InteractiveObj::Effect { world, .. } | InteractiveObj::EffectNew { world, .. } => {
                    (Pos2::new(world.0 as f32, world.1 as f32), colors::EFFECT)
                }
            };
            let p = to_screen(pos);
            if rect.contains(p) {
                painter.circle(p, 4.0, color, Stroke::new(1.0, Color32::BLACK));
            }
        }
    }

    if ov.portals {
        for p in &doc.dmap.passages {
            let c = to_screen(doc.cell_to_world(p.x, p.y));
            if !rect.expand(40.0).contains(c) {
                continue;
            }
            // Keep portals readable when zoomed far out.
            let r = (CELL_W / 2.0 * cam.zoom).max(9.0);
            let pts = vec![
                c + Vec2::new(0.0, -r / 2.0),
                c + Vec2::new(r, 0.0),
                c + Vec2::new(0.0, r / 2.0),
                c + Vec2::new(-r, 0.0),
            ];
            painter.add(Shape::convex_polygon(
                pts,
                colors::PORTAL.gamma_multiply(0.8),
                Stroke::new(1.5, Color32::WHITE),
            ));
            painter.text(
                c + Vec2::new(0.0, -r / 2.0 - 2.0),
                egui::Align2::CENTER_BOTTOM,
                format!("Portal {}", p.index),
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );
        }
    }

    let hover_cell = resp.hover_pos().map(|s| doc.world_to_cell(to_world(s)));
    if let Some(cell) = hover_cell.filter(|&c| doc.cell(c).is_some()) {
        let pts = diamond(to_screen(doc.cell_to_world(cell.0, cell.1)));
        painter.add(Shape::closed_line(pts, Stroke::new(2.0, colors::HOVER)));
    }

    ViewResult {
        hover_cell,
        streaming,
    }
}
