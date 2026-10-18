//! Quests: the project's `quests` table says what each quest asks (kill monsters, bring
//! items, or just talk) and what it rewards. A player's quest state lives in their script
//! variables (`quest_<id>`: 0 not taken, 1 taken, 2 done; `quest_<id>_kills`), so scripts,
//! the simulation and saves all see the same thing.

use c3_data::{Column, ColumnKind, Table, Value};

/// The table quests are kept in (`tables/quests.table.ron`).
pub const QUEST_TABLE: &str = "quests";

/// What `quest_state` says of a quest.
pub const NOT_TAKEN: i64 = 0;
pub const TAKEN: i64 = 1;
pub const DONE: i64 = 2;

/// What a quest asks of the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    /// Only taking it and coming back.
    Talk,
    /// Kill `count` monsters of type `monster`.
    Kill { monster: i64, count: i64 },
    /// Bring `count` items of type `item` (handed over on completion).
    Collect { item: i64, count: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quest {
    pub id: i64,
    pub name: String,
    /// What the quest log says about it.
    pub description: String,
    /// The NPC who gives the quest (0: none in particular).
    pub giver: i64,
    pub min_level: i64,
    pub goal: Goal,
    pub reward_money: i64,
    pub reward_exp: i64,
    /// Item type rewarded (0: none).
    pub reward_item: i64,
}

/// The variable holding a quest's state.
pub fn state_flag(id: i64) -> String {
    format!("quest_{id}")
}

/// The variable counting a kill quest's kills.
pub fn kills_flag(id: i64) -> String {
    format!("quest_{id}_kills")
}

/// Goal names as the table spells them.
pub const GOALS: [&str; 3] = ["talk", "kill", "collect"];

impl Quest {
    /// An empty quest table with its columns explained.
    pub fn new_table() -> Table {
        let col = |name: &str, kind, doc: &str| Column {
            doc: doc.to_owned(),
            ..Column::new(name, kind)
        };
        let mut table = Table::new(
            QUEST_TABLE,
            vec![
                col("id", ColumnKind::Int, "Quest id"),
                col("name", ColumnKind::Text, "Shown in the quest log"),
                col(
                    "description",
                    ColumnKind::Text,
                    "What the player is asked to do",
                ),
                col("giver", ColumnKind::Int, "NPC id who gives it (0: any)"),
                col(
                    "min_level",
                    ColumnKind::Int,
                    "Lowest level that may take it",
                ),
                col("goal", ColumnKind::Text, "talk, kill or collect"),
                col(
                    "target",
                    ColumnKind::Int,
                    "Monster type (kill) or item type (collect)",
                ),
                col("count", ColumnKind::Int, "How many to kill or bring"),
                col(
                    "reward_money",
                    ColumnKind::Int,
                    "Silver given on completion",
                ),
                col(
                    "reward_exp",
                    ColumnKind::Int,
                    "Experience given on completion",
                ),
                col(
                    "reward_item",
                    ColumnKind::Int,
                    "Item type given on completion (0: none)",
                ),
            ],
        );
        table.doc = "Quests: what each asks and rewards. Dialogues start and complete them.".into();
        table
    }

    /// Every quest in `table` (rows it can't read are skipped).
    pub fn all(table: &Table) -> Vec<Quest> {
        (0..table.rows.len())
            .filter_map(|i| Self::from_row(table, i))
            .collect()
    }

    /// The quest in row `row` of `table`.
    pub fn from_row(table: &Table, row: usize) -> Option<Quest> {
        let cells = table.rows.get(row)?;
        let cell = |name: &str| table.column(name).and_then(|c| cells.get(c));
        let int = |name: &str| cell(name).and_then(Value::as_int).unwrap_or(0);
        let text = |name: &str| cell(name).and_then(Value::as_text).unwrap_or("").to_owned();
        let (target, count) = (int("target"), int("count").max(1));
        let goal = match text("goal").trim().to_ascii_lowercase().as_str() {
            "kill" => Goal::Kill {
                monster: target,
                count,
            },
            "collect" => Goal::Collect {
                item: target,
                count,
            },
            _ => Goal::Talk,
        };
        Some(Quest {
            id: table.key(row)?,
            name: text("name"),
            description: text("description"),
            giver: int("giver"),
            min_level: int("min_level"),
            goal,
            reward_money: int("reward_money"),
            reward_exp: int("reward_exp"),
            reward_item: int("reward_item"),
        })
    }

    /// How far the goal is (done, needed), for a player whose variables read through `flag`
    /// and whose items count through `item_count`.
    pub fn progress(
        &self,
        flag: impl Fn(&str) -> i64,
        item_count: impl Fn(i64) -> i64,
    ) -> (i64, i64) {
        match self.goal {
            Goal::Talk => (1, 1),
            Goal::Kill { count, .. } => (flag(&kills_flag(self.id)).min(count), count),
            Goal::Collect { item, count } => (item_count(item).min(count), count),
        }
    }
}
