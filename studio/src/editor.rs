//! Map editing tools: how pointer input becomes `c3_edit` commands on the document.

use crate::doc::MapDoc;
use crate::icons;
use crate::map_view::Pointer;
use crate::selection::{self, Pickable, Selection};
use c3_edit::{Area, SceneCommand};
use c3_scene::CellPos;
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    /// Look around; left-drag pans.
    Inspect,
    /// Select portals, objects, NPCs and spawn areas; drag to move them.
    Select,
    /// Paint cells blocked.
    Block,
    /// Paint cells walkable.
    Clear,
    /// Add, move (drag) and delete (right-click) portals.
    Portal,
    /// Paint, fill, erase or pick ground tiles.
    Ground,
    /// Paint cells' surface type (footsteps, effects).
    Surface,
    /// Paint cells' height.
    Height,
}

/// What the Ground tool does with a click.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroundMode {
    Paint,
    Fill,
    Erase,
    /// Takes the image under the pointer as the one to paint with.
    Pick,
}

/// The Ground tool's settings.
#[derive(Clone, Debug)]
pub struct GroundBrush {
    pub mode: GroundMode,
    /// The image painted (an asset path, as the tileset names its frames).
    pub image: Option<String>,
}

impl Tool {
    pub const ALL: [Tool; 8] = [
        Tool::Inspect,
        Tool::Select,
        Tool::Block,
        Tool::Clear,
        Tool::Portal,
        Tool::Ground,
        Tool::Surface,
        Tool::Height,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Inspect => "Hand",
            Tool::Select => "Select",
            Tool::Block => "Block",
            Tool::Clear => "Walkable",
            Tool::Portal => "Portal",
            Tool::Ground => "Ground",
            Tool::Surface => "Surface",
            Tool::Height => "Height",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Tool::Inspect => icons::HAND,
            Tool::Select => icons::CURSOR,
            Tool::Block => icons::PROHIBIT,
            Tool::Clear => icons::ERASER,
            Tool::Portal => icons::DOOR_OPEN,
            Tool::Ground => icons::PAINT_BRUSH,
            Tool::Surface => icons::FOOTPRINTS,
            Tool::Height => icons::STAIRS,
        }
    }

    /// Number key that selects the tool.
    pub fn key(self) -> u8 {
        match self {
            Tool::Inspect => 1,
            Tool::Select => 2,
            Tool::Block => 3,
            Tool::Clear => 4,
            Tool::Portal => 5,
            Tool::Ground => 6,
            Tool::Surface => 7,
            Tool::Height => 8,
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Inspect => "Drag to pan, hover to inspect cells",
            Tool::Select => "Click: select · Drag: move · Delete: remove · Esc: deselect",
            Tool::Block => "Drag to mark cells blocked · [ ] brush size",
            Tool::Clear => "Drag to mark cells walkable · [ ] brush size",
            Tool::Portal => "Click: add · Drag: move · Right-click: delete",
            Tool::Ground => {
                "Drag to paint ground tiles · [ ] brush size · pick the image in the Inspector"
            }
            Tool::Surface => {
                "Drag to paint the surface type · Alt+click takes a cell's · [ ] brush size"
            }
            Tool::Height => "Drag to paint the height · Alt+click takes a cell's · [ ] brush size",
        }
    }

    pub fn uses_brush(self) -> bool {
        matches!(
            self,
            Tool::Block | Tool::Clear | Tool::Ground | Tool::Surface | Tool::Height
        )
    }
}

pub const MAX_BRUSH: i32 = 10;

/// Tool state that lives across frames.
pub struct Editor {
    pub tool: Tool,
    /// Brush radius in cells (0 = single cell).
    pub brush: i32,
    /// Portal being dragged: (id, cell it started on).
    dragging_portal: Option<(i32, CellPos)>,
    /// What edits act on; the first is what the Inspector shows.
    selection: Vec<Selection>,
    /// The selection is being dragged from this cell.
    drag_from: Option<CellPos>,
    /// A selection box being dragged out: the world point it started at, and where it reaches.
    boxing: Option<((f32, f32), (f32, f32))>,
    pub ground: GroundBrush,
    /// The parallax layer the Inspector shows (picked in the Hierarchy).
    pub layer: Option<usize>,
    /// What the Surface and Height tools paint.
    pub surface: u16,
    pub height: i16,
    /// Hidden from the Scene view (not drawn, not picked); the map keeps them.
    hidden: HashSet<Selection>,
    /// Drawn but not picked, so work around them can't move them by accident.
    locked: HashSet<Selection>,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            tool: Tool::Select,
            brush: 0,
            dragging_portal: None,
            selection: Vec::new(),
            drag_from: None,
            boxing: None,
            ground: GroundBrush {
                mode: GroundMode::Paint,
                image: None,
            },
            surface: 1,
            height: 0,
            layer: None,
            hidden: HashSet::new(),
            locked: HashSet::new(),
        }
    }
}

impl Editor {
    /// Id of the portal being dragged.
    pub fn dragging_portal(&self) -> Option<i32> {
        self.dragging_portal.map(|(id, _)| id)
    }

    /// What the Inspector shows: the first of the selection.
    pub fn selected(&self) -> Option<Selection> {
        self.selection.first().copied()
    }

    pub fn selection(&self) -> &[Selection] {
        &self.selection
    }

    /// Selects `selection` alone (or nothing).
    pub fn select(&mut self, selection: Option<Selection>) {
        self.select_all(selection.into_iter().collect());
    }

    /// Selects all of `selection` (the first shows in the Inspector).
    pub fn select_all(&mut self, selection: Vec<Selection>) {
        if !selection.is_empty() {
            self.layer = None;
        }
        self.selection = selection;
    }

    /// Drops what is no longer on the map (after an undo).
    pub fn forget_missing(&mut self, scene: &c3_scene::Scene) {
        self.selection.retain(|s| s.exists(scene));
        self.hidden.retain(|s| s.exists(scene));
        self.locked.retain(|s| s.exists(scene));
    }

    /// Objects moved in the list (deleted, reordered): their hide and lock marks no longer
    /// name them, so they go.
    pub fn forget_object_marks(&mut self) {
        let object = |s: &Selection| matches!(s, Selection::Entity(_));
        self.hidden.retain(|s| !object(s));
        self.locked.retain(|s| !object(s));
    }

    pub fn hidden(&self) -> &HashSet<Selection> {
        &self.hidden
    }

    pub fn locked(&self) -> &HashSet<Selection> {
        &self.locked
    }

    /// Hides the selection (and lets go of it), or shows it again when all of it is hidden.
    /// Returns what happened.
    pub fn toggle_hidden(&mut self) -> String {
        let n = self.selection.len();
        if toggle(&mut self.hidden, &self.selection) {
            self.selection.clear();
            format!("Hid {n} · Shift+H shows everything")
        } else {
            format!("Showed {n}")
        }
    }

    /// Locks the selection (and lets go of it), or unlocks it when all of it is locked.
    pub fn toggle_locked(&mut self) -> String {
        let n = self.selection.len();
        if toggle(&mut self.locked, &self.selection) {
            self.selection.clear();
            format!("Locked {n} · Shift+L unlocks everything")
        } else {
            format!("Unlocked {n}")
        }
    }

    /// Shows everything hidden; returns how many.
    pub fn show_all(&mut self) -> usize {
        std::mem::take(&mut self.hidden).len()
    }

    /// Unlocks everything locked; returns how many.
    pub fn unlock_all(&mut self) -> usize {
        std::mem::take(&mut self.locked).len()
    }

    /// Whether pointer picking passes over `s`.
    fn unpickable(&self, s: &Selection) -> bool {
        self.hidden.contains(s) || self.locked.contains(s)
    }

    /// The selection box being dragged out, as world corners.
    pub fn boxing(&self) -> Option<((f32, f32), (f32, f32))> {
        self.boxing
    }

    /// Where the dragged selection would land if dropped on `hover`: its new anchor cell.
    pub fn drag_target(&self, doc: &MapDoc, hover: Option<CellPos>) -> Option<CellPos> {
        let (from, to) = (self.drag_from?, hover?);
        let anchor = self.selected()?.cell(doc.scene())?;
        (from != to).then_some((anchor.0 + to.0 - from.0, anchor.1 + to.1 - from.1))
    }

    pub fn grow_brush(&mut self, delta: i32) {
        self.brush = (self.brush + delta).clamp(0, MAX_BRUSH);
    }

    /// Turns this frame's pointer input into commands on the document. Returns a status
    /// message: what a command did, or why it was refused.
    pub fn handle(&mut self, doc: &mut MapDoc, p: &Pointer, what: Pickable) -> Option<String> {
        match self.tool {
            Tool::Inspect => None,
            Tool::Select => self.handle_select(doc, p, what),
            Tool::Block | Tool::Clear => {
                let blocked = self.tool == Tool::Block;
                // One brush stroke is one undo step.
                if p.primary_pressed {
                    doc.begin_group(if blocked {
                        "Paint blocked"
                    } else {
                        "Paint walkable"
                    });
                }
                if p.primary_down
                    && let Some(center) = p.cell
                {
                    let paint = SceneCommand::SetBlocked {
                        area: Area::Square {
                            center,
                            radius: self.brush,
                        },
                        blocked,
                    };
                    // A brush inside the map always succeeds.
                    let _ = doc.execute(&paint);
                }
                if p.primary_released {
                    doc.end_group();
                }
                None
            }
            Tool::Portal => self.handle_portal(doc, p),
            Tool::Ground => self.handle_ground(doc, p),
            Tool::Surface | Tool::Height => self.handle_cell_value(doc, p),
        }
    }

    fn handle_select(&mut self, doc: &mut MapDoc, p: &Pointer, what: Pickable) -> Option<String> {
        if p.primary_released {
            if let Some((from, to)) = self.boxing.take() {
                return Some(self.finish_box(doc, from, to, p.shift, what));
            }
            let target = self.drag_target(doc, p.cell);
            self.drag_from = None;
            return self.move_selection(doc, target?);
        }
        if p.primary_down
            && let (Some(boxing), Some(world)) = (&mut self.boxing, p.world)
        {
            boxing.1 = world;
        }
        if p.primary_pressed {
            let picked = (p.cell.zip(p.world))
                .and_then(|(cell, world)| {
                    selection::pick(doc.scene(), cell, world, p.pick_radius, what)
                })
                .filter(|s| !self.unpickable(s));
            match picked {
                // Shift adds or takes away; it doesn't drag.
                Some(s) if p.shift => match self.selection.iter().position(|x| *x == s) {
                    Some(i) => {
                        self.selection.remove(i);
                    }
                    None => self.selection.push(s),
                },
                Some(s) => {
                    // Pressing on the selection drags all of it, the pressed one leading.
                    match self.selection.iter().position(|x| *x == s) {
                        Some(i) => self.selection.swap(0, i),
                        None => self.selection = vec![s],
                    }
                    self.drag_from = p.cell;
                }
                None => {
                    if !p.shift {
                        self.selection.clear();
                    }
                    self.boxing = p.world.map(|w| (w, w));
                }
            }
        }
        None
    }

    /// Selects what the box from `a` to `b` holds (`add`: keeps the selection too).
    fn finish_box(
        &mut self,
        doc: &MapDoc,
        a: (f32, f32),
        b: (f32, f32),
        add: bool,
        what: Pickable,
    ) -> String {
        let min = (a.0.min(b.0), a.1.min(b.1));
        let max = (a.0.max(b.0), a.1.max(b.1));
        let found = selection::in_box(doc.scene(), min, max, what);
        if !add {
            self.selection.clear();
        }
        let found: Vec<Selection> = found.into_iter().filter(|s| !self.unpickable(s)).collect();
        for s in found {
            if !self.selection.contains(&s) {
                self.selection.push(s);
            }
        }
        format!("{} selected", self.selection.len())
    }

    /// Moves the selection so its first one stands on `to` (one undo step).
    fn move_selection(&mut self, doc: &mut MapDoc, to: CellPos) -> Option<String> {
        let first = self.selected()?;
        let anchor = first.cell(doc.scene())?;
        let delta = (to.0 - anchor.0, to.1 - anchor.1);
        self.nudge(doc, delta)
    }

    /// Moves the whole selection by `delta` cells (one undo step); returns what happened.
    pub fn nudge(&mut self, doc: &mut MapDoc, delta: CellPos) -> Option<String> {
        let first = self.selected()?;
        let commands = selection::move_all(doc.scene(), &self.selection, delta);
        let label = match self.selection.len() {
            1 => format!("Move {}", first.label(doc.scene())),
            n => format!("Move {n} things"),
        };
        let results = doc.execute_batch(&label, &commands);
        Some(match results.into_iter().find_map(Result::err) {
            Some(e) => format!("Can't move: {e}"),
            None => label,
        })
    }

    /// The Surface and Height brushes: a stroke paints the value; Alt+click takes a cell's.
    fn handle_cell_value(&mut self, doc: &mut MapDoc, p: &Pointer) -> Option<String> {
        let surface = self.tool == Tool::Surface;
        if p.primary_pressed && p.alt {
            let cell = doc.cell(p.cell?)?;
            return Some(if surface {
                self.surface = cell.surface;
                format!("Painting surface {}", cell.surface)
            } else {
                self.height = cell.height;
                format!("Painting height {}", cell.height)
            });
        }
        if p.primary_pressed {
            doc.begin_group(if surface {
                "Paint surface"
            } else {
                "Paint height"
            });
        }
        if p.primary_down
            && !p.alt
            && let Some(center) = p.cell
        {
            let area = Area::Square {
                center,
                radius: self.brush,
            };
            let command = if surface {
                SceneCommand::SetSurface {
                    area,
                    surface: self.surface,
                }
            } else {
                SceneCommand::SetHeight {
                    area,
                    height: self.height,
                }
            };
            // A brush inside the map always succeeds.
            let _ = doc.execute(&command);
        }
        if p.primary_released {
            doc.end_group();
        }
        None
    }

    fn handle_ground(&mut self, doc: &mut MapDoc, p: &Pointer) -> Option<String> {
        let tile = p.world.and_then(|w| ground_tile(doc.scene(), w));
        let image = self.ground.image.clone();
        match self.ground.mode {
            GroundMode::Pick => {
                if p.primary_pressed {
                    let picked = tile.and_then(|t| ground_image(doc.scene(), t))?;
                    self.ground.mode = GroundMode::Paint;
                    self.ground.image = Some(picked.clone());
                    return Some(format!("Painting with {picked}"));
                }
                None
            }
            GroundMode::Fill => {
                if !p.primary_pressed {
                    return None;
                }
                let at = tile?;
                let Some(image) = image else {
                    return Some("Pick an image to fill with in the Inspector".into());
                };
                Some(run(
                    doc,
                    SceneCommand::FillGround {
                        at,
                        image: Some(image),
                    },
                ))
            }
            GroundMode::Paint | GroundMode::Erase => {
                let erase = self.ground.mode == GroundMode::Erase;
                if !erase && image.is_none() {
                    return p
                        .primary_pressed
                        .then(|| "Pick an image to paint with in the Inspector".into());
                }
                // One stroke is one undo step.
                if p.primary_pressed {
                    doc.begin_group(if erase {
                        "Erase ground"
                    } else {
                        "Paint ground"
                    });
                }
                if p.primary_down
                    && let Some((x, y)) = tile
                {
                    let r = self.brush as i64;
                    let tiles = (-r..=r)
                        .flat_map(|dy| (-r..=r).map(move |dx| (dx, dy)))
                        .filter_map(|(dx, dy)| {
                            let (tx, ty) = (i64::from(x) + dx, i64::from(y) + dy);
                            Some((u32::try_from(tx).ok()?, u32::try_from(ty).ok()?))
                        })
                        .collect();
                    let image = if erase { None } else { image };
                    // Tiles off the layer are skipped; a stroke always applies.
                    let _ = doc.execute(&SceneCommand::PaintGround { tiles, image });
                }
                if p.primary_released {
                    doc.end_group();
                }
                None
            }
        }
    }

    fn handle_portal(&mut self, doc: &mut MapDoc, p: &Pointer) -> Option<String> {
        // A drag released outside the map is cancelled.
        if p.primary_released && p.cell.is_none() {
            self.dragging_portal = None;
        }
        let cell = p.cell?;
        let run = |doc: &mut MapDoc, command: SceneCommand| match doc.execute(&command) {
            Ok(outcome) => outcome.summary,
            Err(e) => format!("Can't do that: {e}"),
        };
        if p.secondary_clicked {
            let id = doc.portal_id_at(cell)?;
            return Some(run(doc, SceneCommand::RemovePortal { id }));
        }
        if p.primary_pressed {
            self.dragging_portal = doc.portal_id_at(cell).map(|id| (id, cell));
            if self.dragging_portal.is_none() {
                return Some(run(doc, SceneCommand::AddPortal { cell, id: None }));
            }
        }
        if p.primary_released
            && let Some((id, from)) = self.dragging_portal.take()
            && cell != from
        {
            return Some(run(doc, SceneCommand::MovePortal { id, to: cell }));
        }
        None
    }
}

/// Marks `items` in `set`, or unmarks them when all are marked; true when it marked.
fn toggle(set: &mut HashSet<Selection>, items: &[Selection]) -> bool {
    if !items.is_empty() && items.iter().all(|s| set.contains(s)) {
        for s in items {
            set.remove(s);
        }
        false
    } else {
        set.extend(items.iter().copied());
        true
    }
}

/// Runs `command`; returns what it did or why it was refused.
fn run(doc: &mut MapDoc, command: SceneCommand) -> String {
    match doc.execute(&command) {
        Ok(outcome) => outcome.summary,
        Err(e) => format!("Can't do that: {e}"),
    }
}

/// The ground tile under world point `world`, if the map has ground there.
pub fn ground_tile(scene: &c3_scene::Scene, (wx, wy): (f32, f32)) -> Option<(u32, u32)> {
    let g = scene.ground.as_ref()?;
    let size = g.tile_size.max(1) as f32;
    let x = ((wx - g.origin.0 as f32) / size).floor();
    let y = ((wy - g.origin.1 as f32) / size).floor();
    let inside = x >= 0.0 && y >= 0.0 && (x as u32) < g.width && (y as u32) < g.height;
    inside.then_some((x as u32, y as u32))
}

/// The image shown on ground tile `tile`.
fn ground_image(scene: &c3_scene::Scene, (x, y): (u32, u32)) -> Option<String> {
    let g = scene.ground.as_ref()?;
    let id = (*g.tiles.get((y * g.width + x) as usize)?)?;
    g.tileset.frames.get(&id).cloned()
}
