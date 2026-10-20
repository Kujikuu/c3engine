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
    /// Each view group's view (`view:<group>:<name>`; a status page's base or details).
    pub views: std::collections::HashMap<String, String>,
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
    /// The attribute `hero.attr_at.<prefix>.<low>-<high>.<n>[+<group>]` names: its number and
    /// value.
    fn attr_entry(&self, key: &str) -> Option<(i64, i64)> {
        let spec = key.split_once('.')?.1.split_once('.')?.1;
        let (prefix, rest) = spec.split_once('.')?;
        let (range, n) = rest.split_once('.')?;
        let (low, high) = range.split_once('-')?;
        let (low, high): (i64, i64) = (low.parse().ok()?, high.parse().ok()?);
        let (n, group) = n.split_once('+').map_or((n, None), |(n, g)| (n, Some(g)));
        let offset = group
            .and_then(|g| self.state.views.get(g))
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        let n = n.parse::<usize>().ok()? + offset;
        let mut found: Vec<(i64, i64)> = (self.player.attributes.iter())
            .filter_map(|(name, &value)| {
                let number: i64 = name.strip_prefix(prefix)?.parse().ok()?;
                ((low..=high).contains(&number) && value > 0).then_some((number, value))
            })
            .collect();
        found.sort_unstable();
        found.get(n).copied()
    }
}

impl GuiData for HeroData<'_> {
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
            "hero.running" => f64::from(u8::from(p.running)),
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
                let name = key.strip_prefix("hero.attr.")?;
                p.attributes.get(name).copied().unwrap_or(0) as f64
            }
        })
    }

    fn text(&self, key: &str) -> Option<String> {
        match key {
            "hero.name" => Some(self.player.name.clone()),
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
            // The hero's skill `n` (its type), none past the last.
            _ if key.starts_with("skill.") => {
                let n: usize = key.strip_prefix("skill.")?.parse().ok()?;
                Some(self.player.skills.get(n)?.to_string())
            }
            "storage.amount" | "trade.amount" | "dialogue.typed" => {
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
            _ => self.number(key).map(|n| format!("{n}")),
        }
    }

    fn item_looks(&self, key: &str) -> Vec<u32> {
        let sim = self.view.sim;
        let p = self.player;
        let of = |items: &mut dyn Iterator<Item = &c3_game::Item>| -> Vec<u32> {
            items.map(|i| sim.item_look(i)).collect()
        };
        match key {
            "hero.bag" => of(&mut p.bag.iter()),
            "hero.equipment" => of(&mut p.equipment.iter()),
            _ if key.starts_with("hero.worn.") => {
                let slot: Option<u8> = key.strip_prefix("hero.worn.").and_then(|s| s.parse().ok());
                of(&mut p
                    .equipment
                    .iter()
                    .filter(|i| slot.is_some() && sim.slot_of(i) == slot))
            }
            "trade.mine" => of(&mut (self.state.trade.iter())
                .flat_map(|t| &t.mine)
                .filter_map(|id| p.bag.iter().find(|i| i.id == *id))),
            _ if key.starts_with("work.") => {
                let n: usize = key
                    .strip_prefix("work.")
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                of(&mut (self.state.work.get(n))
                    .and_then(|id| p.bag.iter().find(|i| i.id == *id))
                    .into_iter())
            }
            "storage.items" => (self.state.storage)
                .and_then(|npc| p.storage.get(&npc))
                .map(|items| of(&mut items.iter()))
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    fn items(&self, key: &str) -> Vec<u32> {
        match key {
            "hero.bag" => self.player.bag.iter().map(|i| i.kind).collect(),
            "hero.equipment" => self.player.equipment.iter().map(|i| i.kind).collect(),
            // What the hero wears in slot `n` (`hero.worn.<n>`).
            _ if key.starts_with("hero.worn.") => {
                let slot: Option<u8> = key.strip_prefix("hero.worn.").and_then(|s| s.parse().ok());
                (self.player.equipment.iter())
                    .filter(|i| slot.is_some() && self.view.sim.slot_of(i) == slot)
                    .map(|i| i.kind)
                    .collect()
            }
            // What the partner offers, and what the hero does.
            "trade.theirs" => self
                .state
                .trade
                .as_ref()
                .map(|t| t.theirs.clone())
                .unwrap_or_default(),
            "trade.mine" => (self.state.trade.as_ref().into_iter())
                .flat_map(|t| &t.mine)
                .filter_map(|id| self.player.bag.iter().find(|i| i.id == *id))
                .map(|i| i.kind)
                .collect(),
            // Workbench slot `n`: the item put there.
            _ if key.starts_with("work.") => {
                let n: usize = key
                    .strip_prefix("work.")
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                (self.state.work.get(n))
                    .and_then(|id| self.player.bag.iter().find(|i| i.id == *id))
                    .map(|i| vec![i.kind])
                    .unwrap_or_default()
            }
            // What the open warehouse keeps.
            "storage.items" => (self.state.storage)
                .and_then(|npc| self.player.storage.get(&npc))
                .map(|items| items.iter().map(|i| i.kind).collect())
                .unwrap_or_default(),
            // What the open shop sells.
            "shop.items" => (self.player.shop)
                .map(|npc| self.view.sim.rules().shop(npc).to_vec())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    fn rows(&self, key: &str) -> (Vec<String>, Option<usize>) {
        match key {
            // The hero's team: its members' names, the leader first.
            "hero.team" => {
                let names = self.player.mates.iter().map(|m| m.name.clone());
                (names.collect(), None)
            }
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
