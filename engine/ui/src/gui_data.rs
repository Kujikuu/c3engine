//! What a data-driven UI's bindings read from the game: the hero's numbers, name and bag.

use crate::GameView;
use crate::gui::GuiData;
use c3_game::Player;

/// The hero, as bindings see it (`hero.life`, `hero.bag`, ...).
pub(crate) struct HeroData<'a> {
    pub view: &'a GameView<'a>,
    pub player: &'a Player,
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
            // 1 while the hero talks with an NPC.
            "hero.talking" => u8::from(p.dialogue.is_some()).into(),
            // The NPC whose shop is open (0: none).
            "hero.shop" => p.shop.unwrap_or(0).into(),
            "hero.profession" => p.profession.into(),
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
            // What the NPC says now, a line each.
            "dialogue.text" => (self.player.dialogue.as_ref()).map(|c| c.page.lines.join("\n")),
            _ => self.number(key).map(|n| format!("{n}")),
        }
    }

    fn items(&self, key: &str) -> Vec<u32> {
        match key {
            "hero.bag" => self.player.bag.iter().map(|i| i.kind).collect(),
            "hero.equipment" => self.player.equipment.iter().map(|i| i.kind).collect(),
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
