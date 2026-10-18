//! Map viewport: draws ground tiles and overlays, handles camera input, and reports
//! pointer interaction for the active tool (see `editor`).

use crate::doc::MapDoc;
use crate::gpu::GpuView;
use crate::tq_plugin::Client;
use c3_render::Frame;
use c3_render::ground::{self, GroundCache};
use c3_scene::{CellPos, EntityKind};
use eframe::egui::{self, Color32, PointerButton, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use glam::camera::rh::proj::directx;

pub const MIN_ZOOM: f32 = 0.03;
pub const MAX_ZOOM: f32 = 4.0;

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
    pub const BRUSH_BLOCK: Color32 = Color32::from_rgb(255, 90, 90);
    pub const BRUSH_CLEAR: Color32 = Color32::from_rgb(90, 230, 130);
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

/// How the active tool wants the viewport to behave this frame.
#[derive(Default, Clone, Copy)]
pub struct ViewMode {
    /// Left-drag pans (Inspect tool). Right/middle-drag always pans.
    pub pan_with_primary: bool,
    /// Brush preview: radius in cells and colour.
    pub brush: Option<(i32, Color32)>,
    /// Portal being dragged; drawn under the cursor.
    pub dragging_portal: Option<usize>,
}

/// Pointer state over the map this frame.
#[derive(Default)]
pub struct Pointer {
    /// Hovered cell, only if inside the map.
    pub cell: Option<CellPos>,
    pub primary_pressed: bool,
    pub primary_down: bool,
    pub primary_released: bool,
    pub secondary_clicked: bool,
}

pub struct ViewResult {
    pub pointer: Pointer,
    /// Tiles still waiting to stream in.
    pub streaming: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    client: &Client,
    gpu: &mut GpuView,
    ground: &mut GroundCache,
    doc: &mut MapDoc,
    cam: &mut Camera,
    ov: Overlays,
    mode: ViewMode,
) -> ViewResult {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    cam.viewport = rect.size();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_gray(14));

    let panning = resp.dragged_by(PointerButton::Middle)
        || resp.dragged_by(PointerButton::Secondary)
        || (mode.pan_with_primary && resp.dragged_by(PointerButton::Primary));
    if panning {
        cam.center -= resp.drag_delta() / cam.zoom;
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if resp.hovered() {
        ui.ctx().set_cursor_icon(if mode.pan_with_primary {
            egui::CursorIcon::Grab
        } else {
            egui::CursorIcon::Crosshair
        });
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
    let (half_w, half_h) = (
        doc.scene.projection.cell_w / 2.0 * cam.zoom,
        doc.scene.projection.cell_h / 2.0 * cam.zoom,
    );
    let diamond = move |c: Pos2, scale: f32| {
        let (hw, hh) = (half_w * scale, half_h * scale);
        vec![
            c + Vec2::new(0.0, -hh),
            c + Vec2::new(hw, 0.0),
            c + Vec2::new(0.0, hh),
            c + Vec2::new(-hw, 0.0),
        ]
    };

    let streaming = draw_ground(ui, &painter, rect, client, gpu, ground, doc, view, cam.zoom);
    draw_cells(&painter, doc, view, rect, ov, cam.zoom, to_screen, diamond);
    if ov.objects {
        draw_objects(&painter, doc, rect, to_screen);
    }

    // Pointer.
    let hover_pos = resp.hover_pos().or(if resp.is_pointer_button_down_on() {
        ui.input(|i| i.pointer.interact_pos())
    } else {
        None
    });
    let cell = hover_pos
        .map(|s| doc.world_to_cell(to_world(s)))
        .filter(|&c| doc.cell(c).is_some());
    let (pressed, down, released) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.primary_released(),
        )
    });
    let pointer = Pointer {
        cell,
        primary_pressed: pressed && resp.hovered(),
        primary_down: down && resp.is_pointer_button_down_on(),
        primary_released: released,
        secondary_clicked: resp.secondary_clicked(),
    };

    if ov.portals || mode.dragging_portal.is_some() {
        draw_portals(
            &painter,
            doc,
            rect,
            cam.zoom,
            to_screen,
            mode.dragging_portal,
            cell,
        );
    }

    // Brush / hover outline.
    if let Some(c) = cell {
        let center = to_screen(doc.cell_to_world(c));
        match mode.brush {
            Some((r, color)) => {
                // A square brush in cell space is a diamond on screen.
                let pts = diamond(center, (2 * r + 1) as f32);
                painter.add(Shape::convex_polygon(
                    pts,
                    color.gamma_multiply(0.18),
                    Stroke::new(2.0, color),
                ));
            }
            None => {
                painter.add(Shape::closed_line(
                    diamond(center, 1.0),
                    Stroke::new(2.0, colors::HOVER),
                ));
            }
        }
    }

    ViewResult { pointer, streaming }
}

/// Renders visible ground tiles with the engine renderer and paints the result.
/// Returns true while tiles are still streaming in.
#[allow(clippy::too_many_arguments)]
fn draw_ground(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rect: Rect,
    client: &Client,
    gpu: &mut GpuView,
    cache: &mut GroundCache,
    doc: &MapDoc,
    view: Rect,
    zoom: f32,
) -> bool {
    // Half a screen pixel of overlap hides seams between tiles.
    let ground = match doc.scene.ground.as_ref() {
        Some(layer) => ground::sprites(
            layer,
            [view.min.x, view.min.y, view.max.x, view.max.y],
            0.5 / zoom,
            cache,
            &mut gpu.renderer,
            ground::DEFAULT_DECODE_BUDGET,
            |path| client.load_image(path),
        ),
        None => ground::GroundSprites {
            sprites: Vec::new(),
            streaming: false,
        },
    };
    let (sprites, streaming) = (ground.sprites, ground.streaming);
    let ppp = ui.ctx().pixels_per_point();
    let size = [
        (rect.width() * ppp).round() as u32,
        (rect.height() * ppp).round() as u32,
    ];
    let frame = Frame {
        clear: [0.0044, 0.0044, 0.0044, 1.0],
        sprite_view: directx::orthographic(
            view.min.x, view.max.x, view.max.y, view.min.y, -1.0, 1.0,
        ),
        sprites: &sprites,
        ..Default::default()
    };
    let texture = gpu.render(size, &frame);
    painter.image(
        texture,
        rect,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
    if streaming {
        ui.ctx().request_repaint();
    }
    streaming
}

#[allow(clippy::too_many_arguments)]
fn draw_cells(
    painter: &egui::Painter,
    doc: &MapDoc,
    view: Rect,
    rect: Rect,
    ov: Overlays,
    zoom: f32,
    to_screen: impl Fn(Pos2) -> Pos2,
    diamond: impl Fn(Pos2, f32) -> Vec<Pos2>,
) {
    let cell_px = doc.scene.projection.cell_w * zoom;
    // Grid fades in between 24 and 48 px per cell so it never turns into noise.
    let grid_alpha = ((cell_px - 24.0) / 24.0).clamp(0.0, 1.0);
    let show_blocked = ov.blocked && cell_px >= 3.0;
    let show_grid = ov.grid && grid_alpha > 0.0;
    if !show_blocked && !show_grid {
        return;
    }
    // Visible cell range: invert the four view corners.
    let corners = [
        view.min,
        Pos2::new(view.max.x, view.min.y),
        view.max,
        Pos2::new(view.min.x, view.max.y),
    ];
    let cells: Vec<_> = corners.iter().map(|&p| doc.world_to_cell(p)).collect();
    let (w, h) = (doc.scene.grid.width as i32, doc.scene.grid.height as i32);
    let cx0 = (cells.iter().map(|c| c.0).min().unwrap() - 1).max(0);
    let cx1 = (cells.iter().map(|c| c.0).max().unwrap() + 1).min(w - 1);
    let cy0 = (cells.iter().map(|c| c.1).min().unwrap() - 1).max(0);
    let cy1 = (cells.iter().map(|c| c.1).max().unwrap() + 1).min(h - 1);

    let grid_stroke = Stroke::new(1.0, Color32::from_white_alpha((45.0 * grid_alpha) as u8));
    for y in cy0..=cy1 {
        for x in cx0..=cx1 {
            let c = to_screen(doc.cell_to_world((x, y)));
            if !rect.expand(cell_px).contains(c) {
                continue;
            }
            let pts = diamond(c, 1.0);
            if show_blocked && doc.cell((x, y)).is_some_and(|c| c.is_blocked()) {
                painter.add(Shape::convex_polygon(
                    pts.clone(),
                    colors::BLOCKED,
                    Stroke::NONE,
                ));
            }
            if show_grid {
                painter.add(Shape::closed_line(pts, grid_stroke));
            }
        }
    }
}

fn draw_objects(
    painter: &egui::Painter,
    doc: &MapDoc,
    rect: Rect,
    to_screen: impl Fn(Pos2) -> Pos2,
) {
    for e in &doc.scene.entities {
        let color = match e.kind {
            EntityKind::Sprite { .. } => colors::COVER,
            EntityKind::Prefab { .. } => colors::TERRAIN,
            EntityKind::Sound { .. } => colors::SOUND,
            EntityKind::Effect { .. } => colors::EFFECT,
        };
        let (x, y) = doc.scene.entity_world(e);
        let p = to_screen(Pos2::new(x, y));
        if rect.contains(p) {
            painter.circle(p, 4.0, color, Stroke::new(1.0, Color32::BLACK));
        }
    }
}

fn draw_portals(
    painter: &egui::Painter,
    doc: &MapDoc,
    rect: Rect,
    zoom: f32,
    to_screen: impl Fn(Pos2) -> Pos2,
    dragging: Option<usize>,
    hover: Option<CellPos>,
) {
    // Keep portals readable when zoomed far out.
    let r = (doc.scene.projection.cell_w / 2.0 * zoom).max(9.0);
    let marker = |c: Pos2, fill: Color32, label: String| {
        let pts = vec![
            c + Vec2::new(0.0, -r / 2.0),
            c + Vec2::new(r, 0.0),
            c + Vec2::new(0.0, r / 2.0),
            c + Vec2::new(-r, 0.0),
        ];
        painter.add(Shape::convex_polygon(
            pts,
            fill,
            Stroke::new(1.5, Color32::WHITE),
        ));
        painter.text(
            c + Vec2::new(0.0, -r / 2.0 - 2.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            egui::FontId::proportional(12.0),
            Color32::WHITE,
        );
    };
    for (i, p) in doc.scene.portals.iter().enumerate() {
        let c = to_screen(doc.cell_to_world(p.cell));
        if !rect.expand(40.0).contains(c) {
            continue;
        }
        let dragged = dragging == Some(i);
        marker(
            c,
            colors::PORTAL.gamma_multiply(if dragged { 0.3 } else { 0.8 }),
            format!("Portal {}", p.id),
        );
        if dragged && let Some(h) = hover {
            marker(
                to_screen(doc.cell_to_world(h)),
                colors::PORTAL,
                format!("Portal {}", p.id),
            );
        }
    }
}
