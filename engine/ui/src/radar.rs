//! The radar in the top-right corner: the ground around the hero, turned like the map on
//! screen, with monsters, NPCs, other players and portals as dots; or, as a game asks
//! (TQ's CRadar), its minimap upright in a box, with the hero and its team marked.

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

/// The box's minimap shows faded (CRadar's 0xaa).
const BOX_ALPHA: u8 = 0xaa;

/// Draws the radar; `scale` takes layout pixels to the screen's (a box radar).
pub(crate) fn show(ui: &egui::Ui, view: &GameView, scale: f32) {
    let Some(hero) = view.sim.player(view.hero) else {
        return;
    };
    if let Some(map) = view.minimap.as_ref().filter(|m| m.boxed.is_some()) {
        return boxed(&ui.painter_at(view.rect), view, map, hero, scale);
    }
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

/// The minimap in a box in the top-right corner, upright and at its own size (layout
/// pixels): around the hero, but never past the picture's edges (CRadar::ShowCrop). The hero
/// and its teammates are marked (CRadar::ShowHero, ShowRelationships).
fn boxed(
    painter: &egui::Painter,
    view: &GameView,
    map: &RadarMap,
    hero: &c3_game::Player,
    scale: f32,
) {
    let Some(frame) = map.boxed else {
        return;
    };
    let [l, t, r, b] = map.bounds;
    // Picture pixels per world pixel.
    let (sx, sy) = (map.size.x / (r - l), map.size.y / (b - t));
    let shown = frame.min(map.size);
    let picture = |cell: CellPos| {
        let (wx, wy) = map.projection.cell_to_world(cell);
        egui::vec2((wx - l) * sx, (wy - t) * sy)
    };
    let centre = picture(hero.cell);
    let crop = egui::vec2(
        (centre.x - frame.x / 2.0).clamp(0.0, (map.size.x - frame.x).max(0.0)),
        (centre.y - frame.y / 2.0).clamp(0.0, (map.size.y - frame.y).max(0.0)),
    )
    .floor();
    let corner = Pos2::new(view.rect.right() - frame.x * scale, view.rect.top());
    let uv = egui::Rect::from_min_size((crop / map.size).to_pos2(), shown / map.size);
    let area = egui::Rect::from_min_size(corner, shown * scale);
    painter.image(map.texture, area, uv, Color32::from_white_alpha(BOX_ALPHA));
    let mark = |cell: CellPos, (texture, size): (egui::TextureId, egui::Vec2)| {
        let at = corner + (picture(cell) - crop) * scale;
        if area.contains(at) {
            let full = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            let spot = egui::Rect::from_center_size(at, size * scale);
            painter.image(texture, spot, full, Color32::WHITE);
        }
    };
    let team = hero.team;
    let mates =
        (view.sim.players()).filter(|p| p.id != hero.id && team.is_some() && p.team == team);
    for p in mates {
        let picture = match p.team == Some(p.id) {
            true => map.marks.leader.or(map.marks.player),
            false => map.marks.player,
        };
        if let Some(picture) = picture {
            mark(p.cell, picture);
        }
    }
    if let Some(picture) = map.marks.hero {
        mark(hero.cell, picture);
    }
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
