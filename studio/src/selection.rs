//! What the Scene view has selected (one portal, object, NPC or spawn area), how to pick it
//! under the pointer, and the commands that move or delete it.

use c3_edit::{SceneCommand, entity_label};
use c3_scene::{CellPos, Placement, Scene, Spawn};

/// One selected thing on the map, by the id (or index) commands use.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Selection {
    Portal(i32),
    /// Index in the scene's entity list.
    Entity(usize),
    Npc(u32),
    Spawn(u32),
}

impl Selection {
    /// Whether it is still on the map (undo may have removed it).
    pub fn exists(self, scene: &Scene) -> bool {
        match self {
            Self::Portal(id) => scene.portals.iter().any(|p| p.id == id),
            Self::Entity(i) => i < scene.entities.len(),
            Self::Npc(id) => scene.npcs.iter().any(|n| n.id == id),
            Self::Spawn(id) => spawn(scene, id).is_some(),
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
        })
    }

    pub fn remove(self) -> SceneCommand {
        match self {
            Self::Portal(id) => SceneCommand::RemovePortal { id },
            Self::Entity(index) => SceneCommand::RemoveEntity { index },
            Self::Npc(id) => SceneCommand::RemoveNpc { id },
            Self::Spawn(id) => SceneCommand::RemoveSpawn { id },
        }
    }
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
    if what.spawns {
        let area = |s: &Spawn| (s.max.0 - s.min.0 + 1) * (s.max.1 - s.min.1 + 1);
        return scene
            .spawns
            .iter()
            .filter(|s| s.contains(cell))
            .min_by_key(|s| area(s))
            .map(|s| Selection::Spawn(s.id));
    }
    None
}
