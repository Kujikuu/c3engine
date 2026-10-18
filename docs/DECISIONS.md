# Decisions Log

Append-only. Format: date · decision · why.

- 2026-10-18 · Build a new engine in **Rust**, not an editor for the old client · the user wants a general engine ("any game"), like TQ's C3 engine.
- 2026-10-18 · Conquer is a **game module** (`games/conquer`), never part of the engine · keeps the engine reusable for future games.
- 2026-10-18 · TQ formats are **import/export plugins**; the engine has its own native formats · clean engine model + legacy compatibility.
- 2026-10-18 · Location `D:\Conquer2\c3engine`, its own git repo · user choice.
- 2026-10-18 · Planned stack: wgpu, winit, egui, glam, tokio · modern, cross-API, Rust-native.

- 2026-10-18 · Studio UI on **eframe/egui 0.37**; the v1 map viewport draws with egui's painter · fastest path to a usable editor. The game client's renderer (`engine/render`, wgpu) comes later, and the Studio viewport can switch to it then.
- 2026-10-18 · **UI/UX and clean code are standing requirements** · user priority. Studio work is checked with screenshots; the workspace must stay `cargo clippy` clean and `cargo fmt` formatted.

## Open
- ECS: `bevy_ecs` standalone vs custom.
- Scripting: Rhai (pure Rust) vs Lua (`mlua`).
- Visual target for Conquer: exact classic 5065 first, or modernized from the start.
