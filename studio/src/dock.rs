//! A Unity-style docking layout: panels live as tabs in groups; groups are arranged by
//! resizable splits. Drag a tab onto a group to add it there, or onto a group's edge to split
//! it; right-click (or middle-click) a tab to close it. The layout serializes, so it is kept
//! between sessions.

use eframe::egui::{
    self, Color32, CornerRadius, CursorIcon, FontId, Id, Rect, Sense, Stroke, StrokeKind, Vec2,
    pos2, vec2,
};
use serde::{Deserialize, Serialize};

/// Tab bar height.
const TAB_BAR: f32 = 26.0;
/// Width of the draggable gap between split children.
const SPLITTER: f32 = 4.0;
/// Smallest share of a split either child keeps.
const MIN_FRACTION: f32 = 0.08;

/// Colours the dock draws with (kept in one place so the theme owns them).
#[derive(Clone, Copy)]
pub struct DockStyle {
    pub background: Color32,
    pub tab_bar: Color32,
    pub tab_active: Color32,
    pub tab_hover: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub accent: Color32,
}

/// What a dock shows for its tabs.
pub trait TabViewer<T> {
    /// Tab label (may start with an icon).
    fn title(&self, tab: T) -> String;
    fn ui(&mut self, ui: &mut egui::Ui, tab: T);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Node<T> {
    /// Two children side by side (`vertical`: stacked top to bottom); `fraction` is the first
    /// child's share.
    Split {
        vertical: bool,
        fraction: f32,
        first: Box<Node<T>>,
        second: Box<Node<T>>,
    },
    /// A tab group.
    Tabs {
        id: u32,
        tabs: Vec<T>,
        active: usize,
    },
}

impl<T> Node<T> {
    pub fn split(vertical: bool, fraction: f32, first: Node<T>, second: Node<T>) -> Self {
        Self::Split {
            vertical,
            fraction,
            first: Box::new(first),
            second: Box::new(second),
        }
    }
}

/// Where a dragged tab would land on a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Zone {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

enum Action<T> {
    Activate { group: u32, index: usize },
    Resize { path: Vec<bool>, fraction: f32 },
    StartDrag(T),
    Drop { tab: T, group: u32, zone: Zone },
    Close(T),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct Dock<T> {
    root: Node<T>,
    next_id: u32,
    #[serde(skip)]
    dragging: Option<T>,
    /// The layout changed since this was last cleared (save it).
    #[serde(skip)]
    pub changed: bool,
}

impl<T: Copy + PartialEq> Dock<T> {
    /// A dock built by `layout`, which creates groups with the given id generator.
    pub fn new(layout: impl FnOnce(&mut dyn FnMut(Vec<T>) -> Node<T>) -> Node<T>) -> Self {
        let mut next_id = 0;
        let root = layout(&mut |tabs| {
            next_id += 1;
            Node::Tabs {
                id: next_id,
                tabs,
                active: 0,
            }
        });
        Self {
            root,
            next_id,
            dragging: None,
            changed: false,
        }
    }

    pub fn contains(&self, tab: T) -> bool {
        group_of(&self.root, tab).is_some()
    }

    /// Brings `tab` to the front, adding it next to the first of `near` that is open (or to the
    /// first group) if it was closed.
    pub fn show_tab(&mut self, tab: T, near: &[T]) {
        let group = group_of(&self.root, tab)
            .or_else(|| near.iter().find_map(|n| group_of(&self.root, *n)))
            .or_else(|| first_group(&self.root));
        let Some(group) = group else {
            self.next_id += 1;
            self.root = Node::Tabs {
                id: self.next_id,
                tabs: vec![tab],
                active: 0,
            };
            self.changed = true;
            return;
        };
        if let Some(Node::Tabs { tabs, active, .. }) = find_group(&mut self.root, group) {
            if !tabs.contains(&tab) {
                tabs.push(tab);
                self.changed = true;
            }
            *active = tabs.iter().position(|t| *t == tab).unwrap_or(0);
        }
    }

    pub fn close(&mut self, tab: T) {
        if remove_tab(&mut self.root, tab) {
            self.prune();
            self.changed = true;
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, style: &DockStyle, viewer: &mut impl TabViewer<T>) {
        let rect = ui.available_rect_before_wrap();
        ui.painter().rect_filled(rect, 0.0, style.background);
        let mut actions = Vec::new();
        let mut groups = Vec::new();
        let mut frame = Frame {
            style,
            dragging: self.dragging,
            actions: &mut actions,
            groups: &mut groups,
        };
        frame.node(ui, &self.root, rect, &mut Vec::new(), viewer);
        self.drag_and_drop(ui, style, viewer, &groups, &mut actions);
        for action in actions {
            self.apply(action);
        }
        ui.allocate_rect(rect, Sense::hover());
    }

    /// Paints the drop preview and the dragged tab, and drops it on release.
    fn drag_and_drop(
        &mut self,
        ui: &egui::Ui,
        style: &DockStyle,
        viewer: &impl TabViewer<T>,
        groups: &[(u32, Rect)],
        actions: &mut Vec<Action<T>>,
    ) {
        let Some(tab) = self.dragging else {
            return;
        };
        let ctx = ui.ctx();
        let released = ctx.input(|i| i.pointer.any_released());
        let Some(pos) = ctx.pointer_latest_pos() else {
            return;
        };
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            Id::unique("dock-drag"),
        ));
        if let Some(&(group, area)) = groups.iter().find(|(_, r)| r.contains(pos)) {
            let zone = zone_at(area, pos);
            painter.rect(
                preview_rect(area, zone),
                CornerRadius::same(3),
                style.accent.gamma_multiply(0.25),
                Stroke::new(1.5, style.accent),
                StrokeKind::Inside,
            );
            if released {
                actions.push(Action::Drop { tab, group, zone });
            }
        }
        let title = viewer.title(tab);
        let galley = painter.layout_no_wrap(title, FontId::proportional(13.0), style.text);
        let ghost = Rect::from_min_size(pos + vec2(12.0, 8.0), galley.size() + vec2(16.0, 8.0));
        painter.rect_filled(ghost, CornerRadius::same(3), style.tab_active);
        painter.galley(ghost.min + vec2(8.0, 4.0), galley, style.text);
        ctx.set_cursor_icon(CursorIcon::Grabbing);
        if released {
            self.dragging = None;
        }
    }

    fn apply(&mut self, action: Action<T>) {
        match action {
            Action::Activate { group, index } => {
                if let Some(Node::Tabs { active, .. }) = find_group(&mut self.root, group) {
                    *active = index;
                }
            }
            Action::Resize { path, fraction } => {
                if let Some(Node::Split { fraction: f, .. }) = node_at(&mut self.root, &path) {
                    *f = fraction.clamp(MIN_FRACTION, 1.0 - MIN_FRACTION);
                    self.changed = true;
                }
            }
            Action::StartDrag(tab) => self.dragging = Some(tab),
            Action::Close(tab) => self.close(tab),
            Action::Drop { tab, group, zone } => self.drop_tab(tab, group, zone),
        }
    }

    fn drop_tab(&mut self, tab: T, group: u32, zone: Zone) {
        let source = group_of(&self.root, tab);
        let alone = matches!(find_group(&mut self.root, group), Some(Node::Tabs { tabs, .. }) if tabs.len() == 1);
        // Dropping a tab where it already is changes nothing.
        if source == Some(group) && (zone == Zone::Center || alone) {
            return;
        }
        remove_tab(&mut self.root, tab);
        self.prune();
        self.next_id += 1;
        let new_id = self.next_id;
        let Some(target) = find_group(&mut self.root, group) else {
            return;
        };
        match zone {
            Zone::Center => {
                if let Node::Tabs { tabs, active, .. } = target {
                    tabs.push(tab);
                    *active = tabs.len() - 1;
                }
            }
            edge => {
                let old = std::mem::replace(
                    target,
                    Node::Tabs {
                        id: 0,
                        tabs: Vec::new(),
                        active: 0,
                    },
                );
                let new = Node::Tabs {
                    id: new_id,
                    tabs: vec![tab],
                    active: 0,
                };
                let vertical = matches!(edge, Zone::Top | Zone::Bottom);
                let new_first = matches!(edge, Zone::Left | Zone::Top);
                *target = if new_first {
                    Node::split(vertical, 0.5, new, old)
                } else {
                    Node::split(vertical, 0.5, old, new)
                };
            }
        }
        self.changed = true;
    }

    /// Removes empty groups and the splits they leave with one child.
    fn prune(&mut self) {
        let root = std::mem::replace(
            &mut self.root,
            Node::Tabs {
                id: 0,
                tabs: Vec::new(),
                active: 0,
            },
        );
        self.root = prune(root).unwrap_or(Node::Tabs {
            id: 0,
            tabs: Vec::new(),
            active: 0,
        });
    }
}

/// One frame of drawing: collects actions instead of changing the layout while walking it.
struct Frame<'a, T> {
    style: &'a DockStyle,
    dragging: Option<T>,
    actions: &'a mut Vec<Action<T>>,
    /// Every group's rect, for drop targets.
    groups: &'a mut Vec<(u32, Rect)>,
}

impl<T: Copy + PartialEq> Frame<'_, T> {
    fn node(
        &mut self,
        ui: &mut egui::Ui,
        node: &Node<T>,
        rect: Rect,
        path: &mut Vec<bool>,
        viewer: &mut impl TabViewer<T>,
    ) {
        match node {
            Node::Split {
                vertical,
                fraction,
                first,
                second,
            } => {
                let (a, gap, b) = split_rects(rect, *vertical, *fraction);
                path.push(false);
                self.node(ui, first, a, path, viewer);
                path.pop();
                path.push(true);
                self.node(ui, second, b, path, viewer);
                path.pop();
                self.splitter(ui, rect, gap, *vertical, path);
            }
            Node::Tabs { id, tabs, active } => {
                self.groups.push((*id, rect));
                self.group(ui, *id, tabs, *active, rect, viewer);
            }
        }
    }

    fn splitter(&mut self, ui: &egui::Ui, rect: Rect, gap: Rect, vertical: bool, path: &[bool]) {
        let id = ui.scope_id().with(("dock-splitter", path));
        let response = ui.interact(gap, id, Sense::drag());
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(if vertical {
                CursorIcon::ResizeVertical
            } else {
                CursorIcon::ResizeHorizontal
            });
        }
        if response.dragged()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let fraction = if vertical {
                (pos.y - rect.top()) / rect.height()
            } else {
                (pos.x - rect.left()) / rect.width()
            };
            self.actions.push(Action::Resize {
                path: path.to_vec(),
                fraction,
            });
        }
    }

    fn group(
        &mut self,
        ui: &mut egui::Ui,
        id: u32,
        tabs: &[T],
        active: usize,
        rect: Rect,
        viewer: &mut impl TabViewer<T>,
    ) {
        let style = *self.style;
        let bar = Rect::from_min_size(rect.min, vec2(rect.width(), TAB_BAR));
        let content = Rect::from_min_max(pos2(rect.left(), bar.bottom()), rect.max);
        let painter = ui.painter_at(rect);
        painter.rect_filled(bar, 0.0, style.tab_bar);
        painter.rect_filled(content, 0.0, style.tab_active);

        let mut x = bar.left();
        for (index, &tab) in tabs.iter().enumerate() {
            let galley =
                painter.layout_no_wrap(viewer.title(tab), FontId::proportional(13.0), style.text);
            let width = galley.size().x + 22.0;
            let tab_rect = Rect::from_min_size(pos2(x, bar.top()), vec2(width, TAB_BAR));
            x += width + 1.0;
            let response = ui.interact(
                tab_rect,
                ui.scope_id().with(("dock-tab", id, index)),
                Sense::click_and_drag(),
            );
            let is_active = index == active;
            let being_dragged = self.dragging == Some(tab);
            if is_active {
                painter.rect_filled(
                    tab_rect,
                    CornerRadius {
                        nw: 3,
                        ne: 3,
                        sw: 0,
                        se: 0,
                    },
                    style.tab_active,
                );
                painter.line_segment(
                    [
                        tab_rect.left_top() + vec2(1.0, 0.5),
                        tab_rect.right_top() + vec2(-1.0, 0.5),
                    ],
                    Stroke::new(2.0, style.accent),
                );
            } else if response.hovered() {
                painter.rect_filled(
                    tab_rect,
                    CornerRadius {
                        nw: 3,
                        ne: 3,
                        sw: 0,
                        se: 0,
                    },
                    style.tab_hover,
                );
            }
            let color = if is_active && !being_dragged {
                style.text
            } else {
                style.text_dim
            };
            painter.galley(
                tab_rect.left_center() + vec2(11.0, -galley.size().y / 2.0),
                galley,
                color,
            );
            if response.clicked() {
                self.actions.push(Action::Activate { group: id, index });
            }
            if response.drag_started() {
                self.actions.push(Action::StartDrag(tab));
            }
            if response.middle_clicked() {
                self.actions.push(Action::Close(tab));
            }
            let mut close = false;
            response.context_menu(|ui| {
                if ui.button("Close Tab").clicked() {
                    close = true;
                    ui.close();
                }
            });
            if close {
                self.actions.push(Action::Close(tab));
            }
        }

        let Some(&tab) = tabs.get(active.min(tabs.len().saturating_sub(1))) else {
            return;
        };
        let inner = content.shrink2(Vec2::new(6.0, 4.0));
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(egui::Layout::top_down(egui::Align::Min))
                .id_salt(("dock-content", id)),
        );
        child.set_clip_rect(content);
        viewer.ui(&mut child, tab);
    }
}

fn split_rects(rect: Rect, vertical: bool, fraction: f32) -> (Rect, Rect, Rect) {
    if vertical {
        let y = rect.top() + (rect.height() * fraction).round();
        (
            Rect::from_min_max(rect.min, pos2(rect.right(), y - SPLITTER / 2.0)),
            Rect::from_min_max(
                pos2(rect.left(), y - SPLITTER / 2.0),
                pos2(rect.right(), y + SPLITTER / 2.0),
            ),
            Rect::from_min_max(pos2(rect.left(), y + SPLITTER / 2.0), rect.max),
        )
    } else {
        let x = rect.left() + (rect.width() * fraction).round();
        (
            Rect::from_min_max(rect.min, pos2(x - SPLITTER / 2.0, rect.bottom())),
            Rect::from_min_max(
                pos2(x - SPLITTER / 2.0, rect.top()),
                pos2(x + SPLITTER / 2.0, rect.bottom()),
            ),
            Rect::from_min_max(pos2(x + SPLITTER / 2.0, rect.top()), rect.max),
        )
    }
}

/// Middle half of a group = add as tab; otherwise the nearest edge = split there.
fn zone_at(rect: Rect, pos: egui::Pos2) -> Zone {
    let rel = (pos - rect.min) / rect.size();
    if (0.25..=0.75).contains(&rel.x) && (0.25..=0.75).contains(&rel.y) {
        return Zone::Center;
    }
    let distances = [
        (rel.x, Zone::Left),
        (1.0 - rel.x, Zone::Right),
        (rel.y, Zone::Top),
        (1.0 - rel.y, Zone::Bottom),
    ];
    distances
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(Zone::Center, |(_, z)| z)
}

fn preview_rect(rect: Rect, zone: Zone) -> Rect {
    let (w, h) = (rect.width() / 2.0, rect.height() / 2.0);
    match zone {
        Zone::Center => rect.shrink(4.0),
        Zone::Left => Rect::from_min_size(rect.min, vec2(w, rect.height())),
        Zone::Right => Rect::from_min_max(pos2(rect.right() - w, rect.top()), rect.max),
        Zone::Top => Rect::from_min_size(rect.min, vec2(rect.width(), h)),
        Zone::Bottom => Rect::from_min_max(pos2(rect.left(), rect.bottom() - h), rect.max),
    }
}

fn group_of<T: Copy + PartialEq>(node: &Node<T>, tab: T) -> Option<u32> {
    match node {
        Node::Split { first, second, .. } => group_of(first, tab).or_else(|| group_of(second, tab)),
        Node::Tabs { id, tabs, .. } => tabs.contains(&tab).then_some(*id),
    }
}

fn first_group<T>(node: &Node<T>) -> Option<u32> {
    match node {
        Node::Split { first, second, .. } => first_group(first).or_else(|| first_group(second)),
        Node::Tabs { id, .. } => Some(*id),
    }
}

fn find_group<T>(node: &mut Node<T>, group: u32) -> Option<&mut Node<T>> {
    match node {
        Node::Tabs { id, .. } if *id == group => Some(node),
        Node::Tabs { .. } => None,
        Node::Split { first, second, .. } => {
            find_group(first, group).or_else(|| find_group(second, group))
        }
    }
}

fn node_at<'a, T>(node: &'a mut Node<T>, path: &[bool]) -> Option<&'a mut Node<T>> {
    match path.split_first() {
        None => Some(node),
        Some((&second_child, rest)) => match node {
            Node::Split { first, second, .. } => {
                node_at(if second_child { second } else { first }, rest)
            }
            Node::Tabs { .. } => None,
        },
    }
}

fn remove_tab<T: Copy + PartialEq>(node: &mut Node<T>, tab: T) -> bool {
    match node {
        Node::Split { first, second, .. } => remove_tab(first, tab) || remove_tab(second, tab),
        Node::Tabs { tabs, active, .. } => match tabs.iter().position(|t| *t == tab) {
            Some(i) => {
                tabs.remove(i);
                if *active >= tabs.len() {
                    *active = tabs.len().saturating_sub(1);
                }
                true
            }
            None => false,
        },
    }
}

fn prune<T>(node: Node<T>) -> Option<Node<T>> {
    match node {
        Node::Tabs { ref tabs, .. } if tabs.is_empty() => None,
        Node::Tabs { .. } => Some(node),
        Node::Split {
            vertical,
            fraction,
            first,
            second,
        } => match (prune(*first), prune(*second)) {
            (Some(a), Some(b)) => Some(Node::split(vertical, fraction, a, b)),
            (one, None) | (None, one) => one,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dock() -> Dock<char> {
        Dock::new(|group| Node::split(false, 0.3, group(vec!['h']), group(vec!['s', 'g'])))
    }

    fn groups(node: &Node<char>) -> Vec<Vec<char>> {
        match node {
            Node::Split { first, second, .. } => {
                let mut v = groups(first);
                v.extend(groups(second));
                v
            }
            Node::Tabs { tabs, .. } => vec![tabs.clone()],
        }
    }

    #[test]
    fn dropping_moves_tabs_and_empty_groups_collapse() {
        let mut d = dock();
        // 'h' joins the scene group; its own group disappears.
        let target = group_of(&d.root, 's').unwrap();
        d.drop_tab('h', target, Zone::Center);
        assert_eq!(groups(&d.root), [vec!['s', 'g', 'h']]);
        // 'g' splits off to the bottom.
        let target = group_of(&d.root, 's').unwrap();
        d.drop_tab('g', target, Zone::Bottom);
        assert_eq!(groups(&d.root), [vec!['s', 'h'], vec!['g']]);
        assert!(matches!(d.root, Node::Split { vertical: true, .. }));
    }

    #[test]
    fn closing_and_reopening_tabs() {
        let mut d = dock();
        d.close('h');
        assert!(!d.contains('h'));
        assert_eq!(groups(&d.root), [vec!['s', 'g']]);
        d.show_tab('h', &['g']);
        assert_eq!(groups(&d.root), [vec!['s', 'g', 'h']]);
        assert!(d.changed);
    }

    #[test]
    fn drop_zones_follow_the_pointer() {
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0));
        assert_eq!(zone_at(r, pos2(50.0, 50.0)), Zone::Center);
        assert_eq!(zone_at(r, pos2(5.0, 50.0)), Zone::Left);
        assert_eq!(zone_at(r, pos2(50.0, 95.0)), Zone::Bottom);
    }
}
