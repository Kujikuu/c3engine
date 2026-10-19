//! c3-script: the game's scripts, in Rhai (see docs/DECISIONS.md). A project keeps them in
//! `scripts/*.rhai`; NPCs name the script they run. [`ScriptHost`] compiles and runs them in
//! a sandbox, and gives the editor their errors with line and column.
//!
//! An NPC script defines `fn talk()`, which builds the first page of the dialogue:
//!
//! ```rhai
//! const PRICE = 100;
//!
//! fn talk() {
//!     text("Welcome, " + player_name() + "!");
//!     option("Buy a potion (" + global::PRICE + " silver)", "buy");
//!     option("Goodbye");
//! }
//!
//! fn buy() {
//!     if take_money(global::PRICE) { give_item(1000000); text("Here you go."); }
//!     else { text("You can't afford it."); }
//! }
//! ```
//!
//! Each answer names the function run when picked; an answer without one closes the dialogue.
//! Functions see the script's top-level constants through `global::`.

mod api;
pub mod blueprint;
mod clock;
mod conversation;
pub mod debug;
mod dialog;
pub mod formulas;
mod graph;
mod host;
pub mod quest;
mod tables;

pub use conversation::{Conversation, Step};
pub use dialog::{
    BountyView, DialogOption, DialogPage, Effect, GuildView, HeldItem, MapCopyView, NpcInfo,
    PlayerState, Reply, WantedView, WorldChange,
};
pub use formulas::{Formulas, RULES_SCRIPT, map_int};
pub use graph::{
    Action, Case, Choice, Condition, DIALOGUE_SUFFIX, DialogueGraph, GENERATED_DIR, GraphProblem,
    Node, NodeKind, PLACEHOLDERS, dialogue_names, generated_script, load_dialogue, save_dialogue,
};
pub use host::{EVENTS_SCRIPT, ScriptError, ScriptHost, TALK, script_names};
pub use quest::{Goal, Quest};
pub use rhai;
pub use rhai::Dynamic;

/// File extension of script files.
pub const SCRIPT_SUFFIX: &str = ".rhai";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A problem in a script, for the editor and the console.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// 1-based line and column; 0 when the problem has no place.
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            line: 0,
            column: 0,
            message: message.into(),
        }
    }
}

/// Every function scripts can call, with what it does, for the editor's reference.
pub const API: &[(&str, &str)] = &[
    (
        "players_near_with(status, range) / add_status_near(status, range)",
        "Other living players within `range` cells bearing `status`, counted; `status` put on those who lack it",
    ),
    (
        "npc_count(field, value)",
        "The NPCs on the player's map: all, or those of `name` or `kind` `value`",
    ),
    (
        "create_npc(map, x, y, name, look, kind, script)",
        "An NPC made on map `map` (here, or carried there) with its look, kind and talk script",
    ),
    (
        "make_map_copy(base, x, y, level) / map_copy(id, field) / set_map_copy(id, field, value) / map_copy_of(owner)",
        "Maps made while playing (homes): a copy of map `base` owned by the player (its new id), its `base`, `x`, `y` or `level` read and set, an owner's latest one (0: none)",
    ),
    (
        "create_sub_guild(name, level, cost, fund) / change_sub_leader(name, level)",
        "A sub-guild under the guild the player leads, led by its first teammate here (its guild, of `level`); the parent pays `cost`. A sub-guild handed to that teammate",
    ),
    (
        "team_all_named(name)",
        "Whether the player's teammates here (itself left out) are all called `name`",
    ),
    (
        "police_list(name, level, listed) / police_at(index) / police_has(name)",
        "The police wanted list every map shares: a player put on (highest level first) or taken off; the name at `index` (empty past the last); whether a name is on it",
    ),
    (
        "guild_member(index)",
        "The name of member `index` (from 0) of the player's guild; empty past the last ({member} in text: at the iterator)",
    ),
    (
        "record_bounty(killer) / bounty_count(role, unset) / set_bounty(kind, bonus) / clear_bounties()",
        "Bounties on killers every map shares: one recorded on the player's killer; the player's rows as `target` or `hunter` without or with a reward; the oldest unset reward on its killer set; its unset rows removed",
    ),
    (
        "claimable_bounty(kind) / take_bounty()",
        "The oldest reward the player may claim as the killer: its amount (or kind, `true`); taken",
    ),
    (
        "wanted_at(index) / wanted_bounty(id) / wanted_target(id)",
        "The wanted list every map shares: the entry id at `index` (0: none), its bounty and target",
    ),
    (
        "add_wanted(target, bounty) / set_wanted_bounty(id, bounty)",
        "A wanted entry added (the player pays; its new id), its bounty changed (0 removes it)",
    ),
    (
        "text_var(name) / set_text_var(name, text)",
        "A text kept with the player (empty when unset); setting it empty removes it",
    ),
    (
        "open_storage() / saved_money() / add_saved_money(n)",
        "The NPC opens its storage for the player; the money kept in storage, added to (or, \
         below 0, taken from: false when there isn't so much)",
    ),
    (
        "add_status_for(status, secs) / remove_status(status)",
        "A status on the player for `secs` (0: its own time); a status taken off",
    ),
    (
        "drop_at(item, map, x, y) / remove_monsters(map, kind) / map_travel(from, to, x, y)",
        "An item left on a cell of a map; a map's living monsters of a type gone; every \
         player on map `from` sent to a cell of map `to` (each: this map only)",
    ),
    (
        "create_trap(kind, look, map, x, y) / erase_trap()",
        "A trap on a cell of a map (this one): stepping on it raises custom event \
         `trap_<kind>`; the trap the player set off, gone",
    ),
    (
        "world_timer(name, secs)",
        "Runs world event `name` (a custom event with no player) after `secs`",
    ),
    (
        "transform(monster, secs, skill)",
        "Turns the player into a monster type for a while (its look and fighting numbers), \
         skill `skill`'s effect shown first (0: none)",
    ),
    (
        "room_cue_text(name, a, text) / team_effect(effect) / map_timer(map, name, count) / \
         erase_npcs(kind)",
        "A room cue with a text; an effect on each teammate here; timer `name` at once for \
         the first `count` players on a map (all: below 1); this map's NPCs of a kind gone",
    ),
    (
        "loot_item(type) / loot_money(n)",
        "An item of `type` (or money) lies where the monster being killed fell (`killed_<type>` \
         events), else where the player stands",
    ),
    (
        "used_item()",
        "The id of the item being used, while its `use_<type>` custom event runs (0 otherwise)",
    ),
    (
        "npc_look(npc) / set_npc_look(npc, look) / die() / player_uid() / clear_attrs(prefix)",
        "An NPC's look here and a new one for everyone; the player dies; a number for the \
         player that lasts; the player's attributes starting with `prefix` gone",
    ),
    (
        "team_here() / team_holding(item) / team_give(item) / team_take(item) / team_say(text)",
        "The player's living teammates on its map, how many of them hold an item type; each of \
         them given or relieved of one, or told something",
    ),
    (
        "guild_id() / guild_rank() / guild_name(id) / guild_fund(id) / guild_master(id) / \
         guild_members(id) / guild_named(name) / next_guild(after) / map_owner(map) / \
         guild_relation(guild, other, ally)",
        "The player's guild and rank (0: none); a guild's name, fund, main guild, member \
         count; a guild by name; the guild after `after` (0: none left); a map's owning guild; \
         whether `other` is an ally (or enemy) of `guild`",
    ),
    (
        "create_guild(name, fund) / destroy_guild() / add_guild_fund(id, n) / \
         set_guild_rank(member, rank) / join_guild(id, rank) / \
         set_guild_relation(other, ally, on) / set_map_owner(map, guild)",
        "Guild changes every map shares (rank 100 leads; rank 0 leaves); whether they could be \
         made",
    ),
    (
        "room_cue(name, a, b)",
        "client_cue for everyone who sees the player",
    ),
    ("leads_team()", "Whether the player leads a team"),
    (
        "team_travel(map, x, y)",
        "The player's team (the leader and every member on its map) goes to a cell of game map \
         `map`",
    ),
    (
        "monsters_named(map, name)",
        "How many living monsters of type `name` are on game map `map` (the player's own map; \
         0 for others)",
    ),
    (
        "client_cue(name, a, b)",
        "Sends the player's client cue `name` with two numbers: a message the game's front \
         knows by that name (other fronts let it pass)",
    ),
    (
        "input_option(label, length, next)",
        "An answer with a text box of `length` characters; `next` runs with what was typed",
    ),
    (
        "accept() / accept_number()",
        "What the player typed in the answer's text box (as a number: 0 if it isn't one); \
         {accept} in fill()",
    ),
    (
        "pick_row(table, column, value, weight)",
        "The key of a row of `table` whose `column` is `value`, drawn at random weighted by its \
         `weight` column; 0 when none",
    ),
    (
        "effect_at(map, x, y, effect)",
        "Plays effect `effect` once on a cell of game map `map`, for everyone on the map",
    ),
    (
        "map_players(map, alive)",
        "How many players are on game map `map` (only the living with `alive`)",
    ),
    (
        "held_of(type)",
        "The ids of the player's items of `type` in the bag, for the item functions",
    ),
    (
        "erase_item(id)",
        "Takes away the player's item `id` from the bag; false when it can't",
    ),
    (
        "set_given_attr(type, name, value)",
        "The item of `type` given last (give_item) gets `value` as attribute `name`, or as its \
         durability",
    ),
    (
        "level_after(exp) / percent_after(exp)",
        "The level, and the whole percent into it, the player would be at with `exp` more \
         experience (the levels table walked; nothing changes)",
    ),
    (
        "exp_for_time(units)",
        "The experience that takes the player `units` further by the levels table's `time` \
         column (how long each level takes)",
    ),
    (
        "pick_worn(name, op, value)",
        "A random slot whose worn item's `name` (an item attribute, or \"type\") compares \
         by `op` (==, <, <=, >, >=) with `value`; 0 when none does",
    ),
    (
        "raise(name, a, b, c)",
        "Signals `name` with three numbers: every script and blueprint with fn name(a, b, c) \
         (a Signal event) runs it after this step",
    ),
    (
        "worn_id(slot)",
        "The id of the item worn in `slot` (0: none), for the item functions",
    ),
    (
        "set_level(n) / set_exp(n) / set_profession(n) / set_body(n)",
        "Sets the player's level, experience, profession or body (rebirth, promotion)",
    ),
    (
        "move_npc(npc, map, x, y)",
        "Moves NPC `npc` to a cell of game map `map` (another map: it leaves this one for it)",
    ),
    (
        "item_type_of(id) / set_item_type(id, type)",
        "The type of the player's item `id` (bag or worn; 0: none); changes it (an upgrade)",
    ),
    (
        "item_attr(id, name) / set_item_attr(id, name, v)",
        "A number of the player's item `id` (the settings' item attributes)",
    ),
    (
        "item_durability(id) / set_item_durability(id, v) / item_worn(id)",
        "The durability of the player's item `id`, and whether it is worn",
    ),
    ("exp()", "The player's experience toward its next level"),
    (
        "map_pk_free()",
        "Whether fights between players here bring no crime or PK points",
    ),
    (
        "drop_money(n) / drop_bag_items(percent) / lose_exp(n)",
        "Death's costs: money dropped where the player stands, each bag item with a chance, \
         experience lost",
    ),
    (
        "killed_by_monster()",
        "The type of the monster that last killed the player (0: none, or a player)",
    ),
    (
        "body() / hair() / set_hair(n)",
        "The player's look: body and hair as the game numbers them; changes the hair",
    ),
    ("text(line)", "Adds a line to what the NPC says"),
    (
        "option(label, next)",
        "Adds an answer; picking it runs fn next()",
    ),
    ("option(label)", "Adds an answer that closes the dialogue"),
    (
        "open_shop()",
        "Opens the NPC's shop (its goods in the game's tables)",
    ),
    ("npc_id()", "The NPC's id"),
    ("npc_name()", "The NPC's name"),
    ("player_name()", "The player's name"),
    ("level()", "The player's level"),
    ("money()", "The player's money"),
    ("give_money(n)", "Gives the player n money"),
    (
        "take_money(n)",
        "Takes n money if the player has it; true if taken",
    ),
    (
        "item_count(item)",
        "How many of an item type the player carries",
    ),
    (
        "has_item(item[, n])",
        "Whether the player carries (n of) an item",
    ),
    (
        "give_item(item[, n])",
        "Gives the player (n of) an item when the bag has room for it; whether it did",
    ),
    (
        "take_item(item[, n])",
        "Takes (n of) an item if carried; true if taken",
    ),
    ("flag(name)", "A variable kept with the player (0 if unset)"),
    (
        "set_flag(name, value)",
        "Sets a variable kept with the player",
    ),
    ("currency(name)", "How much of a currency the player has"),
    ("give_currency(name, n)", "Gives the player n of a currency"),
    (
        "take_currency(name, n)",
        "Takes n of a currency if the player has it; true if taken",
    ),
    ("knows_skill(skill)", "Whether the player knows a skill"),
    (
        "learn_skill(skill)",
        "Teaches the player a skill; true if it was new",
    ),
    ("message(text)", "Shows the player a system message"),
    ("teleport(map, x, y)", "Moves the player to a cell of a map"),
    (
        "start_timer(name, seconds)",
        "Runs on_timer(name) of the events script after the seconds",
    ),
    (
        "lookup(table, key, column)",
        "A value from a game table, or () if no such row",
    ),
    ("has_row(table, key)", "Whether a game table has the row"),
    (
        "quest_state(id)",
        "A quest's state: 0 not taken, 1 taken, 2 done",
    ),
    ("quest_name(id)", "A quest's name, from the quests table"),
    (
        "quest_progress(id)",
        "Monsters killed or items carried towards a quest's goal",
    ),
    ("quest_ready(id)", "Whether a taken quest's goal is met"),
    (
        "start_quest(id)",
        "Gives the player a quest (if not taken and the level is met); true if given",
    ),
    (
        "complete_quest(id)",
        "Completes a ready quest: takes the items, gives the rewards; true if done",
    ),
    ("give_exp(amount)", "Gives the player experience"),
    (
        "heal(amount) / hurt(amount)",
        "Gives back or takes life (never below 1)",
    ),
    ("life() / max_life()", "The player's life and most life"),
    (
        "add_status(id) / has_status(id)",
        "Puts the player under a status, or asks",
    ),
    ("map_id() / player_x() / player_y()", "Where the player is"),
    (
        "world_flag(name) / set_world_flag(name, value)",
        "A variable every player's scripts share (unset: 0)",
    ),
    ("random(low, high)", "A whole number from low to high"),
    (
        "play_effect(name) / play_sound(path)",
        "An effect on the player, a sound where it is",
    ),
    ("announce(text)", "Tells every player on the map"),
    (
        "in_team() / team_size() / team_leader() / team_message(text)",
        "The player's team: whether in one, how many, its leader, telling it",
    ),
    (
        "profession()",
        "The profession chosen when the character was made (0: none)",
    ),
    (
        "mana() / max_mana() / add_mana(n)",
        "The player's mana; gives (n < 0 spends) some",
    ),
    (
        "attr(name) / add_attr(name, n) / set_attr(name, v)",
        "The player's attribute `name` (Project Settings > Gameplay > attributes); add_attr is \
         false (and changes nothing) when it would go below its lowest",
    ),
    (
        "set_record(map, x, y) / go_record()",
        "Sets where the player comes back to; sends it there",
    ),
    ("bag_space()", "Free places in the bag"),
    (
        "worn_attr(slot, name) / set_worn_attr(slot, name, v)",
        "A number of the item worn in `slot` (the settings' item attributes: plus, gems...)",
    ),
    (
        "count_items_between(low, high) / take_items_between(low, high, n)",
        "Items carried whose types are from `low` to `high`; takes n of them (false: too few)",
    ),
    (
        "forget_skill(id)",
        "Forgets a skill; false if the player didn't know it",
    ),
    ("clock()", "Seconds since 1970 (for timestamps)"),
    (
        "pk() / add_pk(n)",
        "The player's PK points; gives (or with n < 0 takes) some",
    ),
    (
        "worn(slot)",
        "The item type worn in equipment slot `slot` (0: none)",
    ),
    (
        "picture(id)",
        "Shows picture `id` (the NPC's face) with what the NPC says",
    ),
    (
        "time_within(mode, window)",
        "Whether now is in `window`: mode 0 dates `Y-M-D h:m Y-M-D h:m`, 1 yearly `M-D h:m M-D h:m`,          2 monthly `day h:m day h:m`, 3 weekly `weekday h:m weekday h:m` (0 Sunday), 4 daily          `h:m h:m`, 5 hourly `minute minute`",
    ),
    (
        "open_window(id)",
        "Opens the game UI's dialog id for the player",
    ),
    (
        "spawn_monster(kind, x, y)",
        "A monster of type kind appears at x, y",
    ),
    (
        "walk_to(x, y, tag)",
        "Walks the player there; the events' on_arrive(tag) runs when it gets there",
    ),
    ("print(value)", "Writes to the console"),
    (
        "global::NAME",
        "A const declared at the top of the script, read in a function",
    ),
];

#[cfg(test)]
mod tests;
