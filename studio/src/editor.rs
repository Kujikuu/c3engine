//! Map editing tools: how pointer input becomes `c3_edit` commands on the document.

use crate::doc::MapDoc;
use crate::map_view::Pointer;
use c3_edit::{Area, SceneCommand};
use c3_scene::CellPos;

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
    /// Portal being dragged: (id, cell it started on).
    dragging_portal: Option<(i32, CellPos)>,
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
    /// Id of the portal being dragged.
    pub fn dragging_portal(&self) -> Option<i32> {
        self.dragging_portal.map(|(id, _)| id)
    }

    pub fn grow_brush(&mut self, delta: i32) {
        self.brush = (self.brush + delta).clamp(0, MAX_BRUSH);
    }

    /// Turns this frame's pointer input into commands on the document. Returns a status
    /// message: what a command did, or why it was refused.
    pub fn handle(&mut self, doc: &mut MapDoc, p: &Pointer) -> Option<String> {
        match self.tool {
            Tool::Inspect => None,
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
