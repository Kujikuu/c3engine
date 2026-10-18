//! Game-master commands typed in chat (`/money 500`): for testing in Play mode now, for GMs
//! on the server later.

use crate::sim::Sim;
use crate::{EntityId, Task};
use c3_script::quest;

/// The commands and what they do, for `/help`.
pub const HELP: &str =
    "/money N · /item TYPE [COUNT] · /level N · /heal · /goto X Y · /quest ID STATE [KILLS]";

impl Sim {
    /// Runs chat command `line` for player `id`; the answer to show, or why it failed.
    pub fn command(&mut self, id: EntityId, line: &str) -> Result<String, String> {
        let mut words = line.trim_start_matches('/').split_whitespace();
        let name = words.next().unwrap_or_default().to_lowercase();
        let args: Vec<i64> = words
            .map(|w| w.parse().map_err(|_| format!("\"{w}\" is not a number")))
            .collect::<Result<_, _>>()?;
        let arg = |i: usize| args.get(i).copied().ok_or_else(|| format!("Usage: {HELP}"));
        if !self.players.contains_key(&id) {
            return Err("No such player".into());
        }
        match name.as_str() {
            "help" => Ok(HELP.into()),
            "money" => {
                let money = arg(0)?.max(0) as u64;
                self.set_money(id, money);
                Ok(format!("Money set to {money}."))
            }
            "item" => {
                let kind = u32::try_from(arg(0)?).map_err(|_| "Bad item type".to_owned())?;
                let name = self
                    .rules
                    .item(kind)
                    .map(|t| t.name.clone())
                    .ok_or_else(|| format!("There is no item type {kind}."))?;
                let count = args
                    .get(1)
                    .copied()
                    .unwrap_or(1)
                    .clamp(1, crate::BAG_SIZE as i64);
                let given = (0..count).take_while(|_| self.give_item(id, kind)).count();
                Ok(format!("Gave {given} × {name}."))
            }
            "level" => {
                let level = arg(0)?.clamp(1, 1000) as u32;
                let p = self.players.get_mut(&id).expect("checked above");
                p.level = level;
                p.exp = 0;
                self.refresh_stats(id);
                let p = self.players.get_mut(&id).expect("checked above");
                p.life = p.stats.max_life;
                p.mana = p.stats.max_mana;
                Ok(format!("Level set to {level}."))
            }
            "heal" => {
                let p = self.players.get_mut(&id).expect("checked above");
                p.life = p.stats.max_life;
                p.mana = p.stats.max_mana;
                Ok("Healed.".into())
            }
            "goto" => {
                let cell = (arg(0)? as i32, arg(1)? as i32);
                if !self.walkable(cell) {
                    return Err(format!("({}, {}) can't be stood on.", cell.0, cell.1));
                }
                self.place(id, cell);
                self.players.get_mut(&id).expect("checked above").task = Task::Idle;
                Ok(format!("Moved to ({}, {}).", cell.0, cell.1))
            }
            "quest" => {
                let quest = arg(0)?;
                let state = arg(1)?.clamp(quest::NOT_TAKEN, quest::DONE);
                let name = self
                    .quests
                    .iter()
                    .find(|q| q.id == quest)
                    .map(|q| q.name.clone())
                    .ok_or_else(|| format!("There is no quest {quest}."))?;
                let p = self.players.get_mut(&id).expect("checked above");
                let mut set = |flag: String, value: i64| {
                    if value == 0 {
                        p.flags.remove(&flag);
                    } else {
                        p.flags.insert(flag, value);
                    }
                };
                set(quest::state_flag(quest), state);
                set(
                    quest::kills_flag(quest),
                    args.get(2).copied().unwrap_or(0).max(0),
                );
                let says = ["not taken", "taken", "done"][state as usize];
                Ok(format!("{name} is now {says}."))
            }
            _ => Err(format!("Unknown command /{name}. {HELP}")),
        }
    }
}
