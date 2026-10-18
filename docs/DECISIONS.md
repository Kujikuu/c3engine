# Decisions Log

Append-only. Format: date · decision · why.

- 2026-10-18 · Build a new engine in **Rust**, not an editor for the old client · the user wants a general engine ("any game"), like TQ's C3 engine.
- 2026-10-18 · Conquer is a **game module** (`games/conquer`), never part of the engine · keeps the engine reusable for future games.
- 2026-10-18 · TQ formats are **import/export plugins**; the engine has its own native formats · clean engine model + legacy compatibility.
- 2026-10-18 · Location `D:\Conquer2\c3engine`, its own git repo · user choice.
- 2026-10-18 · Planned stack: wgpu, winit, egui, glam, tokio · modern, cross-API, Rust-native.

- 2026-10-18 · Studio UI on **eframe/egui 0.37**; the v1 map viewport draws with egui's painter · fastest path to a usable editor. The game client's renderer (`engine/render`, wgpu) comes later, and the Studio viewport can switch to it then.
- 2026-10-18 · **UI/UX and clean code are standing requirements** · user priority. Studio work is checked with screenshots; the workspace must stay `cargo clippy` clean and `cargo fmt` formatted.

- 2026-10-18 · **Course correction (drift).** Studio work jumped ahead of M1/M2 and edited raw TQ `DMap`s, so the Studio was a Conquer editor rather than an engine editor. Fix, in this order: (1) game-agnostic `engine/scene` + `engine/assets`, plus a TQ importer/exporter; (2) move the Studio onto engine types; (3) finish M1 with the C3 models; (4) M2 wgpu renderer. **Rule: no new Studio features until (1)–(2) are done; editors only ever touch engine types.**
- 2026-10-18 · Engine text is UTF-8. TQ strings are GBK, converted at the importer/exporter boundary (`encoding_rs`). Export→import round-trips are *semantic*: bytes after a fixed-string NUL are not preserved. The raw format parsers stay byte-exact.

- 2026-10-18 · Engine math uses **glam** (`Mat4`, column vectors). TQ/D3D matrices load with `Mat4::from_cols_array` and no conversion (same memory layout).
- 2026-10-18 · Engine animation sampling defaults to the **classic C3 behaviour** (linear matrix blend, matching the original look). SRT/slerp sampling (the client's Enhanced mode) comes later as an option.
- 2026-10-18 · Imported mesh positions are stored **already bound** (bind matrix applied, as the client does). `bind_pose` is kept for a future exporter.

- 2026-10-18 · **The engine never encrypts or obfuscates its own files** (user decision). TQ ciphers (XorRot, the +6 shift, the LevelExp XOR) live only in `formats/c3`: decrypt on import, encrypt only when exporting back to the original 5065 client.

## Open
- ECS: `bevy_ecs` standalone vs custom.
- Scripting: Rhai (pure Rust) vs Lua (`mlua`).
- Visual target for Conquer: exact classic 5065 first, or modernized from the start.
