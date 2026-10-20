//! A project's formulas over any game's own rules: when `scripts/rules.rhai` or a blueprint
//! defines one of the rules' formulas (`hit`, `kill_exp`, `drops`, …), it answers instead of
//! the game's code. (The engine's data rules read the formulas themselves.)

use crate::data_rules::{drops_from, fighter_map, skill_map, uint};
use c3_game::{
    Drop, Fighter, ItemType, Loot, MonsterType, Rng, Rules, SkillType, Stats, StatsFor, StatusType,
};
use c3_script::Formulas;
use c3_script::formulas::map_int;
use std::cell::RefCell;
use std::collections::HashSet;

pub struct FormulaRules {
    inner: Box<dyn Rules>,
    formulas: Formulas,
    /// Failures not yet reported, and those reported once already.
    problems: RefCell<(Vec<String>, HashSet<String>)>,
}

impl FormulaRules {
    pub fn new(inner: Box<dyn Rules>, formulas: Formulas) -> Self {
        Self {
            inner,
            formulas,
            problems: RefCell::default(),
        }
    }

    /// `name(args)`, if the project defines it; failures are reported once.
    fn formula<T>(
        &self,
        name: &str,
        run: impl FnOnce(&Formulas) -> Result<T, String>,
    ) -> Option<T> {
        if !self.formulas.has(name) {
            return None;
        }
        run(&self.formulas)
            .inspect_err(|e| {
                let (pending, seen) = &mut *self.problems.borrow_mut();
                let line = format!("formula {name}: {e}");
                if seen.insert(line.clone()) {
                    pending.push(line);
                }
            })
            .ok()
    }
}

impl Rules for FormulaRules {
    fn monster(&self, kind: u32) -> Option<&MonsterType> {
        self.inner.monster(kind)
    }

    fn protected(&self, sim: &c3_game::Sim, owner: &c3_game::Player, item: &c3_game::Item) -> bool {
        self.inner.protected(sim, owner, item)
    }

    fn protects_work(&self, kind: u32, at: usize) -> bool {
        self.inner.protects_work(kind, at)
    }

    fn item(&self, kind: u32) -> Option<&ItemType> {
        self.inner.item(kind)
    }

    fn player_stats(&self, who: &StatsFor) -> Stats {
        let base = self.inner.player_stats(who);
        match self.formula("player_stats", |f| crate::data_rules::stats_formula(f, who)) {
            Some(map) => Stats {
                max_life: uint(map_int(&map, "life")),
                max_mana: uint(map_int(&map, "mana")),
                attack_min: uint(map_int(&map, "attack_min")),
                attack_max: uint(map_int(&map, "attack_max")),
                defence: uint(map_int(&map, "defence")),
                dodge: uint(map_int(&map, "dodge")),
                dexterity: uint(map_int(&map, "dexterity")),
                ..base
            },
            None => base,
        }
    }

    fn level_time(&self, level: u32) -> Option<u64> {
        self.inner.level_time(level)
    }

    fn level_exp(&self, level: u32) -> Option<u64> {
        match self.formula("level_exp", |f| f.int("level_exp", (i64::from(level),))) {
            Some(exp) => (exp > 0).then_some(exp as u64),
            None => self.inner.level_exp(level),
        }
    }

    fn hit(&self, attacker: &Fighter, target: &Fighter, rng: &mut Rng) -> u32 {
        if !self.formulas.has("hit") {
            return self.inner.hit(attacker, target, rng);
        }
        let a = &attacker.stats;
        let roll = rng.range(a.attack_min.into(), a.attack_max.max(a.attack_min).into());
        let chance = rng.range(0, 99);
        let scripted = self.formula("hit", |f| {
            let mut me = fighter_map(attacker);
            me.insert("roll".into(), roll.into());
            me.insert("chance".into(), chance.into());
            f.int("hit", (me, fighter_map(target)))
        });
        scripted.map_or_else(|| self.inner.hit(attacker, target, rng), uint)
    }

    fn hit_exp(&self, attacker: &Fighter, target: &Fighter, damage: u32) -> u64 {
        self.formula("hit_exp", |f| {
            f.int(
                "hit_exp",
                (
                    fighter_map(attacker),
                    fighter_map(target),
                    i64::from(damage),
                ),
            )
        })
        .map_or_else(
            || self.inner.hit_exp(attacker, target, damage),
            |e| e.max(0) as u64,
        )
    }

    fn kill_exp(&self, attacker: &Fighter, target: &Fighter) -> u64 {
        self.formula("kill_exp", |f| {
            f.int("kill_exp", (fighter_map(attacker), fighter_map(target)))
        })
        .map_or_else(
            || self.inner.kill_exp(attacker, target),
            |e| e.max(0) as u64,
        )
    }

    fn drops(&self, kind: u32, rng: &mut Rng) -> Vec<Drop> {
        let monster = self.inner.monster(kind);
        match self.formula("drops", |f| drops_from(f, kind, monster)) {
            Some(drops) => drops,
            None => self.inner.drops(kind, rng),
        }
    }

    fn loot(&self, kind: u32) -> Loot {
        self.inner.loot(kind)
    }

    fn shop(&self, npc: u32) -> &[u32] {
        self.inner.shop(npc)
    }

    fn skill(&self, id: u32) -> Option<&SkillType> {
        self.inner.skill(id)
    }

    fn skill_at(&self, id: u32, rank: u32) -> Option<&SkillType> {
        self.inner.skill_at(id, rank)
    }

    fn status(&self, id: u32) -> Option<&StatusType> {
        self.inner.status(id)
    }

    fn skill_hit(
        &self,
        attacker: &Fighter,
        target: &Fighter,
        skill: &SkillType,
        rng: &mut Rng,
    ) -> u32 {
        if !self.formulas.has("skill_hit") {
            return self.inner.skill_hit(attacker, target, skill, rng);
        }
        let a = &attacker.stats;
        let roll = rng.range(a.attack_min.into(), a.attack_max.max(a.attack_min).into());
        let scripted = self.formula("skill_hit", |f| {
            let mut me = fighter_map(attacker);
            me.insert("roll".into(), roll.into());
            f.int("skill_hit", (me, fighter_map(target), skill_map(skill)))
        });
        scripted.map_or_else(|| self.inner.skill_hit(attacker, target, skill, rng), uint)
    }

    fn can_wear(&self, player: &Fighter, item: &ItemType) -> bool {
        match self.formula("can_wear", |f| {
            let player = crate::data_rules::fighter_map(player);
            f.int("can_wear", (player, crate::data_rules::item_map(item)))
        }) {
            Some(ok) => ok != 0,
            None => self.inner.can_wear(player, item),
        }
    }

    fn skill_heal(&self, healer: &Fighter, skill: &SkillType) -> u32 {
        self.formula("skill_heal", |f| {
            f.int("skill_heal", (fighter_map(healer), skill_map(skill)))
        })
        .map_or_else(|| self.inner.skill_heal(healer, skill), uint)
    }

    fn wrapped(&self) -> Option<&dyn Rules> {
        Some(self.inner.as_ref())
    }

    fn take_problems(&self) -> Vec<String> {
        let mut problems = self.inner.take_problems();
        problems.append(&mut self.problems.borrow_mut().0);
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_rules::DataRules;

    #[test]
    fn project_formulas_answer_over_a_games_rules() {
        let (inner, _) = DataRules::load(None);
        let formulas = Formulas::compile(
            "fn kill_exp(attacker, target) { 999 }\nfn drops(monster) { [7, #{money: 5}] }\n",
        )
        .unwrap();
        let rules = FormulaRules::new(Box::new(inner), formulas);
        let target = Fighter {
            level: 3,
            stats: Stats::default(),
            ..Default::default()
        };
        assert_eq!(rules.kill_exp(&target, &target), 999);
        // What the formulas leave out is the game's.
        assert!(rules.level_exp(1).is_some());
        let drops = rules.drops(1, &mut Rng::new(1));
        assert_eq!(drops, [Drop::Item(7), Drop::Money(5)]);
        assert!(
            c3_game::find_rules::<DataRules>(&rules).is_some(),
            "the game's rules are found"
        );
    }
}
