//! Dice tables as players see them (TQ's CDiceView): a desk and three dice by the table's
//! NPC, as the game dresses them ([`GameModule::dice_table`]). A roll spins each die for three
//! to four seconds, bobbing, then it rests on its face.

use crate::actor::Actor;
use crate::module::DiceTableLook;
use crate::world::World;
use glam::Vec2;
use std::collections::HashMap;

/// How long a die spins at least, and the most it spins longer (ms).
const SPIN_MS: u64 = 3000;
const SPIN_SPREAD_MS: u64 = 1000;
/// How high a spinning die bobs (pixels) and how long it takes up and down (ms).
const BOB: f32 = 10.0;
const BOB_MS: u64 = 660;

/// One table's actors: its desk, then each die's spinning and resting models; when each
/// die stops spinning.
struct Table {
    actors: Vec<usize>,
    look: DiceTableLook,
    spin_until: [u64; 3],
}

impl Table {
    fn spinning(&self, die: usize) -> usize {
        self.actors[1 + die]
    }

    fn resting(&self, die: usize) -> usize {
        self.actors[4 + die]
    }
}

/// The dice tables shown, by their NPCs.
#[derive(Default)]
pub struct DiceTables {
    tables: HashMap<u32, Table>,
}

impl DiceTables {
    /// Dresses NPC `npc` standing at `npc_actor` as a dice table (the faces shown 1, 2, 3).
    pub fn add(&mut self, npc: u32, npc_actor: usize, world: &mut World) {
        let Some(look) = world.game.dice_table() else {
            return;
        };
        let Some(base) = world.actors.get(npc_actor) else {
            return;
        };
        let (cell, zoom) = (base.cell, base.zoom_percent);
        let game = world.game.clone();
        // The dice sort with the desk and come after it, so they are drawn on it.
        let desk = Vec2::from(look.desk_offset);
        let mut spawn = |look: &crate::character::Look, offset: Vec2, rise: f32, zoom: f32| {
            let actor = Actor::from_look(&*game, look, 0, cell, 0)
                .ok()
                .map(|mut a| {
                    a.offset = offset;
                    a.order_offset = desk - offset;
                    a.rise = rise;
                    a.zoom_percent = zoom;
                    a
                })?;
            world.actors.push(actor);
            Some(world.actors.len() - 1)
        };
        let mut actors = Vec::new();
        actors.extend(spawn(&look.desk, look.desk_offset.into(), 0.0, zoom));
        for &offset in &look.dice_offsets {
            actors.extend(spawn(
                &look.rolling,
                offset.into(),
                look.rise,
                look.dice_zoom,
            ));
        }
        for &offset in &look.dice_offsets {
            actors.extend(spawn(
                &look.resting,
                offset.into(),
                look.rise,
                look.dice_zoom,
            ));
        }
        if actors.len() != 7 {
            return;
        }
        let table = Table {
            actors,
            look,
            spin_until: [0; 3],
        };
        for die in 0..3 {
            world.actors[table.spinning(die)].hidden = true;
            world.actors[table.resting(die)].hold_frame = Some(die as u32);
        }
        self.tables.insert(npc, table);
    }

    /// Table `npc` rolled `dice`: each spins a while, then rests on its face.
    pub fn rolled(&mut self, npc: u32, dice: [u8; 3], world: &mut World, seed: u64) {
        let Some(table) = self.tables.get_mut(&npc) else {
            return;
        };
        let now = world.now_ms();
        for (die, face) in dice.into_iter().enumerate() {
            let spread = (seed >> (die * 10)) % SPIN_SPREAD_MS;
            table.spin_until[die] = now + SPIN_MS + spread;
            world.actors[table.resting(die)].hold_frame = Some(u32::from(face.max(1)) - 1);
        }
    }

    /// Shows each die spinning (bobbing) or resting as its time says.
    pub fn tick(&self, world: &mut World) {
        let now = world.now_ms();
        for table in self.tables.values() {
            for die in 0..3 {
                let spinning = now < table.spin_until[die];
                let bob = (now + die as u64 * BOB_MS / 3) % (2 * BOB_MS);
                let bob = BOB * (1.0 - (bob as f32 / BOB_MS as f32 - 1.0).abs());
                let spin = &mut world.actors[table.spinning(die)];
                spin.hidden = !spinning;
                spin.rise = table.look.rise + bob;
                world.actors[table.resting(die)].hidden = spinning;
            }
        }
    }

    /// Every actor index the tables hold (to follow actors moved by a removal).
    pub fn indices_mut(&mut self) -> impl Iterator<Item = &mut usize> {
        self.tables.values_mut().flat_map(|t| t.actors.iter_mut())
    }

    /// Table `npc`'s actors, no longer shown (to remove, highest first).
    pub fn remove(&mut self, npc: u32) -> Vec<usize> {
        let mut actors = self
            .tables
            .remove(&npc)
            .map(|t| t.actors)
            .unwrap_or_default();
        actors.sort_unstable_by(|a, b| b.cmp(a));
        actors
    }
}
