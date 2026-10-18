//! The World view: every map of the game as a card, the portals between them as arrows
//! (a link both ways drawn once, with two heads). Drag cards to arrange them (kept in the
//! project's `world.ron`), drag the background to pan, scroll to zoom, double-click a card
//! to open its map in the Scene.

use crate::app::Studio;
use crate::icons;
use crate::maps::MapRef;
use crate::shell::Panel;
use crate::theme;
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use std::collections::BTreeMap;

/// Card size and spacing, in world-view units.
const CARD: Vec2 = vec2(170.0, 52.0);
const GAP: Vec2 = vec2(60.0, 50.0);
/// Cards per row when placed automatically.
const PER_ROW: usize = 8;
const LINK: Color32 = Color32::from_rgb(90, 160, 255);

/// One map on the canvas.
struct MapCard {
    key: String,
    id: u32,
    name: String,
    /// Portals leading out.
    exits: usize,
}

#[derive(Default)]
pub(crate) struct WorldView {
    cards: Vec<MapCard>,
    /// Which card leads to which (by index), and how many portals do.
    links: BTreeMap<(usize, usize), usize>,
    /// Where each map's card is, by key (saved).
    places: BTreeMap<String, [f32; 2]>,
    offset: Vec2,
    zoom: f32,
    dragging: Option<usize>,
    /// Read again from the maps on the next frame.
    stale: bool,
    loaded: bool,
    /// The open map and its edit count when last read (its edits show at once).
    seen: Option<(String, usize)>,
    /// Frame the linked maps (else all) on the next frame.
    fit: bool,
}

impl WorldView {
    /// Reads the maps again (their portals changed).
    pub(crate) fn refresh(&mut self) {
        self.stale = true;
    }

    /// Zooms and pans so the linked maps (else all) fill `rect`.
    fn frame(&mut self, rect: Rect) {
        let linked: Vec<usize> = (self.links.keys()).flat_map(|&(a, b)| [a, b]).collect();
        let shown: Vec<usize> = match linked.is_empty() {
            true => (0..self.cards.len()).collect(),
            false => linked,
        };
        let corners = shown.iter().flat_map(|&i| {
            let [x, y] = self.places[&self.cards[i].key];
            [pos2(x, y), pos2(x, y) + CARD]
        });
        let Some(bounds) = corners
            .map(|p| Rect::from_min_max(p, p))
            .reduce(|a, b| a.union(b))
        else {
            return;
        };
        let room = rect.size() - vec2(40.0, 40.0);
        self.zoom = (room.x / bounds.width())
            .min(room.y / bounds.height())
            .clamp(0.2, 1.0);
        // Place the middle of the bounds at the middle of the view (offset is past the
        // view's corner and its 20-point margin).
        let middle = bounds.center().to_vec2() * self.zoom;
        self.offset = rect.size() / 2.0 - vec2(20.0, 20.0) - middle;
    }
}

impl Studio {
    pub(crate) fn world_panel(&mut self, ui: &mut egui::Ui) {
        let seen = (self.doc.as_ref()).map(|d| (d.map.key(), d.history().len()));
        if !self.world.loaded || self.world.stale || seen != self.world.seen {
            self.world.seen = seen;
            self.load_world();
        }
        if self.world.cards.is_empty() {
            theme::empty_state(
                ui,
                icons::GLOBE_SIMPLE,
                "No maps yet",
                "Make one with File > New Map; its portals show here as links.",
            );
            return;
        }
        let mut refresh = false;
        ui.horizontal(|ui| {
            let links: usize = self.world.links.values().sum();
            ui.label(
                egui::RichText::new(format!(
                    "{} maps · {} portal links · double-click a map to open it",
                    self.world.cards.len(),
                    links
                ))
                .color(theme::TEXT_DIM),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                refresh = ui
                    .button(theme::icon_text(icons::ARROW_CLOCKWISE, "Refresh"))
                    .on_hover_text("Read the maps' portals again")
                    .clicked();
                if ui
                    .button(theme::icon_text(icons::CORNERS_OUT, "Fit"))
                    .on_hover_text("Show every linked map")
                    .clicked()
                {
                    self.world.fit = true;
                }
            });
        });
        if refresh {
            self.load_world();
        }
        let open = self.world_canvas(ui);
        if let Some(key) = open {
            self.open_map(&MapRef::parse(&key));
            self.show_panel(Panel::Scene);
        }
    }

    /// Reads every listed map's portal links: a project map's from its scene, a client map's
    /// from its population file (the destinations TQ's data gives).
    fn load_world(&mut self) {
        let w = &mut self.world;
        w.fit |= !w.loaded;
        w.loaded = true;
        w.stale = false;
        w.cards.clear();
        w.links.clear();
        if w.zoom == 0.0 {
            w.zoom = 1.0;
        }
        let Some(project) = &self.project else { return };
        w.places = std::fs::read_to_string(project.root.join(LAYOUT_FILE))
            .ok()
            .and_then(|t| ron::from_str(&t).ok())
            .unwrap_or_default();
        let maps: Vec<_> = self.map_list.iter().cloned().collect();
        let index: BTreeMap<u32, usize> = (maps.iter().enumerate())
            .flat_map(|(i, m)| m.ids.iter().map(move |id| (*id, i)))
            .collect();
        // The open map as edited, others as saved.
        let open = (self.doc.as_ref()).map(|d| (d.map.key().to_lowercase(), d.scene()));
        for (from, m) in maps.iter().enumerate() {
            let edited = open.as_ref().filter(|(k, _)| *k == m.key.to_lowercase());
            let targets = match MapRef::parse(&m.key) {
                _ if edited.is_some() => (edited.expect("checked").1.portals.iter())
                    .filter_map(|p| p.to.map(|t| t.map))
                    .collect(),
                MapRef::Project(name) => c3_scene::native::load_scene(&project.map_file(&name))
                    .map(|s| {
                        s.portals
                            .iter()
                            .filter_map(|p| p.to.map(|t| t.map))
                            .collect()
                    })
                    .unwrap_or_default(),
                MapRef::Client(path) => {
                    let name = crate::maps::client_map_name(&path).to_owned();
                    c3_scene::native::load_population(&project.population_file(&name))
                        .map(|p| p.links.iter().map(|l| l.to.map).collect())
                        .unwrap_or_default()
                }
            };
            let targets: Vec<u32> = targets;
            for to in &targets {
                if let Some(&to) = index.get(to) {
                    *self.world.links.entry((from, to)).or_default() += 1;
                }
            }
            self.world.cards.push(MapCard {
                key: m.key.clone(),
                id: m.id(),
                name: m.name.clone(),
                exits: targets.len(),
            });
        }
        let places = auto_layout(self.world.cards.len(), &self.world.links);
        let w = &mut self.world;
        for (card, place) in w.cards.iter().zip(places) {
            w.places.entry(card.key.clone()).or_insert(place);
        }
    }

    /// Draws the canvas; returns the map to open (a double-clicked card).
    fn world_canvas(&mut self, ui: &mut egui::Ui) -> Option<String> {
        let (rect, background) =
            ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(16));
        let w = &mut self.world;
        if std::mem::take(&mut w.fit) {
            w.frame(rect);
        }
        if background.dragged() && w.dragging.is_none() {
            w.offset += background.drag_delta();
        }
        if background.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                w.zoom = (w.zoom * (scroll * 0.002).exp()).clamp(0.2, 2.5);
            }
        }
        let (offset, zoom) = (rect.min.to_vec2() + vec2(20.0, 20.0) + w.offset, w.zoom);
        let to_screen = |p: [f32; 2]| pos2(p[0], p[1]) * zoom + offset;
        let card_rect = |p: [f32; 2]| Rect::from_min_size(to_screen(p), CARD * zoom);
        let place = |w: &WorldView, i: usize| w.places[&w.cards[i].key];
        // Links under the cards.
        for (&(a, b), &count) in &w.links {
            if a == b || (w.links.contains_key(&(b, a)) && b < a) {
                continue;
            }
            let both = w.links.contains_key(&(b, a));
            let (ra, rb) = (card_rect(place(w, a)), card_rect(place(w, b)));
            let (pa, pb) = (edge_point(ra, rb.center()), edge_point(rb, ra.center()));
            let width = (1.0 + count as f32 * 0.5).min(4.0);
            painter.line_segment([pa, pb], Stroke::new(width, LINK));
            arrow_head(&painter, pa, pb, zoom);
            if both {
                arrow_head(&painter, pb, pa, zoom);
            }
        }
        let mut open = None;
        let mut moved = false;
        for i in 0..w.cards.len() {
            let r = card_rect(place(w, i));
            if !r.intersects(rect) {
                continue;
            }
            let id = ui.make_persistent_id(("world card", i));
            let resp = ui.interact(r, id, Sense::click_and_drag());
            if resp.drag_started() {
                w.dragging = Some(i);
            }
            if resp.dragged() && w.dragging == Some(i) {
                let d = resp.drag_delta() / zoom;
                let p = w.places.get_mut(&w.cards[i].key).expect("placed");
                *p = [p[0] + d.x, p[1] + d.y];
            }
            if resp.drag_stopped() {
                w.dragging = None;
                moved = true;
            }
            if resp.double_clicked() {
                open = Some(w.cards[i].key.clone());
            }
            let card = &w.cards[i];
            let open_here = self.doc.as_ref().is_some_and(|d| d.map.key() == card.key);
            let edge = match (open_here, resp.hovered()) {
                (true, _) => Stroke::new(2.0, theme::ACCENT),
                (false, true) => Stroke::new(1.5, Color32::from_gray(140)),
                _ => Stroke::new(1.0, Color32::from_gray(70)),
            };
            painter.rect(
                r,
                6.0 * zoom,
                Color32::from_rgb(34, 36, 42),
                edge,
                egui::StrokeKind::Inside,
            );
            let text = |y: f32, s: String, size: f32, color: Color32| {
                painter.text(
                    r.min + vec2(10.0, y) * zoom,
                    egui::Align2::LEFT_TOP,
                    s,
                    FontId::proportional(size * zoom),
                    color,
                );
            };
            text(
                8.0,
                format!("{} {}", icons::MAP_TRIFOLD, card.name),
                14.0,
                Color32::WHITE,
            );
            let exits = match card.exits {
                0 => "no portal links".to_owned(),
                1 => "1 portal link".to_owned(),
                n => format!("{n} portal links"),
            };
            text(
                30.0,
                format!("{} · {exits}", card.id),
                11.0,
                theme::TEXT_DIM,
            );
            resp.on_hover_text(&card.key);
        }
        if moved && let Some(project) = &self.project {
            let text =
                ron::ser::to_string_pretty(&w.places, Default::default()).unwrap_or_default();
            // A layout that can't be saved only costs the arrangement.
            let _ = std::fs::write(project.root.join(LAYOUT_FILE), text);
        }
        open
    }
}

/// Where the cards' arrangement is kept, in the project folder.
const LAYOUT_FILE: &str = "world.ron";

/// The point on `r`'s edge towards `to`.
fn edge_point(r: Rect, to: Pos2) -> Pos2 {
    let c = r.center();
    let d = to - c;
    if d == Vec2::ZERO {
        return c;
    }
    let scale = (r.width() / 2.0 / d.x.abs()).min(r.height() / 2.0 / d.y.abs());
    c + d * scale
}

/// An arrow head at `to` pointing away from `from`.
fn arrow_head(painter: &egui::Painter, from: Pos2, to: Pos2, zoom: f32) {
    let dir = (to - from).normalized();
    let side = vec2(-dir.y, dir.x);
    let size = 9.0 * zoom.max(0.5);
    let back = to - dir * size;
    painter.add(egui::Shape::convex_polygon(
        vec![to, back + side * size * 0.5, back - side * size * 0.5],
        LINK,
        Stroke::NONE,
    ));
}

/// Where cards go when the project hasn't placed them: the most linked map in the middle,
/// the maps linked with it on a ring around it, the rest of the linked maps on a wider ring,
/// and maps without links in rows below.
fn auto_layout(count: usize, links: &BTreeMap<(usize, usize), usize>) -> Vec<[f32; 2]> {
    let mut degree = vec![0usize; count];
    for &(a, b) in links.keys() {
        degree[a] += 1;
        degree[b] += 1;
    }
    let mut linked: Vec<usize> = (0..count).filter(|&i| degree[i] > 0).collect();
    linked.sort_by_key(|&i| std::cmp::Reverse(degree[i]));
    let mut places = vec![[0.0; 2]; count];
    let step = CARD + GAP;
    let mut bottom = 0.0f32;
    if let Some((&hub, rest)) = linked.split_first() {
        let near = |i: &usize| links.contains_key(&(hub, *i)) || links.contains_key(&(*i, hub));
        let (inner, outer): (Vec<usize>, Vec<usize>) = rest.iter().partition(|i| near(i));
        let ring = |n: usize, k: f32| (n as f32 * step.x / std::f32::consts::TAU).max(step.x * k);
        let (r1, r2) = (ring(inner.len(), 1.6), ring(outer.len(), 2.8));
        let r2 = r2.max(r1 + step.x);
        let centre = [r2 + step.x, r2 * 0.6 + step.y];
        places[hub] = centre;
        for (ring_maps, r) in [(&inner, r1), (&outer, r2)] {
            for (k, &i) in ring_maps.iter().enumerate() {
                let a = k as f32 / ring_maps.len() as f32 * std::f32::consts::TAU;
                // Isometric-ish: the ring is flattened like the maps.
                places[i] = [centre[0] + r * a.cos(), centre[1] + r * 0.6 * a.sin()];
            }
        }
        bottom = centre[1] + r2 * 0.6 + step.y * 1.5;
    }
    let unlinked = (0..count).filter(|&i| degree[i] == 0);
    for (slot, i) in unlinked.enumerate() {
        let (col, row) = ((slot % PER_ROW) as f32, (slot / PER_ROW) as f32);
        places[i] = [col * step.x, bottom + row * step.y];
    }
    places
}
