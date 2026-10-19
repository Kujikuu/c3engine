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
pub use dialog::{DialogOption, DialogPage, Effect, NpcInfo, PlayerState, Reply};
pub use formulas::{Formulas, RULES_SCRIPT};
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
    ("give_item(item[, n])", "Gives the player (n of) an item"),
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
