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
mod conversation;
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
pub use host::{ScriptError, ScriptHost, TALK, script_names};
pub use quest::{Goal, Quest};
pub use rhai;

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
    ("message(text)", "Shows the player a system message"),
    ("teleport(map, x, y)", "Moves the player to a cell of a map"),
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
    ("print(value)", "Writes to the console"),
    (
        "global::NAME",
        "A const declared at the top of the script, read in a function",
    ),
];

#[cfg(test)]
mod tests;
