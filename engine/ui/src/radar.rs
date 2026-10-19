//! The radar in the top-right corner: the ground around the hero, turned like the map on
//! screen, with monsters, NPCs, other players and portals as dots.

use crate::style;
use crate::{GameView, RadarMap};
use c3_scene::CellPos;
use egui::{Color32, Pos2, Stroke};

/// Radius on screen, in points.
const RADIUS: f32 = 64.0;
/// Cells from the hero to the radar's edge.
const RANGE: f32 = 36.0;
/// Room left at the top for the host's corner button.
const TOP: f32 = 52.0;
const MARGIN: f32 = 12.0;

const MONSTER: Color32 = Color32::from_rgb(230, 70, 60);
const NPC: Color32 = Color32::from_rgb(250, 210, 80);
const PLAYER: Color32 = Color32::from_rgb(110, 190, 255);
const PORTAL: Color32 = Color32::from_rgb(170, 120, 255);

pub(crate) fn show(ui: &egui::Ui, view: &GameView) {
    let Some(hero) = view.sim.player(view.hero) else {
        return;
    };
    let center = Pos2::new(
        view.rect.right() - MARGIN - RADIUS,
        view.rect.top() + TOP + RADIUS,
    );
    let painter = ui.painter_at(view.rect);
    painter.circle_filled(center, RADIUS, style::backing(0.85));
    match &view.minimap {
        Some(map) => minimap(&painter, map, center, hero.cell),
        None => ground(&painter, view, center, hero.cell),
    }
    let dot = |cell: CellPos, color: Color32, size: f32| {
        if let Some(at) = place(center, hero.cell, cell) {
            painter.circle_filled(at, size, color);
            painter.circle_stroke(at, size, Stroke::new(0.5, Color32::BLACK));
        }
    };
    for &cell in view.sim.portal_cells() {
        dot(cell, PORTAL, 3.0);
    }
    for npc in view.sim.npcs() {
        dot(npc.cell, NPC, 2.5);
    }
    for m in view.sim.monsters().filter(|m| m.is_alive()) {
        dot(m.cell, MONSTER, 2.0);
    }
    let marks = view.minimap.as_ref().map(|m| m.marks).unwrap_or_default();
    let mark = |at: Pos2, (texture, size): (egui::TextureId, egui::Vec2)| {
        let uv = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        painter.image(
            texture,
            egui::Rect::from_center_size(at, size),
            uv,
            Color32::WHITE,
        );
    };
    for p in view.sim.players().filter(|p| p.id != view.hero) {
        let picture = if p.team == Some(p.id) {
            marks.leader.or(marks.player)
        } else {
            marks.player
        };
        match (picture, place(center, hero.cell, p.cell)) {
            (Some(picture), Some(at)) => mark(at, picture),
            _ => dot(p.cell, PLAYER, 2.5),
        }
    }
    match marks.hero {
        Some(picture) => mark(center, picture),
        None => {
            painter.circle_filled(center, 3.0, Color32::WHITE);
        }
    }
    painter.circle_stroke(center, RADIUS, Stroke::new(1.0, style::EDGE));
    painter.text(
        center + egui::vec2(0.0, RADIUS - 4.0),
        egui::Align2::CENTER_BOTTOM,
        format!("{}, {}", hero.cell.0, hero.cell.1),
        crate::fonts::font(10.0),
        style::DIM,
    );
}

/// The minimap around the hero, cut to the radar's circle.
fn minimap(painter: &egui::Painter, map: &RadarMap, center: Pos2, hero: CellPos) {
    const RIM: usize = 48;
    let [l, t, r, b] = map.bounds;
    let (hx, hy) = map.projection.cell_to_world(hero);
    // Screen points per world pixel, matching the dots (cells `RANGE` away reach the edge).
    let scale = RADIUS / (RANGE * map.projection.cell_w / 2.0);
    let uv = |offset: egui::Vec2| {
        let (wx, wy) = (hx + offset.x / scale, hy + offset.y / scale);
        Pos2::new((wx - l) / (r - l), (wy - t) / (b - t))
    };
    let mut mesh = egui::Mesh::with_texture(map.texture);
    let tint = Color32::from_white_alpha(220);
    mesh.vertices.push(egui::epaint::Vertex {
        pos: center,
        uv: uv(egui::Vec2::ZERO),
        color: tint,
    });
    for i in 0..=RIM {
        let angle = i as f32 / RIM as f32 * std::f32::consts::TAU;
        let offset = egui::vec2(angle.cos(), angle.sin()) * (RADIUS - 1.0);
        mesh.vertices.push(egui::epaint::Vertex {
            pos: center + offset,
            uv: uv(offset),
            color: tint,
        });
        if i > 0 {
            mesh.add_triangle(0, i as u32, i as u32 + 1);
        }
    }
    painter.add(mesh);
}

/// Walkable ground near the hero, sampled every other cell.
fn ground(painter: &egui::Painter, view: &GameView, center: Pos2, hero: CellPos) {
    let grid = view.sim.grid();
    let range = RANGE as i32 + 2;
    let fill = Color32::from_white_alpha(22);
    let size = egui::vec2(2.5, 2.5);
    for dy in (-range..=range).step_by(2) {
        for dx in (-range..=range).step_by(2) {
            let cell = (hero.0 + dx, hero.1 + dy);
            if grid.get(cell).is_none_or(|c| c.is_blocked()) {
                continue;
            }
            if let Some(at) = place(center, hero, cell) {
                painter.rect_filled(egui::Rect::from_center_size(at, size), 0.0, fill);
            }
        }
    }
}

/// Where `cell` shows on the radar centred on `hero`, turned as the isometric map is;
/// `None` past the edge.
fn place(center: Pos2, hero: CellPos, cell: CellPos) -> Option<Pos2> {
    let (dx, dy) = ((cell.0 - hero.0) as f32, (cell.1 - hero.1) as f32);
    let scale = RADIUS / RANGE;
    let offset = egui::vec2((dx - dy) * scale, (dx + dy) * scale / 2.0);
    (offset.length() <= RADIUS - 4.0).then(|| center + offset)
}
