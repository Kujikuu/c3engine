//! Map viewport: draws the scene as the game does (through the engine's `SceneView`) with
//! editing overlays on top, handles camera input, and reports pointer interaction for the
//! active tool (see `editor`).

use crate::browser::AssetKey;
use crate::data_panel::DraggedRow;
use crate::doc::MapDoc;
use crate::gpu::GpuView;
use crate::icons;
use crate::live_scene::LiveScene;
use crate::selection::Selection;
use c3_play::GameModule;
use c3_render::View;
use c3_scene::{CellPos, EntityKind, Scene};
use eframe::egui::{self, Color32, PointerButton, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use std::collections::HashSet;

pub const MIN_ZOOM: f32 = 0.03;
pub const MAX_ZOOM: f32 = 4.0;
/// How near (screen pixels) the pointer must be to an object's anchor to pick it.
const PICK_RADIUS: f32 = 14.0;
/// Roughly how tall an NPC model stands, in world pixels (where its name goes).
const NPC_HEIGHT: f32 = 110.0;

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
    /// Where a dragged asset would land.
    pub const DROP: Color32 = Color32::from_rgb(90, 200, 255);
    pub const BRUSH_BLOCK: Color32 = Color32::from_rgb(255, 90, 90);
    pub const BRUSH_CLEAR: Color32 = Color32::from_rgb(90, 230, 130);
    pub const NPC: Color32 = Color32::from_rgb(120, 220, 255);
    pub const SPAWN: Color32 = Color32::from_rgb(255, 110, 80);
    pub const SELECTED: Color32 = Color32::from_rgb(255, 255, 255);
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

#[derive(Clone, Copy)]
pub struct Overlays {
    pub grid: bool,
    pub blocked: bool,
    pub portals: bool,
    /// Dots on object, sound and effect anchors.
    pub markers: bool,
    pub npcs: bool,
    /// Monster spawn areas.
    pub spawns: bool,
}

/// How the active tool wants the viewport to behave this frame.
#[derive(Clone, Copy)]
pub struct ViewMode<'a> {
    /// Left-drag pans (Inspect tool). Right/middle-drag always pans.
    pub pan_with_primary: bool,
    /// Brush preview: radius in cells and colour.
    pub brush: Option<(i32, Color32)>,
    /// Portal being dragged; drawn under the cursor.
    /// Id of the portal being dragged.
    pub dragging_portal: Option<i32>,
    /// Animation clock for objects and effects.
    pub time_ms: u64,
    /// What is selected; the first leads a drag.
    pub selection: &'a [Selection],
    /// Where the dragged selection would land (its first one's cell).
    pub drag_target: Option<CellPos>,
    /// A selection box being dragged out (world corners).
    pub boxing: Option<((f32, f32), (f32, f32))>,
    /// Things hidden from the view (the map keeps them).
    pub hidden: &'a HashSet<Selection>,
    /// The Ground tool's brush: tiles on each side of the one under the pointer.
    pub ground_brush: Option<i32>,
}

/// Pointer state over the map this frame.
#[derive(Default)]
pub struct Pointer {
    /// Hovered cell, only if inside the map.
    pub cell: Option<CellPos>,
    /// Hovered world point.
    pub world: Option<(f32, f32)>,
    /// Picking reach in world pixels (a fixed distance on screen).
    pub pick_radius: f32,
    pub primary_pressed: bool,
    pub primary_down: bool,
    pub primary_released: bool,
    pub secondary_clicked: bool,
    /// Shift is held (adds to the selection).
    pub shift: bool,
}

/// What can be dragged onto the map.
pub enum Dropped {
    Asset(AssetKey),
    Row(DraggedRow),
}

impl Dropped {
    /// What the ghost says.
    fn label(&self) -> String {
        match self {
            Self::Asset(key) => key.rel.rsplit('/').next().unwrap_or(&key.rel).to_owned(),
            Self::Row(row) => format!("{} #{}", row.table, row.key),
        }
    }

    /// What is being dragged now, if anything placeable.
    fn carried(ctx: &egui::Context) -> Option<Self> {
        egui::DragAndDrop::payload::<AssetKey>(ctx)
            .map(|k| Self::Asset((*k).clone()))
            .or_else(|| {
                egui::DragAndDrop::payload::<DraggedRow>(ctx).map(|r| Self::Row((*r).clone()))
            })
    }

    /// What was let go over `rect` this frame. (The pointer is read directly: a drag's
    /// preview can cover the view, so it isn't "hovered" when the button goes up.)
    pub(crate) fn released(ui: &egui::Ui, rect: Rect) -> Option<Self> {
        let over = ui.input(|i| {
            i.pointer.primary_released() && i.pointer.hover_pos().is_some_and(|p| rect.contains(p))
        });
        if !over {
            return None;
        }
        // `take_payload` takes whatever is carried, so the type is checked first.
        let carried = Self::carried(ui.ctx());
        if carried.is_some() {
            egui::DragAndDrop::clear_payload(ui.ctx());
        }
        carried
    }
}

pub struct ViewResult {
    pub pointer: Pointer,
    /// What was dropped from the Project or Data panel, and the cell it landed on.
    pub dropped: Option<(Dropped, CellPos)>,
    /// Tiles still waiting to stream in.
    pub streaming: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    game: &dyn GameModule,
    gpu: &mut GpuView,
    live: &mut LiveScene,
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
        doc.scene().projection.cell_w / 2.0 * cam.zoom,
        doc.scene().projection.cell_h / 2.0 * cam.zoom,
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

    let streaming = draw_scene(
        ui,
        &painter,
        rect,
        game,
        gpu,
        live,
        doc,
        View {
            rect: [view.min.x, view.min.y, view.max.x, view.max.y],
            center: (cam.center.x, cam.center.y),
            zoom: cam.zoom,
            time_ms: mode.time_ms,
        },
        mode.hidden,
    );
    draw_cells(&painter, doc, view, rect, ov, cam.zoom, to_screen, diamond);
    if ov.spawns {
        draw_spawns(&painter, doc, mode.hidden, rect, mode.selection, to_screen);
    }
    if ov.markers {
        draw_objects(&painter, doc, mode.hidden, rect, to_screen);
    }
    if ov.npcs {
        let modelled = live.modelled_npcs();
        draw_npcs(&painter, doc, &mode, rect, cam.zoom, &modelled, to_screen);
    }
    draw_selection(&painter, doc, &mode, to_screen, diamond);

    // Pointer.
    // While an asset is dragged in from the Project panel the view isn't "hovered" (the
    // drag owns the pointer), so the pointer is read directly.
    let carried = Dropped::carried(ui.ctx());
    let carrying = carried.is_some();
    let hover_pos = resp
        .hover_pos()
        .or(if resp.is_pointer_button_down_on() || carrying {
            ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()))
                .filter(|p| rect.contains(*p))
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
    // An asset dragged from the Project panel: a ghost on the cell it would land on.
    let dropped = Dropped::released(ui, rect).zip(cell);
    if let (Some(what), Some(c)) = (&carried, cell) {
        let center = to_screen(doc.cell_to_world(c));
        painter.add(Shape::convex_polygon(
            diamond(center, 1.0),
            colors::DROP.gamma_multiply(0.35),
            Stroke::new(2.0, colors::DROP),
        ));
        let name = what.label();
        painter.text(
            center - Vec2::new(0.0, half_h + 6.0),
            egui::Align2::CENTER_BOTTOM,
            format!("{name} · {}, {}", c.0, c.1),
            egui::FontId::proportional(13.0),
            Color32::WHITE,
        );
    }
    let dragging = carrying || dropped.is_some();
    let pointer = Pointer {
        cell,
        world: hover_pos.map(|s| {
            let w = to_world(s);
            (w.x, w.y)
        }),
        pick_radius: PICK_RADIUS / cam.zoom,
        // A drag from the Project panel isn't a click on the map.
        primary_pressed: pressed && resp.hovered() && !dragging,
        primary_down: down && resp.is_pointer_button_down_on() && !dragging,
        primary_released: released && !dragging,
        secondary_clicked: resp.secondary_clicked(),
        shift: ui.input(|i| i.modifiers.shift),
    };

    if ov.portals || mode.dragging_portal.is_some() {
        draw_portals(&painter, doc, &mode, rect, cam.zoom, to_screen, cell);
    }

    // The Ground tool paints whole tiles: outline the ones it would paint.
    if let (Some(r), Some(world), Some(g)) = (
        mode.ground_brush,
        pointer.world,
        doc.scene().ground.as_ref(),
    ) && let Some((x, y)) = crate::editor::ground_tile(doc.scene(), world)
    {
        let size = g.tile_size as f32;
        let corner = |tx: i64, ty: i64| {
            to_screen(Pos2::new(
                g.origin.0 as f32 + tx as f32 * size,
                g.origin.1 as f32 + ty as f32 * size,
            ))
        };
        let (x, y, r) = (i64::from(x), i64::from(y), i64::from(r));
        let area = Rect::from_two_pos(corner(x - r, y - r), corner(x + r + 1, y + r + 1));
        painter.rect(
            area,
            0.0,
            colors::DROP.gamma_multiply(0.12),
            Stroke::new(2.0, colors::DROP),
            egui::StrokeKind::Inside,
        );
    }
    // Brush / hover outline.
    if let Some(c) = cell.filter(|_| mode.ground_brush.is_none()) {
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

    ViewResult {
        pointer,
        dropped,
        streaming,
    }
}

/// Renders the scene with the engine renderer and paints the result.
/// Returns true while art is still streaming in.
#[allow(clippy::too_many_arguments)]
fn draw_scene(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rect: Rect,
    game: &dyn GameModule,
    gpu: &mut GpuView,
    live: &mut LiveScene,
    doc: &MapDoc,
    view: View,
    hidden: &HashSet<Selection>,
) -> bool {
    let shown = without_hidden(doc.scene(), hidden);
    live.prepare(
        shown.as_ref().unwrap_or(doc.scene()),
        game,
        &mut gpu.renderer,
        &view,
    );
    let meshes = live.mesh_draws();
    let frame = live.frame(&meshes, view.rect);
    let ppp = ui.ctx().pixels_per_point();
    let size = [
        (rect.width() * ppp).round() as u32,
        (rect.height() * ppp).round() as u32,
    ];
    let texture = gpu.render(size, &frame);
    painter.image(
        texture,
        rect,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
    let streaming = live.view.streaming();
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
    let cell_px = doc.scene().projection.cell_w * zoom;
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
    let (w, h) = (
        doc.scene().grid.width as i32,
        doc.scene().grid.height as i32,
    );
    let cx0 = (cells.iter().map(|c| c.0).min().unwrap() - 1).max(0);
    let cx1 = (cells.iter().map(|c| c.0).max().unwrap() + 1).min(w - 1);
    let cy0 = (cells.iter().map(|c| c.1).min().unwrap() - 1).max(0);
    let cy1 = (cells.iter().map(|c| c.1).max().unwrap() + 1).min(h - 1);

    let grid_stroke = Stroke::new(1.0, Color32::from_white_alpha((28.0 * grid_alpha) as u8));
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
    hidden: &HashSet<Selection>,
    rect: Rect,
    to_screen: impl Fn(Pos2) -> Pos2,
) {
    for (i, e) in doc.scene().entities.iter().enumerate() {
        if hidden.contains(&Selection::Entity(i)) {
            continue;
        }
        let color = match e.kind {
            EntityKind::Sprite { .. } => colors::COVER,
            EntityKind::Prefab { .. } => colors::TERRAIN,
            EntityKind::Sound { .. } => colors::SOUND,
            EntityKind::Effect { .. } => colors::EFFECT,
        };
        let (x, y) = doc.scene().entity_world(e);
        let p = to_screen(Pos2::new(x, y));
        if rect.contains(p) {
            painter.circle(p, 4.0, color, Stroke::new(1.0, Color32::BLACK));
        }
    }
}

/// Monster spawn areas: the cell rectangle, a diamond-like shape on screen, with its label.
fn draw_spawns(
    painter: &egui::Painter,
    doc: &MapDoc,
    hidden: &HashSet<Selection>,
    rect: Rect,
    selection: &[Selection],
    to_screen: impl Fn(Pos2) -> Pos2,
) {
    for s in &doc.scene().spawns {
        if hidden.contains(&Selection::Spawn(s.id)) {
            continue;
        }
        let corners = cell_rect_corners(doc, s.min, s.max).map(&to_screen);
        if !Rect::from_points(&corners).intersects(rect) {
            continue;
        }
        let chosen = selection.contains(&Selection::Spawn(s.id));
        let stroke = match chosen {
            true => Stroke::new(2.0, colors::SELECTED),
            false => Stroke::new(1.5, colors::SPAWN),
        };
        painter.add(Shape::convex_polygon(
            corners.to_vec(),
            colors::SPAWN.gamma_multiply(if chosen { 0.22 } else { 0.10 }),
            stroke,
        ));
        let label = format!("Monster {} × {}", s.monster, s.max_alive);
        painter.text(
            to_screen(doc.cell_to_world(s.center())),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(12.0),
            Color32::WHITE,
        );
    }
}

/// Screen corners of the cells from `min` to `max` (outer edges, top corner first).
fn cell_rect_corners(doc: &MapDoc, min: CellPos, max: CellPos) -> [Pos2; 4] {
    let origin = doc.cell_to_world((0, 0));
    let ex = doc.cell_to_world((1, 0)) - origin;
    let ey = doc.cell_to_world((0, 1)) - origin;
    let at = |x: f32, y: f32| origin + ex * x + ey * y;
    let (x0, y0) = (min.0 as f32 - 0.5, min.1 as f32 - 0.5);
    let (x1, y1) = (max.0 as f32 + 0.5, max.1 as f32 + 0.5);
    [at(x0, y0), at(x1, y0), at(x1, y1), at(x0, y1)]
}

/// NPC names, and a marker for NPCs whose model the scene can't draw.
fn draw_npcs(
    painter: &egui::Painter,
    doc: &MapDoc,
    mode: &ViewMode,
    rect: Rect,
    zoom: f32,
    modelled: &HashSet<u32>,
    to_screen: impl Fn(Pos2) -> Pos2,
) {
    let (hidden, selection) = (mode.hidden, mode.selection);
    let size = (14.0 * zoom.sqrt()).clamp(9.0, 18.0);
    for n in &doc.scene().npcs {
        if hidden.contains(&Selection::Npc(n.id)) {
            continue;
        }
        let p = to_screen(doc.cell_to_world(n.cell));
        if !rect.expand(60.0).contains(p) {
            continue;
        }
        let chosen = selection.contains(&Selection::Npc(n.id));
        let has_model = modelled.contains(&n.id);
        if !has_model {
            painter.circle(
                p,
                size * 0.75,
                colors::NPC.gamma_multiply(0.85),
                Stroke::new(if chosen { 2.5 } else { 1.0 }, Color32::WHITE),
            );
            painter.text(
                p,
                egui::Align2::CENTER_CENTER,
                icons::USER,
                egui::FontId::proportional(size),
                Color32::from_gray(20),
            );
        }
        // Names float above the head, as the game shows them.
        let above = if has_model { NPC_HEIGHT * zoom } else { size };
        if zoom >= 0.35 || chosen {
            painter.text(
                p + Vec2::new(0.0, -above),
                egui::Align2::CENTER_BOTTOM,
                &n.name,
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );
        }
    }
}

/// Outline around the selected thing and, while dragging, a preview where it would land.
fn draw_selection(
    painter: &egui::Painter,
    doc: &MapDoc,
    mode: &ViewMode,
    to_screen: impl Fn(Pos2) -> Pos2,
    diamond: impl Fn(Pos2, f32) -> Vec<Pos2>,
) {
    let scene = doc.scene();
    if let Some((a, b)) = mode.boxing {
        let corners = Rect::from_two_pos(
            to_screen(Pos2::new(a.0, a.1)),
            to_screen(Pos2::new(b.0, b.1)),
        );
        painter.rect(
            corners,
            0.0,
            colors::SELECTED.gamma_multiply(0.08),
            Stroke::new(1.0, colors::SELECTED),
            egui::StrokeKind::Inside,
        );
    }
    // A drag moves every selected thing as far as the first one goes.
    let shift = (mode.selection.first())
        .and_then(|first| first.cell(scene))
        .zip(mode.drag_target)
        .map(|(anchor, target)| (target.0 - anchor.0, target.1 - anchor.1));
    for &selected in mode.selection {
        let Some(anchor) = selected.cell(scene) else {
            continue;
        };
        // Marks the selection moved by `shift` cells.
        let mark = |shift: CellPos, alpha: f32| {
            let stroke = Stroke::new(2.0, colors::SELECTED.gamma_multiply(alpha));
            let cell = (anchor.0 + shift.0, anchor.1 + shift.1);
            match selected {
                Selection::Entity(i) => {
                    // Objects placed off-grid are marked where they really stand until moved.
                    let world = match scene.entities.get(i) {
                        Some(e) if shift == (0, 0) => {
                            let (x, y) = scene.entity_world(e);
                            Pos2::new(x, y)
                        }
                        _ => doc.cell_to_world(cell),
                    };
                    // The cells a cover object stands on (it sorts by them).
                    if let Some(EntityKind::Sprite { footprint, .. }) =
                        scene.entities.get(i).map(|e| &e.kind)
                    {
                        let (w, h) = (footprint.0.max(1) as i32, footprint.1.max(1) as i32);
                        let min = (cell.0 - w + 1, cell.1 - h + 1);
                        let corners = cell_rect_corners(doc, min, cell).map(&to_screen);
                        let fill = colors::SELECTED.gamma_multiply(0.12 * alpha);
                        painter.add(Shape::convex_polygon(corners.to_vec(), fill, Stroke::NONE));
                    }
                    let p = to_screen(world);
                    painter.circle_stroke(p, 9.0, stroke);
                    painter.circle_filled(p, 3.0, colors::SELECTED.gamma_multiply(alpha));
                }
                Selection::Spawn(id) => {
                    let Some(s) = scene.spawns.iter().find(|s| s.id == id) else {
                        return;
                    };
                    let moved = |c: CellPos| (c.0 + shift.0, c.1 + shift.1);
                    let corners =
                        cell_rect_corners(doc, moved(s.min), moved(s.max)).map(&to_screen);
                    painter.add(Shape::closed_line(corners.to_vec(), stroke));
                }
                Selection::Npc(_) | Selection::Portal(_) => {
                    let center = to_screen(doc.cell_to_world(cell));
                    painter.add(Shape::closed_line(diamond(center, 1.2), stroke));
                }
            }
        };
        mark((0, 0), 1.0);
        if let Some(shift) = shift {
            mark(shift, 0.6);
        }
    }
}

fn draw_portals(
    painter: &egui::Painter,
    doc: &MapDoc,
    mode: &ViewMode,
    rect: Rect,
    zoom: f32,
    to_screen: impl Fn(Pos2) -> Pos2,
    hover: Option<CellPos>,
) {
    let (hidden, dragging) = (mode.hidden, mode.dragging_portal);
    // Keep portals readable when zoomed far out.
    let r = (doc.scene().projection.cell_w / 2.0 * zoom).max(9.0);
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
    for p in &doc.scene().portals {
        if hidden.contains(&Selection::Portal(p.id)) {
            continue;
        }
        let c = to_screen(doc.cell_to_world(p.cell));
        if !rect.expand(40.0).contains(c) {
            continue;
        }
        let dragged = dragging == Some(p.id);
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

/// `scene` without what is hidden, when anything drawn by the renderer is (objects, NPCs).
fn without_hidden(scene: &Scene, hidden: &HashSet<Selection>) -> Option<Scene> {
    let drawn = |s: &&Selection| matches!(s, Selection::Entity(_) | Selection::Npc(_));
    if !hidden.iter().any(|s| drawn(&s)) {
        return None;
    }
    let mut shown = scene.clone();
    let mut index = 0;
    shown.entities.retain(|_| {
        index += 1;
        !hidden.contains(&Selection::Entity(index - 1))
    });
    shown
        .npcs
        .retain(|n| !hidden.contains(&Selection::Npc(n.id)));
    Some(shown)
}
