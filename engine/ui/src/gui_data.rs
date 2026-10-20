//! What a data-driven UI's bindings read from the game: the hero's numbers, name and bag.

use crate::GameView;
use crate::gui::GuiData;
use c3_game::Player;

/// What the game UI keeps between frames: the storage NPC whose warehouse is open, and
/// what is typed in its fields.
#[derive(Debug, Default)]
pub(crate) struct GuiState {
    pub storage: Option<u32>,
    pub fields: std::collections::HashMap<String, String>,
    /// The hero's open trade.
    pub trade: Option<TradeView>,
    /// Bag items put on a workbench (compose, gems), in its slots' order.
    pub work: Vec<u32>,
    /// What the guild list shows: `members` (also when empty), `allies`, `enemies`.
    pub guild_tab: String,
    /// Invites and asks to join a team are taken at once (the team dialog's auto check).
    pub auto_team: bool,
    /// The minimap rolled up.
    pub radar_hidden: bool,
    /// The field whose edit takes the keyboard next frame.
    pub focus: Option<String>,
    /// Each view group's view (`view:<group>:<name>`; a status page's base or details).
    pub views: std::collections::HashMap<String, String>,
    /// The chat bar's lines said, listed names and recent channels.
    pub chat_bar: crate::chat_bar::ChatBar,
    /// The yes/no question shown.
    pub prompt: Option<crate::prompt::Prompt>,
    /// The skill picked for the right button (a type), and a pick waiting for the hero's
    /// skills (`pick_skill:<n>`).
    pub picked_skill: Option<u32>,
    pub picking: Option<usize>,
    /// The typed answer's button was pressed.
    pub answering: bool,
    /// The item picked up from a grid: its list and slot.
    pub held: Option<(String, usize)>,
    /// View-number steps asked for (`view_step:`): the group, the step and the binding that
    /// bounds it.
    pub steps: Vec<(String, i64, String)>,
}

/// The most items a workbench holds.
pub(crate) const WORK_SLOTS: usize = 5;

/// A trade as the hero sees it: the partner's goods, its own, the partner's word.
#[derive(Debug)]
pub(crate) struct TradeView {
    pub partner: c3_game::EntityId,
    pub theirs: Vec<u32>,
    pub their_money: u64,
    /// The bag items the hero offers (their ids).
    pub mine: Vec<u32>,
    pub agreed: bool,
}

impl GuiState {
    /// What the hero is told about its trade.
    pub fn trade_news(&mut self, partner: c3_game::EntityId, news: &c3_game::TradeNews) {
        use c3_game::TradeNews as N;
        match news {
            N::Opened => {
                self.trade = Some(TradeView {
                    partner,
                    theirs: Vec::new(),
                    their_money: 0,
                    mine: Vec::new(),
                    agreed: false,
                })
            }
            N::Done | N::Closed => self.trade = None,
            _ => {}
        }
        let Some(t) = &mut self.trade else {
            return;
        };
        match news {
            N::PartnerItem(item) => t.theirs.push(item.kind),
            N::PartnerMoney(n) => t.their_money = *n,
            N::PartnerAccepted => t.agreed = true,
            N::Refused(item) => t.mine.retain(|i| i != item),
            _ => {}
        }
    }
}

/// The hero, as bindings see it (`hero.life`, `hero.bag`, ...).
pub(crate) struct HeroData<'a> {
    pub view: &'a GameView<'a>,
    pub player: &'a Player,
    pub state: &'a GuiState,
}

impl HeroData<'_> {
    /// The hero's own items a list shows, in slot order: its bag, what it wears (all, or in
    /// slot `n`: `hero.worn.<n>`), what it offers in a trade, a workbench slot, the open
    /// warehouse; none for a list of item types (a shop's).
    pub(crate) fn own_items(&self, key: &str) -> Option<Vec<&c3_game::Item>> {
        let p = self.player;
        let sim = self.view.sim;
        let in_bag = |id: &u32| p.bag.iter().find(|i| i.id == *id);
        Some(match key {
            "hero.bag" => p.bag.iter().collect(),
            "hero.equipment" => p.equipment.iter().collect(),
            _ if key.starts_with("hero.worn.") => {
                let slot: Option<u8> = key.strip_prefix("hero.worn.").and_then(|s| s.parse().ok());
                (p.equipment.iter())
                    .filter(|i| slot.is_some() && sim.slot_of(i) == slot)
                    .collect()
            }
            "trade.mine" => (self.state.trade.iter())
                .flat_map(|t| &t.mine)
                .filter_map(in_bag)
                .collect(),
            _ if key.starts_with("work.") => {
                let n: usize = key.strip_prefix("work.")?.parse().ok()?;
                self.state
                    .work
                    .get(n)
                    .and_then(in_bag)
                    .into_iter()
                    .collect()
            }
            "storage.items" => (self.state.storage)
                .and_then(|npc| p.storage.get(&npc))
                .map(|items| items.iter().collect())
                .unwrap_or_default(),
            _ => return None,
        })
    }

    /// The attribute `hero.attr_at.<prefix>.<low>-<high>.<n>[+<group>]` names: its number and
    /// value.
    fn attr_entry(&self, key: &str) -> Option<(i64, i64)> {
        let spec = key.split_once('.')?.1.split_once('.')?.1;
        let (prefix, rest) = spec.split_once('.')?;
        let (ranges, n) = rest.split_once('.')?;
        // `low-high`, or several between commas (`0-399,600-999999`).
        let ranges: Vec<(i64, i64)> = (ranges.split(','))
            .map(|r| {
                let (low, high) = r.split_once('-')?;
                Some((low.parse().ok()?, high.parse().ok()?))
            })
            .collect::<Option<_>>()?;
        let (n, group) = n.split_once('+').map_or((n, None), |(n, g)| (n, Some(g)));
        let offset = group
            .and_then(|g| self.state.views.get(g))
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        let n = n.parse::<usize>().ok()? + offset;
        let mut found: Vec<(i64, i64)> = (self.player.attributes.iter())
            .filter_map(|(name, &value)| {
                let number: i64 = name.strip_prefix(prefix)?.parse().ok()?;
                let within = ranges
                    .iter()
                    .any(|&(low, high)| (low..=high).contains(&number));
                (within && value > 0).then_some((number, value))
            })
            .collect();
        found.sort_unstable();
        found.get(n).copied()
    }
}

impl GuiData for HeroData<'_> {
    fn focus(&self) -> Option<&str> {
        self.state.focus.as_deref()
    }

    fn number(&self, key: &str) -> Option<f64> {
        let p = self.player;
        let need = || self.view.sim.rules().level_exp(p.level);
        Some(match key {
            "hero.level" => p.level.into(),
            "hero.life" => p.life.into(),
            "hero.life_max" => p.stats.max_life.into(),
            "hero.mana" => p.mana.into(),
            "hero.mana_max" => p.stats.max_mana.into(),
            "hero.exp" => p.exp as f64,
            "hero.exp_next" => need()? as f64,
            "hero.money" => p.money as f64,
            // The hero's money of currency `<name>` (Conquer's CPs: `hero.wallet.CPs`).
            _ if key.starts_with("hero.wallet.") => {
                let name = key.strip_prefix("hero.wallet.")?;
                p.wallet.get(name).copied().unwrap_or(0) as f64
            }
            "hero.running" => f64::from(u8::from(p.running)),
            "ui.radar" => f64::from(u8::from(!self.state.radar_hidden)),
            "hero.bag_count" => p.bag.len() as f64,
            "hero.bag_size" => self.view.sim.bag_size() as f64,
            "hero.attack_min" => p.stats.attack_min.into(),
            "hero.attack_max" => p.stats.attack_max.into(),
            "hero.defence" => p.stats.defence.into(),
            "hero.dodge" => p.stats.dodge.into(),
            "hero.dexterity" => p.stats.dexterity.into(),
            "hero.pk" => p.pk.into(),
            // The storage NPC whose warehouse is open (0: none), and the money kept.
            "hero.storage" => self.state.storage.unwrap_or(0).into(),
            "storage.money" => p.saved_money as f64,
            // 1 while the hero trades; the partner's money and word (1: agreed).
            "hero.trading" => u8::from(self.state.trade.is_some()).into(),
            "trade.money" => self.state.trade.as_ref().map_or(0, |t| t.their_money) as f64,
            "trade.agreed" => u8::from(self.state.trade.as_ref().is_some_and(|t| t.agreed)).into(),
            // 1 while the hero talks with an NPC.
            "hero.talking" => u8::from(p.dialogue.is_some()).into(),
            // The most characters the typed answer takes.
            "dialogue.input_limit" => {
                let page = &p.dialogue.as_ref()?.page;
                page.options.iter().find_map(|o| o.input)?.into()
            }
            // 1 when an answer of the NPC's page is typed (a text box).
            "dialogue.typing" => {
                let page = &p.dialogue.as_ref()?.page;
                u8::from(page.options.iter().any(|o| o.input.is_some())).into()
            }
            // The picture of the NPC's page (its face), when it has one.
            "dialogue.picture" => {
                let page = &p.dialogue.as_ref()?.page;
                page.picture? as f64
            }
            // The NPC whose shop is open (0: none).
            "hero.shop" => p.shop.unwrap_or(0).into(),
            "hero.profession" => p.profession.into(),
            // The hero's team: in one, member `n` there, its options, auto-joining.
            "team.in" => u8::from(p.team.is_some()).into(),
            "team.leads" => u8::from(p.team == Some(p.id)).into(),
            _ if key.starts_with("team.has.") => {
                let n: usize = key.strip_prefix("team.has.")?.parse().ok()?;
                let count = match p.mates.is_empty() {
                    true => self.view.sim.team_mates(p.id).len(),
                    false => p.mates.len(),
                };
                u8::from(n < count).into()
            }
            "team.closed" | "team.shares_money" | "team.shares_items" => {
                let (closed, keeps, shares) = self.view.sim.team_options(p.team?);
                u8::from(match key {
                    "team.closed" => closed,
                    "team.shares_money" => !keeps,
                    _ => shares,
                })
                .into()
            }
            "team.auto" => u8::from(self.state.auto_team).into(),
            // A question shows, of kind `<kind>` (`prompt.kind.<kind>`).
            "prompt.open" => u8::from(self.state.prompt.is_some()).into(),
            // The open shop's wares are priced in currency `<name>` (`shop.priced_in.CPs`; TQ's
            // CP shops).
            _ if key.starts_with("shop.priced_in.") => {
                let currency = key.strip_prefix("shop.priced_in.")?;
                let rules = self.view.sim.rules();
                let Some(npc) = p.shop else {
                    return Some(0.0);
                };
                let first = rules.shop(npc).first().copied();
                let priced = first.and_then(|kind| {
                    let own = rules.item(kind).map(|t| t.currency.as_str());
                    rules.shop_currency(npc, kind).or(own)
                });
                u8::from(priced == Some(currency)).into()
            }
            // The page view group `<group>` shows, `<per>` to a page, from 1
            // (`view_page.20.depot_page`), and how many pages list `<list>` fills, 1 at least
            // (`list_pages.20.storage.items`).
            _ if key.starts_with("view_page.") => {
                let (per, group) = key.strip_prefix("view_page.")?.split_once('.')?;
                let per: usize = per.parse().ok().filter(|&p| p > 0)?;
                (self.view_number(group) / per + 1) as f64
            }
            _ if key.starts_with("list_pages.") => {
                let (per, list) = key.strip_prefix("list_pages.")?.split_once('.')?;
                let per: usize = per.parse().ok().filter(|&p| p > 0)?;
                self.items(list).len().div_ceil(per).max(1) as f64
            }
            // How many items list `<list>` holds (`count.shop.items`).
            _ if key.starts_with("count.") => self.items(key.strip_prefix("count.")?).len() as f64,
            "skill.picked" => u8::from(self.state.picked_skill.is_some()).into(),
            // The hero has skills (the strip opens only then).
            "hero.has_skills" => u8::from(!p.skills.is_empty()).into(),
            _ if key.starts_with("prompt.kind.") => {
                let kind = key.strip_prefix("prompt.kind.")?;
                u8::from(self.state.prompt.as_ref().is_some_and(|p| p.kind == kind)).into()
            }
            // The name typed in the chat bar is listed / can be (the Add button's state).
            "chat.listed" | "chat.can_list" => {
                let name = self.state.fields.get(crate::CHAT_NAME);
                let name = name.map_or("", |n| n.trim());
                let bar = &self.state.chat_bar;
                u8::from(match key {
                    "chat.listed" => bar.listed(name),
                    _ => bar.can_list(name),
                })
                .into()
            }
            // 1 while view group `<group>` shows view `<name>` (`view.<group>.<name>`).
            _ if key.starts_with("view.") => {
                let (group, name) = key.strip_prefix("view.")?.split_once('.')?;
                u8::from(self.state.views.get(group).is_some_and(|v| v == name)).into()
            }
            // The `n`th of the hero's attributes named `<prefix><number>` with the number in
            // `low-high` and above 0: its number (`hero.attr_at.<prefix>.<low>-<high>.<n>`) or
            // its value (`hero.attr_val...`); `n` may add a view group's number (`3+page`).
            _ if key.starts_with("hero.attr_at.") || key.starts_with("hero.attr_val.") => {
                let (number, value) = self.attr_entry(key)?;
                (if key.starts_with("hero.attr_at.") {
                    number
                } else {
                    value
                }) as f64
            }
            // A number the game shows of the hero (`hero.shown.<name>`, its rules' `shown`).
            _ if key.starts_with("hero.shown.") => {
                let name = key.strip_prefix("hero.shown.")?;
                let shown = self.view.sim.shown(p.id);
                shown.iter().find(|(n, _)| n == name).map_or(0, |(_, v)| *v) as f64
            }
            // Any of the hero's attributes (`hero.attr.strength`), 0 when it has none.
            _ => {
                // Its own value, else the attribute's default (stamina starts full).
                let name = key.strip_prefix("hero.attr.")?;
                let default = (self.view.sim.settings().attributes.iter())
                    .find(|a| a.name == name)
                    .map_or(0, |a| a.default);
                p.attributes.get(name).copied().unwrap_or(default) as f64
            }
        })
    }

    fn text(&self, key: &str) -> Option<String> {
        match key {
            "hero.name" => Some(self.player.name.clone()),
            // Who asked the question shown, and its kind.
            "prompt.name" => Some(self.state.prompt.as_ref()?.name.clone()),
            "prompt.kind" => Some(self.state.prompt.as_ref()?.kind.clone()),
            // Team member `n`'s face (1, Conquer's heroes'), none past the team's end.
            _ if key.starts_with("team.face.") => {
                let n = key.strip_prefix("team.face.")?;
                (self.number(&format!("team.has.{n}")) == Some(1.0)).then(|| "1".to_owned())
            }
            // The hero's text variable `<name>` (its spouse: `hero.text.mate`).
            _ if key.starts_with("hero.text.") => {
                let name = key.strip_prefix("hero.text.")?;
                Some(self.player.texts.get(name).cloned().unwrap_or_default())
            }
            // The skill picked for the right button (its type).
            "skill.picked" => self.state.picked_skill.map(|s| s.to_string()),
            // The hero's skill `n` (its type), none past the last.
            _ if key.starts_with("skill.") => {
                let n: usize = key.strip_prefix("skill.")?.parse().ok()?;
                Some(self.player.skills.get(n)?.to_string())
            }
            "storage.amount" | "trade.amount" | "dialogue.typed" | "prompt.amount" => {
                Some(self.state.fields.get(key).cloned().unwrap_or_default())
            }
            // Team member `n`'s name (the leader first), empty past the team's end.
            _ if key.starts_with("team.member.") => {
                let n: usize = key.strip_prefix("team.member.")?.parse().ok()?;
                // A server sends the team; a local game asks its simulation.
                let local;
                let mates = if self.player.mates.is_empty() {
                    local = self.view.sim.team_mates(self.player.id);
                    &local
                } else {
                    &self.player.mates
                };
                Some(mates.get(n).map(|m| m.name.clone()).unwrap_or_default())
            }
            // The hero's guild: its name, leader, the hero's rank, members and fund.
            _ if key.starts_with("guild.") => {
                let world = self.view.sim.world_state();
                let (id, rank) = world.guild_of(&self.player.name)?;
                let g = world.guilds.get(&id)?;
                Some(match key {
                    "guild.name" => g.name.clone(),
                    "guild.leader" => g.leader.clone(),
                    "guild.rank" => rank.to_string(),
                    "guild.members" => g.members.len().to_string(),
                    "guild.fund" => g.fund.to_string(),
                    _ => return None,
                })
            }
            // The trade partner's name.
            "trade.name" => (self.state.trade.as_ref())
                .and_then(|t| self.view.sim.player(t.partner))
                .map(|p| p.name.clone()),
            // What the NPC says now, a line each.
            "dialogue.text" => (self.player.dialogue.as_ref()).map(|c| c.page.lines.join("\n")),
            // The question a typed answer answers (its option's label).
            "dialogue.question" => {
                let page = &self.player.dialogue.as_ref()?.page;
                Some(
                    page.options
                        .iter()
                        .find(|o| o.input.is_some())?
                        .label
                        .clone(),
                )
            }
            // The page's lines run together, as TQ's client joins an NPC's text packets.
            "dialogue.said" => (self.player.dialogue.as_ref()).map(|c| c.page.lines.concat()),
            _ => self.number(key).map(|n| format!("{n}")),
        }
    }

    fn item_looks(&self, key: &str) -> Vec<u32> {
        let sim = self.view.sim;
        (self.own_items(key).unwrap_or_default().into_iter())
            .map(|i| sim.item_look(i))
            .collect()
    }

    fn items(&self, key: &str) -> Vec<u32> {
        if let Some(items) = self.own_items(key) {
            return items.into_iter().map(|i| i.kind).collect();
        }
        match key {
            // What the partner offers.
            "trade.theirs" => self
                .state
                .trade
                .as_ref()
                .map(|t| t.theirs.clone())
                .unwrap_or_default(),
            // What the open shop sells.
            "shop.items" => (self.player.shop)
                .map(|npc| self.view.sim.rules().shop(npc).to_vec())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    fn holding(&self) -> bool {
        self.state.held.is_some()
    }

    fn view_number(&self, group: &str) -> usize {
        (self.state.views.get(group)).map_or(0, |v| v.parse().unwrap_or(0))
    }

    fn item_tip(&self, key: &str, index: usize) -> Option<String> {
        match self.own_items(key) {
            Some(items) => Some(crate::bag::describe_item(self.view, items.get(index)?)),
            None => Some(crate::bag::describe(
                self.view,
                *self.items(key).get(index)?,
            )),
        }
    }

    fn rows(&self, key: &str) -> (Vec<String>, Option<usize>) {
        match key {
            // The hero's team: its members' names, the leader first.
            "hero.team" => {
                let names = self.player.mates.iter().map(|m| m.name.clone());
                (names.collect(), None)
            }
            // The names listed to talk to (the chat bar's list).
            "chat.targets" => (self.state.chat_bar.targets.clone(), None),
            // The hero's guild's members, or its allies or enemies (the guild tab picked).
            "guild.member_names" => {
                let world = self.view.sim.world_state();
                let guild = world
                    .guild_of(&self.player.name)
                    .and_then(|(id, _)| world.guilds.get(&id));
                let Some(g) = guild else {
                    return (Vec::new(), None);
                };
                let named = |ids: &[u32]| -> Vec<String> {
                    let names = ids.iter().filter_map(|id| world.guilds.get(id));
                    names.map(|o| o.name.clone()).collect()
                };
                let rows = match self.state.guild_tab.as_str() {
                    "allies" => named(&g.allies),
                    "enemies" => named(&g.enemies),
                    _ => g.members.keys().cloned().collect(),
                };
                (rows, None)
            }
            // The answers picked by a click (not typed), as TQ's CDlgTask lists them.
            "dialogue.choices" => {
                let page = self.player.dialogue.as_ref().map(|c| &c.page);
                let picked = page.into_iter().flat_map(|p| &p.options);
                let rows = picked
                    .filter(|o| o.input.is_none())
                    .map(|o| o.label.clone());
                (rows.collect(), None)
            }
            // The answers the NPC offers.
            "dialogue.answers" => {
                let page = self.player.dialogue.as_ref().map(|c| &c.page);
                let labels = page.into_iter().flat_map(|p| &p.options);
                (labels.map(|o| o.label.clone()).collect(), None)
            }
            _ => (Vec::new(), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_game::{EntityId, Item, TradeNews};

    #[test]
    fn a_trade_follows_its_news() {
        let mut state = GuiState::default();
        let partner = EntityId(7);
        state.trade_news(partner, &TradeNews::PartnerMoney(5));
        assert!(state.trade.is_none(), "no trade before it opens");
        state.trade_news(partner, &TradeNews::Opened);
        let item = Item::new(1, 1001, None);
        state.trade_news(partner, &TradeNews::PartnerItem(item));
        state.trade_news(partner, &TradeNews::PartnerMoney(300));
        state.trade_news(partner, &TradeNews::PartnerAccepted);
        let t = state.trade.as_ref().unwrap();
        assert_eq!(
            (t.theirs.as_slice(), t.their_money, t.agreed),
            (&[1001][..], 300, true)
        );
        state.trade_news(partner, &TradeNews::Done);
        assert!(state.trade.is_none());
    }
}
