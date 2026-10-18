//! NPCs and monster spawn areas, addressed by id.

use super::{Change, Edit, SceneDoc};
use crate::EditError;
use c3_scene::{CellPos, Npc, Spawn};

/// Lowest id ≥ 1 not in `ids`.
fn free_id(ids: impl Iterator<Item = u32> + Clone) -> u32 {
    (1..).find(|i| !ids.clone().any(|id| id == *i)).unwrap_or(1)
}

impl SceneDoc {
    fn npc_index(&self, id: u32) -> Result<usize, EditError> {
        self.scene
            .npcs
            .iter()
            .position(|n| n.id == id)
            .ok_or(EditError::NoNpc(id))
    }

    fn spawn_index(&self, id: u32) -> Result<usize, EditError> {
        self.scene
            .spawns
            .iter()
            .position(|s| s.id == id)
            .ok_or(EditError::NoSpawn(id))
    }

    fn check_spawn_area(&self, spawn: &Spawn) -> Result<(), EditError> {
        if spawn.max.0 < spawn.min.0 || spawn.max.1 < spawn.min.1 {
            return Err(EditError::InvalidArea("max is before min".into()));
        }
        self.check_cell(spawn.min)?;
        self.check_cell(spawn.max)
    }

    /// The id `add_npc` would give an NPC with id 0.
    pub fn next_npc_id(&self) -> u32 {
        free_id(self.scene.npcs.iter().map(|n| n.id))
    }

    /// The id `add_spawn` would give a spawn area with id 0.
    pub fn next_spawn_id(&self) -> u32 {
        free_id(self.scene.spawns.iter().map(|s| s.id))
    }

    pub(super) fn add_npc(&mut self, npc: &Npc) -> Edit {
        self.check_cell(npc.cell)?;
        let mut npc = npc.clone();
        if npc.id == 0 {
            npc.id = self.next_npc_id();
        } else if self.npc_index(npc.id).is_ok() {
            return Err(EditError::NpcIdTaken(npc.id));
        }
        let summary = format!(
            "Added NPC {} ({}) at {}, {}",
            npc.name, npc.id, npc.cell.0, npc.cell.1
        );
        let change = self.edit_npcs(|npcs| npcs.push(npc));
        Ok((summary, Some(change)))
    }

    pub(super) fn move_npc(&mut self, id: u32, to: CellPos) -> Edit {
        let i = self.npc_index(id)?;
        self.check_cell(to)?;
        let name = self.scene.npcs[i].name.clone();
        if self.scene.npcs[i].cell == to {
            return Ok((format!("{name} is already at {}, {}", to.0, to.1), None));
        }
        let change = self.edit_npcs(|npcs| npcs[i].cell = to);
        Ok((format!("Moved {name} to {}, {}", to.0, to.1), Some(change)))
    }

    pub(super) fn set_npc(&mut self, npc: &Npc) -> Edit {
        let i = self.npc_index(npc.id)?;
        self.check_cell(npc.cell)?;
        if &self.scene.npcs[i] == npc {
            return Ok(("Nothing changed".into(), None));
        }
        let summary = format!("Changed NPC {} ({})", npc.name, npc.id);
        let change = self.edit_npcs(|npcs| npcs[i] = npc.clone());
        Ok((summary, Some(change)))
    }

    pub(super) fn remove_npc(&mut self, id: u32) -> Edit {
        let i = self.npc_index(id)?;
        let summary = format!("Deleted NPC {} ({id})", self.scene.npcs[i].name);
        let change = self.edit_npcs(|npcs| {
            npcs.remove(i);
        });
        Ok((summary, Some(change)))
    }

    pub(super) fn add_spawn(&mut self, spawn: &Spawn) -> Edit {
        self.check_spawn_area(spawn)?;
        let mut spawn = spawn.clone();
        if spawn.id == 0 {
            spawn.id = self.next_spawn_id();
        } else if self.spawn_index(spawn.id).is_ok() {
            return Err(EditError::SpawnIdTaken(spawn.id));
        }
        let summary = format!("Added spawn area {} (monster {})", spawn.id, spawn.monster);
        let change = self.edit_spawns(|spawns| spawns.push(spawn));
        Ok((summary, Some(change)))
    }

    pub(super) fn set_spawn(&mut self, spawn: &Spawn) -> Edit {
        let i = self.spawn_index(spawn.id)?;
        self.check_spawn_area(spawn)?;
        if &self.scene.spawns[i] == spawn {
            return Ok(("Nothing changed".into(), None));
        }
        let summary = format!("Changed spawn area {}", spawn.id);
        let change = self.edit_spawns(|spawns| spawns[i] = spawn.clone());
        Ok((summary, Some(change)))
    }

    pub(super) fn remove_spawn(&mut self, id: u32) -> Edit {
        let i = self.spawn_index(id)?;
        let change = self.edit_spawns(|spawns| {
            spawns.remove(i);
        });
        Ok((format!("Deleted spawn area {id}"), Some(change)))
    }

    fn edit_npcs(&mut self, edit: impl FnOnce(&mut Vec<Npc>)) -> Change {
        let before = self.scene.npcs.clone();
        edit(&mut self.scene.npcs);
        Change::Npcs {
            before,
            after: self.scene.npcs.clone(),
        }
    }

    fn edit_spawns(&mut self, edit: impl FnOnce(&mut Vec<Spawn>)) -> Change {
        let before = self.scene.spawns.clone();
        edit(&mut self.scene.spawns);
        Change::Spawns {
            before,
            after: self.scene.spawns.clone(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::tests::doc;
    use super::*;
    use crate::SceneCommand;

    pub(crate) fn npc(id: u32, cell: CellPos) -> Npc {
        Npc {
            id,
            name: "Guard".into(),
            cell,
            direction: 0,
            look: 900,
            kind: 0,
            script: String::new(),
            extra: Default::default(),
        }
    }

    fn spawn(id: u32, min: CellPos, max: CellPos) -> Spawn {
        Spawn {
            id,
            monster: 1,
            min,
            max,
            max_alive: 3,
            respawn_secs: 5,
            per_wave: 1,
            extra: Default::default(),
        }
    }

    #[test]
    fn npcs_by_id_with_free_ids_and_undo() {
        let mut d = doc();
        d.execute(&SceneCommand::AddNpc {
            npc: npc(0, (1, 1)),
        })
        .unwrap();
        d.execute(&SceneCommand::AddNpc {
            npc: npc(5, (2, 2)),
        })
        .unwrap();
        assert_eq!(
            d.execute(&SceneCommand::AddNpc {
                npc: npc(5, (3, 3))
            }),
            Err(EditError::NpcIdTaken(5))
        );
        assert_eq!(d.scene().npcs[0].id, 1);
        assert_eq!(d.next_npc_id(), 2);
        d.execute(&SceneCommand::MoveNpc { id: 5, to: (6, 6) })
            .unwrap();
        let mut renamed = npc(5, (6, 6));
        renamed.name = "Captain".into();
        d.execute(&SceneCommand::SetNpc { npc: renamed }).unwrap();
        d.execute(&SceneCommand::RemoveNpc { id: 1 }).unwrap();
        assert_eq!(
            d.execute(&SceneCommand::RemoveNpc { id: 1 }),
            Err(EditError::NoNpc(1))
        );
        assert_eq!(d.scene().npcs.len(), 1);
        assert_eq!(d.scene().npcs[0].name, "Captain");
        d.undo();
        d.undo();
        assert_eq!(d.scene().npcs[1].name, "Guard");
        assert_eq!(d.scene().npcs[1].cell, (6, 6));
    }

    #[test]
    fn spawns_check_their_area() {
        let mut d = doc();
        d.execute(&SceneCommand::AddSpawn {
            spawn: spawn(0, (1, 1), (3, 2)),
        })
        .unwrap();
        assert!(d.scene().spawns[0].contains((2, 2)));
        assert!(matches!(
            d.execute(&SceneCommand::AddSpawn {
                spawn: spawn(0, (3, 3), (1, 1)),
            }),
            Err(EditError::InvalidArea(_))
        ));
        assert_eq!(
            d.execute(&SceneCommand::SetSpawn {
                spawn: spawn(1, (1, 1), (30, 2)),
            }),
            Err(EditError::OutOfBounds((30, 2)))
        );
        d.execute(&SceneCommand::SetSpawn {
            spawn: spawn(1, (0, 0), (7, 7)),
        })
        .unwrap();
        assert_eq!(d.scene().spawns[0].center(), (3, 3));
        d.execute(&SceneCommand::RemoveSpawn { id: 1 }).unwrap();
        d.undo();
        assert_eq!(d.scene().spawns[0].max, (7, 7));
    }
}
