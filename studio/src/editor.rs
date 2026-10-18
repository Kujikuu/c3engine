//! Map editing tools and how pointer input maps onto document edits.

use crate::doc::{CellPos, MapDoc};
use crate::map_view::Pointer;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    /// Look around; left-drag pans.
    Inspect,
    /// Paint cells blocked.
    Block,
    /// Paint cells walkable.
    Clear,
    /// Add, move (drag) and delete (right-click) portals.
    Portal,
}

impl Tool {
    pub const ALL: [Tool; 4] = [Tool::Inspect, Tool::Block, Tool::Clear, Tool::Portal];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Inspect => "Inspect",
            Tool::Block => "Block",
            Tool::Clear => "Walkable",
            Tool::Portal => "Portal",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Tool::Inspect => "🔍",
            Tool::Block => "⛔",
            Tool::Clear => "✅",
            Tool::Portal => "🌀",
        }
    }

    /// Number key that selects the tool.
    pub fn key(self) -> u8 {
        match self {
            Tool::Inspect => 1,
            Tool::Block => 2,
            Tool::Clear => 3,
            Tool::Portal => 4,
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Inspect => "Drag to pan, hover to inspect cells",
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
    /// Portal being dragged: (index, cell it started on).
    dragging_portal: Option<(usize, CellPos)>,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            tool: Tool::Inspect,
            brush: 0,
            dragging_portal: None,
        }
    }
}

impl Editor {
    pub fn dragging_portal(&self) -> Option<usize> {
        self.dragging_portal.map(|(i, _)| i)
    }

    pub fn grow_brush(&mut self, delta: i32) {
        self.brush = (self.brush + delta).clamp(0, MAX_BRUSH);
    }

    /// Applies this frame's pointer input to the document. Returns a status message, if any.
    pub fn handle(&mut self, doc: &mut MapDoc, p: &Pointer) -> Option<String> {
        match self.tool {
            Tool::Inspect => None,
            Tool::Block | Tool::Clear => {
                if p.primary_down
                    && let Some(cell) = p.cell
                {
                    doc.paint(cell, self.brush, self.tool == Tool::Block);
                }
                if p.primary_released {
                    doc.end_stroke();
                }
                None
            }
            Tool::Portal => self.handle_portal(doc, p),
        }
    }

    fn handle_portal(&mut self, doc: &mut MapDoc, p: &Pointer) -> Option<String> {
        // A drag released outside the map is cancelled.
        if p.primary_released && p.cell.is_none() {
            self.dragging_portal = None;
        }
        let cell = p.cell?;
        if p.secondary_clicked {
            let i = doc.portal_at(cell)?;
            let index = doc.dmap.passages[i].index;
            doc.remove_portal(i);
            return Some(format!("Deleted portal {index}"));
        }
        if p.primary_pressed {
            self.dragging_portal = doc.portal_at(cell).map(|i| (i, cell));
            if self.dragging_portal.is_none() && doc.add_portal(cell) {
                let index = doc.dmap.passages.last().map_or(0, |p| p.index);
                return Some(format!("Added portal {index} at {}, {}", cell.0, cell.1));
            }
        }
        if p.primary_released
            && let Some((i, from)) = self.dragging_portal.take()
            && cell != from
            && doc.move_portal(i, cell)
        {
            return Some(format!("Moved portal to {}, {}", cell.0, cell.1));
        }
        None
    }
}
