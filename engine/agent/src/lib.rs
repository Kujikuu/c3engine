//! c3-agent: what AI agents can do with a project, as tools.
//!
//! Each tool reads or changes a project through `c3-edit`'s documents and commands, the same
//! way the Studio's own tools do, so every change is undoable and lands in the same history.
//! A tool runs on a [`Workspace`]: the [`Headless`] one opens a project on its own (for agents
//! working without the Studio), the Studio is one too (an agent driving the open Studio,
//! reached through [`live`]). `c3-mcp` serves these tools over MCP.
//!
//! ```text
//! MCP client ─► c3-mcp ─┬─► Headless workspace ───────────────► project files
//!                       └─► live link ─► Studio workspace ─► open documents (undo, panels)
//! ```

mod assets;
mod content;
mod context;
mod data;
mod headless;
pub mod live;
mod maps;
mod playtest;
mod tools;
mod workspace;

pub use headless::Headless;
pub use tools::{ToolSpec, call, tools};
pub use workspace::{DocRef, Workspace};

#[cfg(test)]
mod tests;
