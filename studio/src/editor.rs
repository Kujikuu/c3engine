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
}

impl Tool {
    pub const ALL: [Tool; 5] = [
        Tool::Inspect,
        Tool::Select,
        Tool::Block,
        Tool::Clear,
        Tool::Portal,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Inspect => "Hand",
            Tool::Select => "Select",
            Tool::Block => "Block",
            Tool::Clear => "Walkable",
            Tool::Portal => "Portal",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Tool::Inspect => icons::HAND,
            Tool::Select => icons::CURSOR,
            Tool::Block => icons::PROHIBIT,
            Tool::Clear => icons::ERASER,
            Tool::Portal => icons::DOOR_OPEN,
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
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Inspect => "Drag to pan, hover to inspect cells",
            Tool::Select => "Click: select · Drag: move · Delete: remove · Esc: deselect",
            Tool::Block => "Drag to mark cells blocked · [ ] brush size",
            Tool::Clear => "Drag to mark cells walkable · [ ] brush size",
            Tool::Portal => "Click: add · Drag: move · Right-click: delete",
        }
    }

    pub fn uses_brush(self) -> bool {
        matches!(self, Tool::Block | Tool::Clear)
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
        self.selection = selection.into_iter().collect();
    }

    /// Selects all of `selection` (the first shows in the Inspector).
    pub fn select_all(&mut self, selection: Vec<Selection>) {
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
