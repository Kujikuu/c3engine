//! Game-master commands typed in chat (`/money 500`): for testing in Play mode now, for GMs
//! on the server later.

use crate::sim::Sim;
use crate::{EntityId, Task};
use c3_script::quest;

/// The commands and what they do, for `/help`.
pub const HELP: &str = "/money N · /item TYPE [COUNT] · /skill ID · /status ID · /pk · /level N · /heal · /shop NPC · /goto X Y · /map ID X Y · /quest ID STATE [KILLS] · /signal NAME [A B C]";

impl Sim {
    /// Runs chat command `line` for player `id`; the answer to show, or why it failed.
    /// `/signal NAME [A B C]` (game masters): raises signal NAME for the player, as a
    /// script's `raise` does.
    fn signal_command(&mut self, id: EntityId, words: &[&str]) -> Result<String, String> {
        let gm = self
            .players
            .get(&id)
            .is_some_and(|p| self.settings.is_gm(&p.name));
        if !gm {
            return Err("Only game masters can use commands.".into());
        }
        let (name, numbers) = words.split_first().ok_or("Usage: /signal NAME [A B C]")?;
        let mut args = [0_i64; 3];
        for (slot, word) in args.iter_mut().zip(numbers) {
            *slot = word
                .parse()
                .map_err(|_| format!("\"{word}\" is not a number"))?;
        }
        let values = args.iter().map(|&a| a.into()).collect();
        self.script_event(id, name, values);
        Ok(format!(
            "Raised {name} {} {} {}.",
            args[0], args[1], args[2]
        ))
    }

    pub fn command(&mut self, id: EntityId, line: &str) -> Result<String, String> {
        // Everyone runs their team.
        let all: Vec<&str> = line.trim_start_matches('/').split_whitespace().collect();
        if all.first().is_some_and(|w| w.eq_ignore_ascii_case("team")) {
            return self.team_command(id, &all[1..]);
        }
        if all
            .first()
            .is_some_and(|w| w.eq_ignore_ascii_case("signal"))
        {
            return self.signal_command(id, &all[1..]);
        }
        let mut words = line.trim_start_matches('/').split_whitespace();
        let name = words.next().unwrap_or_default().to_lowercase();
        let args: Vec<i64> = words
            .map(|w| w.parse().map_err(|_| format!("\"{w}\" is not a number")))
            .collect::<Result<_, _>>()?;
        let arg = |i: usize| args.get(i).copied().ok_or_else(|| format!("Usage: {HELP}"));
        let Some(player) = self.players.get(&id) else {
            return Err("No such player".into());
        };
        if !self.settings.is_gm(&player.name) {
            return Err("Only game masters can use commands.".into());
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
                let count = args.get(1).copied().unwrap_or(1).clamp(1, 10_000) as u32;
                let given = self.give_items(id, kind, count);
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
            "skill" => {
                let skill = u32::try_from(arg(0)?).map_err(|_| "Bad skill".to_owned())?;
                let name = (self.rules.skill(skill))
                    .map(|t| t.name.clone())
                    .ok_or_else(|| format!("There is no skill {skill}."))?;
                if self.learn_skill(id, skill) {
                    Ok(format!("Learned {name}."))
                } else {
                    Ok(format!("You already know {name}."))
                }
            }
            "pk" => {
                let pk = !self.pk;
                self.set_pk(pk);
                Ok(if pk {
                    "PK is on here."
                } else {
                    "PK is off here."
                }
                .to_owned())
            }
            "status" => {
                let status = u32::try_from(arg(0)?).map_err(|_| "Bad status".to_owned())?;
                let name = (self.rules.status(status))
                    .map(|t| t.name.clone())
                    .ok_or_else(|| format!("There is no status {status}."))?;
                self.add_status(id, status, None);
                Ok(format!("You bear {name}."))
            }
            "heal" => {
                let p = self.players.get_mut(&id).expect("checked above");
                p.life = p.stats.max_life;
                p.mana = p.stats.max_mana;
                Ok("Healed.".into())
            }
            "shop" => {
                let npc = u32::try_from(arg(0)?).map_err(|_| format!("Usage: {HELP}"))?;
                if self.rules.shop(npc).is_empty() {
                    return Err(format!("NPC {npc} sells nothing."));
                }
                let p = self.players.get_mut(&id).expect("checked above");
                p.shop = Some(npc);
                self.events
                    .push(crate::Event::ShopOpened { player: id, npc });
                Ok(format!("NPC {npc}'s shop is open."))
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
            "map" => {
                let map = u32::try_from(arg(0)?).map_err(|_| "Bad map".to_owned())?;
                let cell = (arg(1)? as i32, arg(2)? as i32);
                self.travel(id, c3_scene::PortalTarget { map, cell });
                Ok(format!("Off to map {map} ({}, {}).", cell.0, cell.1))
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
