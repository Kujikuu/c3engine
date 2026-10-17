# Decisions Log

Append-only. Format: date · decision · why.

- 2026-10-18 · Build a new engine in **Rust**, not an editor for the old client · the user wants a general engine ("any game"), like TQ's C3 engine.
- 2026-10-18 · Conquer is a **game module** (`games/conquer`), never part of the engine · keeps the engine reusable for future games.
- 2026-10-18 · TQ formats are **import/export plugins**; the engine has its own native formats · clean engine model + legacy compatibility.
- 2026-10-18 · Location `D:\Conquer2\c3engine`, its own git repo · user choice.
- 2026-10-18 · Planned stack: wgpu, winit, egui, glam, tokio · modern, cross-API, Rust-native.

## Open
- ECS: `bevy_ecs` standalone vs custom.
- Scripting: Rhai (pure Rust) vs Lua (`mlua`).
- Visual target for Conquer: exact classic 5065 first, or modernized from the start.
