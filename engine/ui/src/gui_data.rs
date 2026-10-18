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
            _ => return None,
        })
    }

    fn text(&self, key: &str) -> Option<String> {
        match key {
            "hero.name" => Some(self.player.name.clone()),
            _ => self.number(key).map(|n| format!("{n}")),
        }
    }

    fn items(&self, key: &str) -> Vec<u32> {
        match key {
            "hero.bag" => self.player.bag.iter().map(|i| i.kind).collect(),
            "hero.equipment" => self.player.equipment.iter().map(|i| i.kind).collect(),
            _ => Vec::new(),
        }
    }
}
