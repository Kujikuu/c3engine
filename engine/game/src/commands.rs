//! Game-master commands typed in chat (`/money 500`): for testing in Play mode now, for GMs
//! on the server later.

use crate::sim::Sim;
use crate::{EntityId, Event, Task};
use c3_script::quest;

/// The commands and what they do, for `/help`.
pub const HELP: &str = "/money N · /wallet CURRENCY N · /train MINUTES · /merchant 0|255|DAYS · /lay LOOK · /hear NAME WORDS · /attribute NAME N · /deposit NAME CURRENCY N · /item TYPE [COUNT] · /durability ITEM N · /itemattr ITEM INDEX N · /skill ID · /status ID · /pk · /level N · /heal · /shop NPC · /storage NPC · /talk NPC · /goto X Y · /map ID X Y · /quest ID STATE [KILLS] · /signal NAME [A B C]";

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

    /// `/hear NAME WORDS` (game masters): NAME whispers WORDS to the player (to try the chat).
    fn hear_command(&mut self, id: EntityId, words: &[&str]) -> Result<String, String> {
        let Some(p) = self
            .players
            .get(&id)
            .filter(|p| self.settings.is_gm(&p.name))
        else {
            return Err("Only game masters can use commands.".into());
        };
        let [name, said @ ..] = words else {
            return Err("Usage: /hear NAME WORDS".into());
        };
        let hearer = p.name.clone();
        self.events.push(Event::Chat {
            player: id,
            name: (*name).to_owned(),
            text: said.join(" "),
            channel: crate::ChatChannel::Whisper,
            to: Some(id),
            hearer,
        });
        Ok(format!("{name} whispered."))
    }

    /// `/attribute NAME N` (game masters): sets one of the player's attributes.
    fn attribute_command(&mut self, id: EntityId, words: &[&str]) -> Result<String, String> {
        let gm = (self.players.get(&id)).is_some_and(|p| self.settings.is_gm(&p.name));
        if !gm {
            return Err("Only game masters can use commands.".into());
        }
        let [name, value] = words else {
            return Err("Usage: /attribute NAME N".into());
        };
        let value: i64 = (value.parse()).map_err(|_| format!("\"{value}\" is not a number"))?;
        let p = self.players.get_mut(&id).expect("checked above");
        p.attributes.insert((*name).to_owned(), value);
        self.refresh_stats(id);
        Ok(format!("{name} set to {value}."))
    }

    /// `/deposit NAME CURRENCY N` (game masters): leaves N of a currency for player NAME to take.
    fn deposit_command(&mut self, id: EntityId, words: &[&str]) -> Result<String, String> {
        let gm = (self.players.get(&id)).is_some_and(|p| self.settings.is_gm(&p.name));
        if !gm {
            return Err("Only game masters can use commands.".into());
        }
        let [name, currency, amount] = words else {
            return Err("Usage: /deposit NAME CURRENCY N".into());
        };
        let amount: u64 = (amount.parse()).map_err(|_| format!("\"{amount}\" is not a number"))?;
        if amount == 0 {
            return Err("Leave something.".into());
        }
        self.change_world(c3_script::WorldChange::Deposit {
            name: (*name).to_owned(),
            currency: (*currency).to_owned(),
            amount,
        });
        Ok(format!("{amount} {currency} wait for {name}."))
    }

    /// `/wallet CURRENCY N` (game masters): sets how much of a currency the player has.
    fn wallet_command(&mut self, id: EntityId, words: &[&str]) -> Result<String, String> {
        let gm = self
            .players
            .get(&id)
            .is_some_and(|p| self.settings.is_gm(&p.name));
        if !gm {
            return Err("Only game masters can use commands.".into());
        }
        let [currency, amount] = words else {
            return Err("Usage: /wallet CURRENCY N".into());
        };
        let amount: u64 = (amount.parse()).map_err(|_| format!("\"{amount}\" is not a number"))?;
        let p = self.players.get_mut(&id).expect("checked above");
        p.wallet.insert((*currency).to_owned(), amount);
        Ok(format!("{currency} set to {amount}."))
    }

    pub fn command(&mut self, id: EntityId, line: &str) -> Result<String, String> {
        // The commands taking words: everyone runs their team, the rest are game masters'.
        let all: Vec<&str> = line.trim_start_matches('/').split_whitespace().collect();
        let first = all.first().map(|w| w.to_ascii_lowercase());
        let rest = all.get(1..).unwrap_or_default();
        match first.as_deref() {
            Some("team") => return self.team_command(id, rest),
            Some("signal") => return self.signal_command(id, rest),
            Some("wallet") => return self.wallet_command(id, rest),
            Some("deposit") => return self.deposit_command(id, rest),
            Some("attribute") => return self.attribute_command(id, rest),
            Some("hear") => return self.hear_command(id, rest),
            _ => {}
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
            "durability" => {
                let item = u32::try_from(arg(0)?).map_err(|_| "Bad item".to_owned())?;
                let value = u32::try_from(arg(1)?.max(0)).unwrap_or(0);
                let p = self.players.get_mut(&id).expect("checked above");
                let held = (p.bag.iter_mut().chain(p.equipment.iter_mut())).find(|i| i.id == item);
                let held = held.ok_or_else(|| format!("You hold no item {item}."))?;
                held.durability = value;
                self.events.push(Event::Dressed { player: id });
                Ok(format!("Item {item}'s durability set to {value}."))
            }
            // Item ITEM's (0: the last in the bag) attribute number INDEX (the settings'
            // `item_attributes` order) set to N.
            "itemattr" => {
                let item = u32::try_from(arg(0)?).map_err(|_| "Bad item".to_owned())?;
                let at = usize::try_from(arg(1)?).map_err(|_| "Bad attribute".to_owned())?;
                let value = i32::try_from(arg(2)?).unwrap_or(0);
                let p = self.players.get_mut(&id).expect("checked above");
                let held = match item {
                    0 => p.bag.last_mut(),
                    _ => (p.bag.iter_mut().chain(p.equipment.iter_mut())).find(|i| i.id == item),
                };
                let held = held.ok_or_else(|| format!("You hold no item {item}."))?;
                *held.attrs.get_mut(at).ok_or("No such attribute")? = value;
                self.events.push(Event::Dressed { player: id });
                Ok(format!("Item {}'s attribute {at} set to {value}.", held.id))
            }
            // An NPC of this look to place, as a script asks (its `on_laid` event when put).
            "lay" => {
                let look = u32::try_from(arg(0)?).map_err(|_| "Bad look".to_owned())?;
                let p = self.players.get_mut(&id).expect("checked above");
                p.placing = Some("on_laid".into());
                let asked = crate::Event::PlacementAsked {
                    player: id,
                    look,
                    kind: 0,
                };
                self.events.push(asked);
                Ok(format!("Place NPC look {look}."))
            }
            // Away training this many minutes already, its credit full (as on coming back).
            // The merchant state: 0 none, 255 one, else waiting so many days; told as at login.
            "merchant" => {
                let n = u32::try_from(arg(0)?.max(0)).unwrap_or(0);
                let state = match n {
                    0 | crate::MERCHANT => n,
                    days => crate::merchants::day(days),
                };
                self.players.get_mut(&id).expect("checked above").merchant = state;
                self.merchant_login(id);
                Ok(format!("Merchant state {state}."))
            }
            "train" => {
                let minutes = arg(0)?.max(0);
                let most = self.settings.offline_training.most_credit;
                let p = self.players.get_mut(&id).expect("checked above");
                p.training_credit = most;
                p.training_since = crate::training::now_secs() - minutes * 60;
                let news = crate::TrainingNews::Ready;
                self.events
                    .push(crate::Event::Training { player: id, news });
                Ok(format!("Training for {minutes} minutes."))
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
            // Beside NPC `npc`, talking to it.
            "talk" => {
                let npc = u32::try_from(arg(0)?).map_err(|_| format!("Usage: {HELP}"))?;
                let cell = (self.npcs.iter().find(|n| n.id == npc))
                    .map(|n| n.cell)
                    .ok_or(format!("There is no NPC {npc} here."))?;
                let from = self.players[&id].cell;
                let beside = self.beside(cell, from);
                self.place(id, beside);
                self.talk(id, npc);
                Ok(format!("Talking to NPC {npc}."))
            }
            "storage" => {
                let npc = u32::try_from(arg(0)?).map_err(|_| format!("Usage: {HELP}"))?;
                self.events
                    .push(crate::Event::StorageOpened { player: id, npc });
                Ok(format!("NPC {npc}'s warehouse is open."))
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
