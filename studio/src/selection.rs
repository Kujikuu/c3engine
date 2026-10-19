//! What the Scene view has selected (portals, objects, NPCs and spawn areas), how to pick it
//! under the pointer or in a box, the commands that move or delete it, and copies of it on
//! the clipboard (pasted on this map or another).

use c3_edit::{SceneCommand, entity_label};
use c3_scene::{CellPos, Entity, EntityKind, Npc, Placement, PortalTarget, Region, Scene, Spawn};
use std::collections::HashSet;

/// One selected thing on the map, by the id (or index) commands use.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Selection {
    Portal(i32),
    /// Index in the scene's entity list.
    Entity(usize),
    Npc(u32),
    Spawn(u32),
    Region(u32),
}

impl Selection {
    /// Whether it is still on the map (undo may have removed it).
    pub fn exists(self, scene: &Scene) -> bool {
        match self {
            Self::Portal(id) => scene.portals.iter().any(|p| p.id == id),
            Self::Entity(i) => i < scene.entities.len(),
            Self::Npc(id) => scene.npcs.iter().any(|n| n.id == id),
            Self::Spawn(id) => spawn(scene, id).is_some(),
            Self::Region(id) => region(scene, id).is_some(),
        }
    }

    /// The cell it stands on (a spawn area's middle).
    pub fn cell(self, scene: &Scene) -> Option<CellPos> {
        match self {
            Self::Portal(id) => scene.portals.iter().find(|p| p.id == id).map(|p| p.cell),
            Self::Entity(i) => scene.entities.get(i).map(|e| match e.placement {
                Placement::Cell(c) => c,
                Placement::World((x, y)) => scene.world_to_cell((x as f32, y as f32)),
            }),
            Self::Npc(id) => scene.npcs.iter().find(|n| n.id == id).map(|n| n.cell),
            Self::Spawn(id) => spawn(scene, id).map(Spawn::center),
            Self::Region(id) => region(scene, id).map(Region::center),
        }
    }

    /// A short name for status messages and the Inspector title.
    pub fn label(self, scene: &Scene) -> String {
        match self {
            Self::Portal(id) => format!("Portal {id}"),
            Self::Entity(i) => scene
                .entities
                .get(i)
                .map_or_else(|| format!("Object {i}"), entity_label),
            Self::Npc(id) => scene
                .npcs
                .iter()
                .find(|n| n.id == id)
                .map_or_else(|| format!("NPC {id}"), |n| n.name.clone()),
            Self::Spawn(id) => format!("Spawn area {id}"),
            Self::Region(id) => {
                region(scene, id).map_or_else(|| format!("Region {id}"), Region::label)
            }
        }
    }

    /// The command that puts it on `to` (a spawn area moves by its middle).
    pub fn move_to(self, scene: &Scene, to: CellPos) -> Option<SceneCommand> {
        Some(match self {
            Self::Portal(id) => SceneCommand::MovePortal { id, to },
            Self::Entity(index) => SceneCommand::MoveEntity { index, to },
            Self::Npc(id) => SceneCommand::MoveNpc { id, to },
            Self::Spawn(id) => {
                let mut spawn = spawn(scene, id)?.clone();
                let from = spawn.center();
                let (dx, dy) = (to.0 - from.0, to.1 - from.1);
                spawn.min = (spawn.min.0 + dx, spawn.min.1 + dy);
                spawn.max = (spawn.max.0 + dx, spawn.max.1 + dy);
                SceneCommand::SetSpawn { spawn }
            }
            Self::Region(id) => {
                let mut region = region(scene, id)?.clone();
                let from = region.center();
                let (dx, dy) = (to.0 - from.0, to.1 - from.1);
                region.min = (region.min.0 + dx, region.min.1 + dy);
                region.max = (region.max.0 + dx, region.max.1 + dy);
                SceneCommand::SetRegion { region }
            }
        })
    }

    pub fn remove(self) -> SceneCommand {
        match self {
            Self::Portal(id) => SceneCommand::RemovePortal { id },
            Self::Entity(index) => SceneCommand::RemoveEntity { index },
            Self::Npc(id) => SceneCommand::RemoveNpc { id },
            Self::Spawn(id) => SceneCommand::RemoveSpawn { id },
            Self::Region(id) => SceneCommand::RemoveRegion { id },
        }
    }
}

/// The commands that delete `selection` (objects last-first, so indices stay right).
pub fn remove_all(selection: &[Selection]) -> Vec<SceneCommand> {
    let mut sorted = selection.to_vec();
    sorted.sort_by_key(|s| match s {
        Selection::Entity(i) => std::cmp::Reverse(*i),
        _ => std::cmp::Reverse(usize::MAX),
    });
    sorted.into_iter().map(Selection::remove).collect()
}

/// The commands that move `selection` by `delta` cells.
pub fn move_all(scene: &Scene, selection: &[Selection], delta: CellPos) -> Vec<SceneCommand> {
    (selection.iter())
        .filter_map(|s| {
            let at = s.cell(scene)?;
            s.move_to(scene, (at.0 + delta.0, at.1 + delta.1))
        })
        .collect()
}

/// The commands that shift the selected sprites' images by `delta` pixels (others stay).
pub fn nudge_pixels(
    scene: &Scene,
    selection: &[Selection],
    delta: (i32, i32),
) -> Vec<SceneCommand> {
    (selection.iter())
        .filter_map(|s| {
            let Selection::Entity(index) = *s else {
                return None;
            };
            let mut entity = scene.entities.get(index)?.clone();
            let EntityKind::Sprite { offset, .. } = &mut entity.kind else {
                return None;
            };
            *offset = (offset.0 + delta.0, offset.1 + delta.1);
            Some(SceneCommand::SetEntity { index, entity })
        })
        .collect()
}

/// The commands that draw the selected objects over (`front`) or under every other object
/// on the same spot, and the selection afterwards.
pub fn reorder(
    scene: &Scene,
    selection: &[Selection],
    front: bool,
) -> (Vec<SceneCommand>, Vec<Selection>) {
    let mut indices: Vec<usize> = (selection.iter())
        .filter_map(|s| match *s {
            Selection::Entity(i) if i < scene.entities.len() => Some(i),
            _ => None,
        })
        .collect();
    indices.sort_unstable();
    let count = indices.len();
    let mut commands: Vec<SceneCommand> = (indices.iter().rev())
        .map(|&index| SceneCommand::RemoveEntity { index })
        .collect();
    let first = if front {
        scene.entities.len() - count
    } else {
        0
    };
    commands.extend(
        indices
            .iter()
            .enumerate()
            .map(|(k, &i)| SceneCommand::AddEntity {
                entity: scene.entities[i].clone(),
                index: Some(first + k),
            }),
    );
    let others = selection
        .iter()
        .filter(|s| !matches!(s, Selection::Entity(_)));
    let selected = (first..first + count)
        .map(Selection::Entity)
        .chain(others.copied());
    (commands, selected.collect())
}

/// A line the selection can be lined up on, through its first one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Line {
    /// Level on screen: the same height.
    Row,
    /// Upright on screen: straight above and below each other.
    Column,
}

/// The commands that move the rest of the selection onto `line` through the first one,
/// each the shortest way (cells on a diamond grid meet a line every other step, so some
/// land one cell beside it).
pub fn align(scene: &Scene, selection: &[Selection], line: Line) -> Vec<SceneCommand> {
    let Some(anchor) = selection.first().and_then(|s| s.cell(scene)) else {
        return Vec::new();
    };
    (selection.iter().skip(1))
        .filter_map(|s| {
            let (x, y) = s.cell(scene)?;
            let to = match line {
                // Screen height follows x + y; moving by (t, t) keeps x - y.
                Line::Row => {
                    let t = ((anchor.0 + anchor.1) - (x + y)).div_euclid(2);
                    (x + t, y + t)
                }
                // Screen x follows x - y; moving by (t, -t) keeps x + y.
                Line::Column => {
                    let t = ((anchor.0 - anchor.1) - (x - y)).div_euclid(2);
                    (x + t, y - t)
                }
            };
            ((x, y) != to).then(|| s.move_to(scene, to)).flatten()
        })
        .collect()
}

/// Everything anchored inside the world box `min`..`max` (of the kinds `what` picks).
pub fn in_box(scene: &Scene, min: (f32, f32), max: (f32, f32), what: Pickable) -> Vec<Selection> {
    let inside = |(x, y): (f32, f32)| x >= min.0 && x <= max.0 && y >= min.1 && y <= max.1;
    let mut found = Vec::new();
    if what.portals {
        let portals = scene
            .portals
            .iter()
            .filter(|p| inside(scene.cell_to_world(p.cell)));
        found.extend(portals.map(|p| Selection::Portal(p.id)));
    }
    if what.objects {
        let objects =
            (scene.entities.iter().enumerate()).filter(|(_, e)| inside(scene.entity_world(e)));
        found.extend(objects.map(|(i, _)| Selection::Entity(i)));
    }
    if what.npcs {
        let npcs = scene
            .npcs
            .iter()
            .filter(|n| inside(scene.cell_to_world(n.cell)));
        found.extend(npcs.map(|n| Selection::Npc(n.id)));
    }
    if what.spawns {
        let spawns = (scene.spawns.iter()).filter(|s| inside(scene.cell_to_world(s.center())));
        found.extend(spawns.map(|s| Selection::Spawn(s.id)));
    }
    if what.regions {
        let regions = (scene.regions.iter()).filter(|r| inside(scene.cell_to_world(r.center())));
        found.extend(regions.map(|r| Selection::Region(r.id)));
    }
    found
}

/// What a map holds, to find what commands added to it.
pub struct Census {
    entities: usize,
    portals: HashSet<i32>,
    npcs: HashSet<u32>,
    spawns: HashSet<u32>,
    regions: HashSet<u32>,
}

impl Census {
    pub fn of(scene: &Scene) -> Self {
        Self {
            entities: scene.entities.len(),
            portals: scene.portals.iter().map(|p| p.id).collect(),
            npcs: scene.npcs.iter().map(|n| n.id).collect(),
            spawns: scene.spawns.iter().map(|s| s.id).collect(),
            regions: scene.regions.iter().map(|r| r.id).collect(),
        }
    }

    /// What `scene` holds that it didn't.
    pub fn added(&self, scene: &Scene) -> Vec<Selection> {
        let portals = (scene.portals.iter()).filter(|p| !self.portals.contains(&p.id));
        let npcs = scene.npcs.iter().filter(|n| !self.npcs.contains(&n.id));
        let spawns = scene.spawns.iter().filter(|s| !self.spawns.contains(&s.id));
        let regions = scene
            .regions
            .iter()
            .filter(|r| !self.regions.contains(&r.id));
        (portals.map(|p| Selection::Portal(p.id)))
            .chain((self.entities..scene.entities.len()).map(Selection::Entity))
            .chain(npcs.map(|n| Selection::Npc(n.id)))
            .chain(spawns.map(|s| Selection::Spawn(s.id)))
            .chain(regions.map(|r| Selection::Region(r.id)))
            .collect()
    }
}

/// Copies of things on a map, placed relative to the first one's cell.
#[derive(Clone, Debug)]
pub struct Clip {
    items: Vec<(CellPos, Copied)>,
}

#[derive(Clone, Debug)]
enum Copied {
    Portal(Option<PortalTarget>),
    Entity(Entity),
    Npc(Npc),
    Spawn(Spawn),
    Region(Region),
}

impl Clip {
    /// `selection` copied out of `scene`; `None` when nothing is left to copy.
    pub fn of(scene: &Scene, selection: &[Selection]) -> Option<Self> {
        let anchor = selection.first()?.cell(scene)?;
        let items: Vec<(CellPos, Copied)> = (selection.iter())
            .filter_map(|s| {
                let cell = s.cell(scene)?;
                let copied = match *s {
                    Selection::Portal(id) => {
                        Copied::Portal(scene.portals.iter().find(|p| p.id == id)?.to)
                    }
                    Selection::Entity(i) => Copied::Entity(scene.entities.get(i)?.clone()),
                    Selection::Npc(id) => {
                        Copied::Npc(scene.npcs.iter().find(|n| n.id == id)?.clone())
                    }
                    Selection::Spawn(id) => Copied::Spawn(spawn(scene, id)?.clone()),
                    Selection::Region(id) => Copied::Region(region(scene, id)?.clone()),
                };
                Some(((cell.0 - anchor.0, cell.1 - anchor.1), copied))
            })
            .collect();
        (!items.is_empty()).then_some(Self { items })
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// The commands that add the copies to `scene` with the first one on `at` (NPCs, spawn
    /// areas and portals get ids of their own; portals keep where they lead).
    pub fn commands(&self, scene: &Scene, at: CellPos) -> Vec<SceneCommand> {
        let mut next_portal = scene.portals.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        let mut commands = Vec::new();
        for (offset, copied) in &self.items {
            let cell = (at.0 + offset.0, at.1 + offset.1);
            match copied.clone() {
                Copied::Portal(to) => {
                    let id = next_portal;
                    next_portal += 1;
                    commands.push(SceneCommand::AddPortal { cell, id: Some(id) });
                    if to.is_some() {
                        commands.push(SceneCommand::SetPortalTarget { id, to });
                    }
                }
                Copied::Entity(mut entity) => {
                    entity.placement = Placement::Cell(cell);
                    commands.push(SceneCommand::AddEntity {
                        entity,
                        index: None,
                    });
                }
                Copied::Npc(mut npc) => {
                    npc.id = 0;
                    npc.cell = cell;
                    commands.push(SceneCommand::AddNpc { npc });
                }
                Copied::Spawn(mut spawn) => {
                    let from = spawn.center();
                    let (dx, dy) = (cell.0 - from.0, cell.1 - from.1);
                    spawn.id = 0;
                    spawn.min = (spawn.min.0 + dx, spawn.min.1 + dy);
                    spawn.max = (spawn.max.0 + dx, spawn.max.1 + dy);
                    commands.push(SceneCommand::AddSpawn { spawn });
                }
                Copied::Region(mut region) => {
                    let from = region.center();
                    let (dx, dy) = (cell.0 - from.0, cell.1 - from.1);
                    region.id = 0;
                    region.min = (region.min.0 + dx, region.min.1 + dy);
                    region.max = (region.max.0 + dx, region.max.1 + dy);
                    commands.push(SceneCommand::AddRegion { region });
                }
            }
        }
        commands
    }
}

fn region(scene: &Scene, id: u32) -> Option<&Region> {
    scene.regions.iter().find(|r| r.id == id)
}

fn spawn(scene: &Scene, id: u32) -> Option<&Spawn> {
    scene.spawns.iter().find(|s| s.id == id)
}

/// What to pick, by kind: lets the view hide kinds it doesn't show.
#[derive(Clone, Copy)]
pub struct Pickable {
    pub portals: bool,
    pub objects: bool,
    pub npcs: bool,
    pub spawns: bool,
    pub regions: bool,
}

/// The thing under the pointer: an NPC or portal on `cell`, else the object anchored
/// nearest `world` within `radius` world pixels, else the smallest spawn area holding `cell`.
pub fn pick(
    scene: &Scene,
    cell: CellPos,
    world: (f32, f32),
    radius: f32,
    what: Pickable,
) -> Option<Selection> {
    if what.npcs
        && let Some(n) = scene.npcs.iter().find(|n| n.cell == cell)
    {
        return Some(Selection::Npc(n.id));
    }
    if what.portals
        && let Some(i) = scene.portal_at(cell)
    {
        return Some(Selection::Portal(scene.portals[i].id));
    }
    if what.objects {
        let nearest = scene
            .entities
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let (x, y) = scene.entity_world(e);
                (i, (x - world.0).hypot(y - world.1))
            })
            .filter(|(_, d)| *d <= radius)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = nearest {
            return Some(Selection::Entity(i));
        }
    }
    // The smallest spawn area or region holding the cell.
    let area = |min: CellPos, max: CellPos| (max.0 - min.0 + 1) * (max.1 - min.1 + 1);
    let spawns = (scene.spawns.iter())
        .filter(|s| what.spawns && s.contains(cell))
        .map(|s| (area(s.min, s.max), Selection::Spawn(s.id)));
    let regions = (scene.regions.iter())
        .filter(|r| what.regions && r.contains(cell))
        .map(|r| (area(r.min, r.max), Selection::Region(r.id)));
    spawns
        .chain(regions)
        .min_by_key(|(a, _)| *a)
        .map(|(_, s)| s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_scene::{Grid, IsoProjection};

    fn sprite(cell: CellPos) -> Entity {
        Entity {
            placement: Placement::Cell(cell),
            kind: EntityKind::Sprite {
                asset: "tree.png".into(),
                animation: "all".into(),
                footprint: (1, 1),
                offset: (0, 0),
                frame_ms: 100,
            },
        }
    }

    fn scene(cells: &[CellPos]) -> Scene {
        Scene {
            grid: Grid::new(32, 32),
            projection: IsoProjection::for_grid(64.0, 32.0, 32),
            ground: None,
            portals: Vec::new(),
            entities: cells.iter().map(|&c| sprite(c)).collect(),
            layers: Vec::new(),
            npcs: Vec::new(),
            spawns: Vec::new(),
            regions: Vec::new(),
            traps: Vec::new(),
            meta: Default::default(),
        }
    }

    fn cell_after(command: &SceneCommand) -> CellPos {
        match command {
            SceneCommand::MoveEntity { to, .. } => *to,
            other => panic!("not a move: {other:?}"),
        }
    }

    #[test]
    fn lining_up_keeps_the_screen_line() {
        let s = scene(&[(10, 10), (14, 9), (7, 13)]);
        let all = [
            Selection::Entity(0),
            Selection::Entity(1),
            Selection::Entity(2),
        ];
        // A row: the same x + y (screen height) as the first.
        for c in align(&s, &all, Line::Row) {
            let (x, y) = cell_after(&c);
            assert!((x + y - 20).abs() <= 1, "{x},{y}");
        }
        // A column: the same x - y (screen x) as the first.
        for c in align(&s, &all, Line::Column) {
            let (x, y) = cell_after(&c);
            assert!((x - y).abs() <= 1, "{x},{y}");
        }
    }

    #[test]
    fn reordering_moves_objects_to_the_ends_of_the_list() {
        let s = scene(&[(1, 1), (2, 2), (3, 3), (4, 4)]);
        let (commands, after) = reorder(&s, &[Selection::Entity(1)], true);
        assert_eq!(after, [Selection::Entity(3)]);
        assert!(matches!(
            commands[0],
            SceneCommand::RemoveEntity { index: 1 }
        ));
        assert!(matches!(
            commands[1],
            SceneCommand::AddEntity { index: Some(3), .. }
        ));
        let (_, after) = reorder(&s, &[Selection::Entity(2), Selection::Entity(3)], false);
        assert_eq!(after, [Selection::Entity(0), Selection::Entity(1)]);
    }

    #[test]
    fn pixel_nudges_shift_sprite_images() {
        let s = scene(&[(1, 1)]);
        let commands = nudge_pixels(&s, &[Selection::Entity(0)], (0, -1));
        let SceneCommand::SetEntity { entity, .. } = &commands[0] else {
            panic!("not a set");
        };
        assert!(matches!(
            entity.kind,
            EntityKind::Sprite {
                offset: (0, -1),
                ..
            }
        ));
    }
}
