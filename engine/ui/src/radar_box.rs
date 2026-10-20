//! The radar as a box in the top-right corner (TQ's CRadar): the game's minimap upright, as
//! an overview squeezed into the box, a crop around the hero, or the whole picture at its own
//! size; it grows out of the button that opens it and shrinks back into it; the hero and the
//! players it knows are marked.

use crate::{GameView, RadarMap, RadarMark};
use c3_game::{EntityId, Player};
use c3_scene::CellPos;
use egui::{Color32, Pos2, Rect, Vec2};

/// How far the box's centre moves each frame while it grows or shrinks (layout pixels,
/// `CRadar::ProcessTransition`).
const GROW_STEP: f32 = 60.0;
/// Milliseconds per frame of the hero's mark (`CRadar::ShowHero`).
const HERO_FRAME_MS: u64 = 30;
/// How faded the picture is: the crop and a single picture at 0xaa, the others at 0x99.
const CROP_ALPHA: u8 = 0xaa;
const WHOLE_ALPHA: u8 = 0x99;

/// The player marks' frames (common.ani `PlayerFlag`).
const FRIEND: usize = 0;
const ENEMY: usize = 1;
const SPOUSE: usize = 2;
const TEAMMATE: usize = 3;

/// How the box shows the map (CRadar's modes).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RadarMode {
    /// Around the hero, at the picture's size.
    Crop,
    /// The whole map squeezed into the box.
    #[default]
    Overview,
    /// The whole picture at its own size, from the top-right corner.
    Full,
}

/// What the box keeps: its mode, whether it shows, and its grow or shrink under way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadarState {
    pub mode: RadarMode,
    pub visible: bool,
    grow: Option<Grow>,
}

impl Default for RadarState {
    fn default() -> Self {
        Self {
            mode: RadarMode::default(),
            visible: true,
            grow: None,
        }
    }
}

/// A grow (opening) or shrink, `done` layout pixels of the way across.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Grow {
    opening: bool,
    done: f32,
}

impl RadarState {
    /// Opens or closes the box, growing out of or shrinking into its button
    /// (`CRadar::SetTransition`); whether it opens.
    pub fn toggle(&mut self) -> bool {
        let opening = !self.visible;
        self.grow = Some(Grow { opening, done: 0.0 });
        self.visible = true;
        opening
    }

    /// Button01: the overview, or back to the crop.
    pub fn toggle_overview(&mut self) {
        self.mode = match self.mode {
            RadarMode::Overview => RadarMode::Crop,
            _ => RadarMode::Overview,
        };
    }

    /// Button02: the full picture, or back to the crop.
    pub fn toggle_full(&mut self) {
        self.mode = match self.mode {
            RadarMode::Full => RadarMode::Crop,
            _ => RadarMode::Full,
        };
    }

    /// The box shown this frame, `target` once grown, moving `step` from `collapsed` (its
    /// button) while it grows or shrinks; none while hidden (`CRadar::ProcessTransition`).
    fn display(&mut self, target: Rect, collapsed: Option<Pos2>, step: f32) -> Option<Rect> {
        if !self.visible {
            return None;
        }
        let (Some(grow), Some(from)) = (&mut self.grow, collapsed) else {
            self.grow = None;
            return Some(target);
        };
        let delta = target.center() - from;
        if delta.x <= 0.0 {
            self.visible = grow.opening;
            self.grow = None;
            return self.visible.then_some(target);
        }
        grow.done = (grow.done + step).min(delta.x);
        let finished = grow.done >= delta.x;
        let part = match grow.opening {
            true => grow.done / delta.x,
            false => 1.0 - grow.done / delta.x,
        };
        if finished {
            self.visible = grow.opening;
            self.grow = None;
            return self.visible.then_some(target);
        }
        Some(Rect::from_center_size(
            from + delta * part,
            target.size() * part,
        ))
    }
}

/// Draws the box; `scale` takes layout pixels to the screen's, `collapsed` is where its
/// button is, `now_ms` animates the hero's mark.
pub(crate) fn show(
    painter: &egui::Painter,
    view: &GameView,
    map: &RadarMap,
    state: &mut RadarState,
    (scale, collapsed, now_ms): (f32, Option<Pos2>, u64),
) {
    let (Some(frame), Some(hero)) = (map.boxed, view.sim.player(view.hero)) else {
        return;
    };
    let corner = Pos2::new(view.rect.right() - frame.x * scale, view.rect.top());
    let target = Rect::from_min_size(corner, frame * scale);
    let Some(display) = state.display(target, collapsed, GROW_STEP * scale) else {
        return;
    };
    let layout = Layout::of(view, map, display, state.mode, scale, hero.cell);
    painter.image(
        map.texture,
        layout.area,
        layout.uv,
        Color32::from_white_alpha(layout.alpha),
    );
    let mark = |cell: CellPos, picture: Option<&RadarMark>| {
        let (Some(&(texture, size)), Some(at)) = (picture, layout.at(map, cell)) else {
            return;
        };
        let full = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        painter.image(
            texture,
            Rect::from_center_size(at, size * scale),
            full,
            Color32::WHITE,
        );
    };
    let hero_frames = &map.marks.hero;
    let hero_frame = (now_ms / HERO_FRAME_MS) as usize % hero_frames.len().max(1);
    mark(hero.cell, hero_frames.get(hero_frame));
    for (cell, picture) in relations(view, map, hero) {
        mark(cell, picture);
    }
}

/// Where the picture lands this frame and how map cells map onto the screen.
struct Layout {
    /// The screen rect the picture (or its crop) fills, and the part of it shown.
    area: Rect,
    uv: Rect,
    alpha: u8,
    /// Screen point of the picture's top-left, and screen pixels per picture pixel.
    origin: Pos2,
    per_pixel: Vec2,
    /// Marks outside this are left out (the crop and overview keep to the box).
    clip: Option<Rect>,
}

impl Layout {
    fn of(
        view: &GameView,
        map: &RadarMap,
        display: Rect,
        mode: RadarMode,
        scale: f32,
        hero: CellPos,
    ) -> Self {
        let whole = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        // A single picture (`CRadar::ShowFallback`) and the full mode sit at their own size
        // from the top-right corner, past the box.
        let own_size = |alpha| {
            let size = map.size * scale;
            let origin = Pos2::new(view.rect.right() - size.x, view.rect.top());
            Self {
                area: Rect::from_min_size(origin, size),
                uv: whole,
                alpha,
                origin,
                per_pixel: Vec2::splat(scale),
                clip: None,
            }
        };
        if map.fallback {
            return own_size(CROP_ALPHA);
        }
        match mode {
            RadarMode::Full => own_size(WHOLE_ALPHA),
            RadarMode::Overview => Self {
                area: display,
                uv: whole,
                alpha: WHOLE_ALPHA,
                origin: display.min,
                per_pixel: display.size() / map.size,
                clip: Some(display),
            },
            RadarMode::Crop => {
                // Around the hero, kept to the picture (`CRadar::ShowCrop`).
                let shown = display.size() / scale;
                let centre = picture_point(map, hero);
                let crop = Vec2::new(
                    (centre.x - shown.x / 2.0).clamp(0.0, (map.size.x - shown.x).max(0.0)),
                    (centre.y - shown.y / 2.0).clamp(0.0, (map.size.y - shown.y).max(0.0)),
                )
                .floor();
                let part = shown.min(map.size);
                Self {
                    area: Rect::from_min_size(display.min, part * scale),
                    uv: Rect::from_min_size((crop / map.size).to_pos2(), part / map.size),
                    alpha: CROP_ALPHA,
                    origin: display.min - crop * scale,
                    per_pixel: Vec2::splat(scale),
                    clip: Some(display),
                }
            }
        }
    }

    /// Where `cell` is drawn, unless outside the box (`CRadar::CellToRadar`, ShowCentered).
    fn at(&self, map: &RadarMap, cell: CellPos) -> Option<Pos2> {
        let at = self.origin + picture_point(map, cell).to_vec2() * self.per_pixel;
        self.clip.is_none_or(|c| c.contains(at)).then_some(at)
    }
}

/// The picture pixel `cell` falls on.
fn picture_point(map: &RadarMap, cell: CellPos) -> Pos2 {
    let [l, t, r, b] = map.bounds;
    let (wx, wy) = map.projection.cell_to_world(cell);
    Pos2::new(
        (wx - l) * map.size.x / (r - l),
        (wy - t) * map.size.y / (b - t),
    )
}

/// The players the hero knows, nearby, with their marks, each once in CRadar's order: the
/// spouse, the team's leader (to its members), teammates, the hero's mentor and students,
/// friends, enemies (`CRadar::ShowRelationships`).
fn relations<'m>(
    view: &GameView,
    map: &'m RadarMap,
    hero: &Player,
) -> Vec<(CellPos, Option<&'m RadarMark>)> {
    let sim = view.sim;
    let range = sim.settings().view_range;
    let near: Vec<&Player> = (sim.players())
        .filter(|p| p.id != hero.id && c3_game::distance(p.cell, hero.cell) <= range)
        .collect();
    let named = |name: &str| near.iter().find(|p| p.name == name).copied();
    let flag = |frame: usize| map.marks.player.get(frame);
    let mut shown: Vec<EntityId> = Vec::new();
    let mut out = Vec::new();
    let mut add = |p: &Player, mark: Option<&'m RadarMark>, keep: bool| {
        if !shown.contains(&p.id) {
            out.push((p.cell, mark));
            if keep {
                shown.push(p.id);
            }
        }
    };
    let spouse = hero.texts.get(&sim.settings().spouse_text);
    if let Some(p) = spouse.and_then(|name| named(name)) {
        add(p, flag(SPOUSE), true);
    }
    if let Some(team) = hero.team.filter(|&t| t != hero.id) {
        if let Some(p) = near.iter().find(|p| p.id == team) {
            add(p, map.marks.leader.first(), false);
        }
        for p in near.iter().filter(|p| p.team == Some(team) && p.id != team) {
            add(p, flag(TEAMMATE), true);
        }
    } else if hero.team == Some(hero.id) {
        for p in near.iter().filter(|p| p.team == Some(hero.id)) {
            add(p, flag(TEAMMATE), true);
        }
    }
    let world = sim.world_state();
    if let Some(p) = (world.mentors.get(&hero.name)).and_then(|m| named(&m.mentor)) {
        add(p, map.marks.tutor.first(), true);
    }
    let students = (world.mentors.iter()).filter(|(_, m)| m.mentor == hero.name);
    for p in students.filter_map(|(student, _)| named(student)) {
        add(p, map.marks.student.first(), true);
    }
    for p in (world.friends_of(&hero.name).iter()).filter_map(|f| named(f)) {
        add(p, flag(FRIEND), true);
    }
    for p in (world.enemies_of(&hero.name).iter()).filter_map(|e| named(e)) {
        add(p, flag(ENEMY), true);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_box_grows_out_of_its_button_and_shrinks_back() {
        let target = Rect::from_min_size(Pos2::new(854.0, 0.0), Vec2::new(170.0, 128.0));
        let button = Some(Pos2::new(72.0, 650.0));
        let mut radar = RadarState {
            visible: false,
            ..Default::default()
        };
        radar.toggle();
        let first = radar.display(target, button, 60.0).unwrap();
        assert!(first.width() < 170.0 && first.center().x < target.center().x);
        while radar.grow.is_some() {
            radar.display(target, button, 60.0);
        }
        assert_eq!(radar.display(target, button, 60.0), Some(target));
        radar.toggle();
        let mut frames = 0;
        while radar.display(target, button, 60.0).is_some() {
            frames += 1;
        }
        assert!(!radar.visible && frames > 10, "{frames}");
    }

    #[test]
    fn the_buttons_swap_with_the_crop() {
        let mut radar = RadarState::default();
        assert_eq!(radar.mode, RadarMode::Overview);
        radar.toggle_overview();
        assert_eq!(radar.mode, RadarMode::Crop);
        radar.toggle_full();
        assert_eq!(radar.mode, RadarMode::Full);
        radar.toggle_full();
        assert_eq!(radar.mode, RadarMode::Crop);
    }
}
