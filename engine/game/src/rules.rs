//! What the game decides: monster and item types, player stats, combat formulas, experience
//! and drops. The simulation asks; a game (`games/conquer`) answers from its tables.

use crate::rng::Rng;
use serde::{Deserialize, Serialize};

/// Fighting numbers of a player or monster.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    pub max_life: u32,
    pub max_mana: u32,
    pub attack_min: u32,
    pub attack_max: u32,
    pub defence: u32,
    /// Chance to avoid hits (game-defined scale).
    pub dodge: u32,
    /// Accuracy (game-defined scale).
    pub dexterity: u32,
    /// Cells from the target within which it can hit.
    pub attack_range: u32,
    /// Milliseconds between hits.
    pub attack_ms: u32,
    /// Milliseconds per one-cell step.
    pub step_ms: u32,
}

impl Stats {
    /// These stats at `percent` (an item's bonus at its quality), rounded down.
    pub fn scaled(self, percent: u32) -> Stats {
        let at = |v: u32| (u64::from(v) * u64::from(percent) / 100) as u32;
        Stats {
            max_life: at(self.max_life),
            max_mana: at(self.max_mana),
            attack_min: at(self.attack_min),
            attack_max: at(self.attack_max),
            defence: at(self.defence),
            dodge: at(self.dodge),
            dexterity: at(self.dexterity),
            ..self
        }
    }

    /// These stats with an item's bonus added (life, mana, attack, defence, dodge, accuracy).
    pub fn with_bonus(mut self, bonus: &Stats) -> Stats {
        self.max_life += bonus.max_life;
        self.max_mana += bonus.max_mana;
        self.attack_min += bonus.attack_min;
        self.attack_max += bonus.attack_max;
        self.defence += bonus.defence;
        self.dodge += bonus.dodge;
        self.dexterity += bonus.dexterity;
        self
    }
}

/// One side of a hit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fighter {
    pub level: u32,
    pub stats: Stats,
    /// Named numbers formulas read beside the stats: `monster` (its type; 0 for a player) and
    /// a monster type's numbers, `status_<name>` (a number of the statuses it bears, summed),
    /// and for players `profession`, every attribute, `worn_<name>` (an item attribute summed
    /// over what it wears), `worn<slot>` (the item type worn there) and `worn<slot>_<name>`
    /// (that item's attribute). Broken items count for nothing.
    pub facts: std::collections::BTreeMap<String, i64>,
}

/// [`MonsterType::hunts`]: criminals (blue names).
pub const HUNTS_CRIMINALS: u8 = 1;
/// Players with a red name.
pub const HUNTS_RED: u8 = 2;
/// Players with a black name.
pub const HUNTS_BLACK: u8 = 4;
/// Players of good name.
pub const HUNTS_GOOD: u8 = 8;

/// A kind of monster.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MonsterType {
    pub id: u32,
    pub name: String,
    /// The game's model (look) id.
    pub look: u32,
    pub level: u32,
    pub stats: Stats,
    /// Cells within which it notices players.
    pub view_range: u32,
    /// Attacks players on sight (otherwise only when attacked).
    pub aggressive: bool,
    /// Players it attacks on sight by their name (guards: criminals), as [`HUNTS_CRIMINALS`]
    /// and the others; 0: none.
    pub hunts: u8,
    /// Hit, it calls the idle monsters of its kind around to join in.
    pub assist: bool,
    /// Below this percent of its life it runs from its foe; 0: never.
    pub flee_percent: u32,
    /// Never gives up a chase and is never stunned.
    pub boss: bool,
    /// A skill it casts instead of hitting, `skill_percent` of the time; 0: none.
    pub skill: u32,
    pub skill_percent: u32,
    /// The game's other numbers for it, for formulas (its fighter's facts).
    pub facts: std::collections::BTreeMap<String, i64>,
}

/// What a monster type may drop: money up to `money`, and items with their chance.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Loot {
    pub money: u64,
    /// Chance in percent of dropping money at all (100: always).
    pub money_percent: u32,
    /// Item types and their chance in percent.
    pub items: Vec<(u32, u32)>,
}

/// Whose stats [`Rules::player_stats`] is asked for.
#[derive(Debug, Clone, Copy)]
pub struct StatsFor<'a> {
    pub level: u32,
    pub profession: u32,
    /// The game's attributes (every one the settings name, defaults included).
    pub attributes: &'a std::collections::BTreeMap<String, i64>,
}

impl StatsFor<'_> {
    /// A player of `level` with nothing else said.
    pub fn level(level: u32) -> StatsFor<'static> {
        static NONE: std::sync::OnceLock<std::collections::BTreeMap<String, i64>> =
            std::sync::OnceLock::new();
        StatsFor {
            level,
            profession: 0,
            attributes: NONE.get_or_init(Default::default),
        }
    }
}

/// A kind of item.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemType {
    pub id: u32,
    pub name: String,
    /// Price in a shop.
    pub price: u64,
    /// Life restored when used (potions); 0 if it can't be used that way.
    pub restores_life: u32,
    pub restores_mana: u32,
    /// The equipment slot it is worn in (game-defined numbers); `None` if not worn.
    pub slot: Option<u8>,
    /// What wearing it adds (see [`Stats::with_bonus`]).
    pub bonus: Stats,
    /// Most of it one bag slot holds (0 or 1: one each).
    pub stack: u32,
    /// Durability of a new one; 0: it never wears out.
    pub durability: u32,
    /// The currency it is priced in (one of the settings' `currencies`); empty: the money.
    pub currency: String,
    /// The level a player needs to wear it (0: any).
    pub requires_level: u32,
    /// The game's other numbers for it (requirements, kinds...), for formulas.
    pub facts: std::collections::BTreeMap<String, i64>,
}

/// What a skill is aimed at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillTarget {
    /// A monster (the caster's target).
    #[default]
    Enemy,
    /// The caster itself (and those around it, with an area).
    Myself,
    /// A cell of the ground (with an area: everyone around it).
    Ground,
}

/// What a skill does to those it reaches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillKind {
    #[default]
    Damage,
    Heal,
    /// Only puts its status on those it reaches (a buff or a curse).
    Status,
}

/// A status effect or buff: how long it lasts, how it changes stats and life.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusType {
    pub id: u32,
    pub name: String,
    /// Milliseconds it lasts.
    pub duration_ms: u32,
    /// Life it gives (or takes, below 0) every `tick_ms`.
    pub life_per_tick: i32,
    pub tick_ms: u32,
    /// Percent changes: +50 is half again, -30 is 30% less.
    pub attack_percent: i32,
    pub defence_percent: i32,
    /// Dodge and accuracy.
    pub dodge_percent: i32,
    /// Walking and hitting speed.
    pub speed_percent: i32,
    /// Its bearer can't act (stunned, frozen).
    pub stun: bool,
    /// The effect shown on its bearer while it lasts.
    pub effect: String,
    /// The game's other numbers for it; a bearer's facts get `status_<name>`, summed over
    /// what it bears.
    pub facts: std::collections::BTreeMap<String, i64>,
}

/// A skill or spell players learn.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillType {
    pub id: u32,
    pub name: String,
    pub target: SkillTarget,
    pub kind: SkillKind,
    /// Cells from the caster it reaches.
    pub range: u32,
    /// Cells around where it lands that it reaches too; 0: only its target.
    pub area: u32,
    /// Milliseconds before it can be cast again.
    pub cooldown_ms: u32,
    /// Mana a cast costs.
    pub mana: u32,
    /// How strong it is (the game's formula reads it: damage, or life healed).
    pub power: u32,
    /// The level a player needs to learn it.
    pub level: u32,
    /// The effect shown on each one it reaches (or where it lands).
    pub effect: String,
    /// The status it puts on those it reaches; 0: none.
    pub status: u32,
    /// A player attribute a cast costs beside mana (an XP bar, stamina); empty: none.
    pub cost_attribute: String,
    /// How much of it.
    pub cost: i64,
}

/// Something a monster leaves on the ground when it dies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drop {
    Money(u64),
    Item(u32),
}

/// Rules of type `T` among `rules` and the rules they wrap.
pub fn find_rules<T: Rules>(rules: &dyn Rules) -> Option<&T> {
    let mut at = Some(rules);
    while let Some(r) = at {
        if let Some(found) = (r as &dyn std::any::Any).downcast_ref::<T>() {
            return Some(found);
        }
        at = r.wrapped();
    }
    None
}

/// A game's rules. They are `Any`, so the game's own code (a protocol front, say) can reach
/// its rules type through a simulation's `rules()` (see [`find_rules`]).
pub trait Rules: std::any::Any {
    fn monster(&self, kind: u32) -> Option<&MonsterType>;

    fn item(&self, kind: u32) -> Option<&ItemType>;

    /// A player's stats at `level`, before equipment (whose bonuses the simulation adds).
    /// A player's own stats (before what it wears): by its level, profession and attributes.
    fn player_stats(&self, who: &StatsFor) -> Stats;

    /// Experience needed to rise from `level` to the next; `None` at the top level.
    fn level_exp(&self, level: u32) -> Option<u64>;

    /// Damage of one hit, 0 for a miss.
    fn hit(&self, attacker: &Fighter, target: &Fighter, rng: &mut Rng) -> u32;

    /// Experience player `attacker` earns for dealing `damage` to a monster.
    fn hit_exp(&self, attacker: &Fighter, target: &Fighter, damage: u32) -> u64;

    /// Extra experience for the killing blow.
    fn kill_exp(&self, attacker: &Fighter, target: &Fighter) -> u64;

    /// What a monster of `kind` drops.
    fn drops(&self, kind: u32, rng: &mut Rng) -> Vec<Drop>;

    /// What a monster of `kind` may drop and how likely, for editors (`drops` rolls it).
    fn loot(&self, _kind: u32) -> Loot {
        Loot::default()
    }

    /// Item types NPC `npc` sells (empty if it has no shop).
    fn shop(&self, npc: u32) -> &[u32];

    /// A skill players may learn.
    fn skill(&self, _id: u32) -> Option<&SkillType> {
        None
    }

    /// A status effect skills may put on.
    fn status(&self, _id: u32) -> Option<&StatusType> {
        None
    }

    /// Damage `skill` cast by `attacker` deals to `target`: its power plus half an attack.
    fn skill_hit(
        &self,
        attacker: &Fighter,
        target: &Fighter,
        skill: &SkillType,
        rng: &mut Rng,
    ) -> u32 {
        let a = &attacker.stats;
        let roll = rng.range(a.attack_min.into(), a.attack_max.max(a.attack_min).into()) as u32;
        (skill.power + roll / 2)
            .saturating_sub(target.stats.defence / 2)
            .max(1)
    }

    /// Life `skill` cast by `healer` restores.
    fn skill_heal(&self, _healer: &Fighter, skill: &SkillType) -> u32 {
        skill.power
    }

    /// Whether `player` may wear `item` (beyond its level, which the engine checks).
    fn can_wear(&self, _player: &Fighter, _item: &ItemType) -> bool {
        true
    }

    /// Money a shop pays for an item (TQ: a third of its price).
    fn sell_price(&self, kind: u32) -> u64 {
        self.item(kind).map_or(0, |i| i.price / 3)
    }

    /// What went wrong answering since the last call (a game's formula failing), for the
    /// simulation's warnings.
    /// The rules these wrap (a layer over a game's rules, such as a project's formulas).
    fn wrapped(&self) -> Option<&dyn Rules> {
        None
    }

    fn take_problems(&self) -> Vec<String> {
        Vec::new()
    }
}
