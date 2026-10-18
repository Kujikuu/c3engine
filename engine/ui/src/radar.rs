//! The radar in the top-right corner: the ground around the hero, turned like the map on
//! screen, with monsters, NPCs, other players and portals as dots.

use crate::GameView;
use crate::style;
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
    ground(&painter, view, center, hero.cell);
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
    for p in view.sim.players().filter(|p| p.id != view.hero) {
        dot(p.cell, PLAYER, 2.5);
    }
    painter.circle_filled(center, 3.0, Color32::WHITE);
    painter.circle_stroke(center, RADIUS, Stroke::new(1.0, style::EDGE));
    painter.text(
        center + egui::vec2(0.0, RADIUS - 4.0),
        egui::Align2::CENTER_BOTTOM,
        format!("{}, {}", hero.cell.0, hero.cell.1),
        crate::fonts::font(10.0),
        style::DIM,
    );
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
