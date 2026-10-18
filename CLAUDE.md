# C3Engine

General-purpose Rust game engine + studio (TQ C3-engine style). Conquer is the first game.

**Read before working:** `docs/VISION.md`, `docs/ARCHITECTURE.md`, `docs/ROADMAP.md`,
`docs/DECISIONS.md`, `docs/FORMATS.md`. Keep them updated: tick roadmap items,
append decisions, document formats as you implement them.

## Rules
- UI/UX matters: clear hierarchy, hints, shortcuts, legends, empty/loading states. Verify UI changes with screenshots.
- Clean code: focused modules, doc comments on public items, `cargo fmt` + `cargo clippy --workspace --all-targets` with zero warnings.
- Editors touch engine types only (`c3_scene`, `c3_assets`). In the Studio, **only `tq_plugin.rs` may use
  `c3_formats_tq`**. TQ ⇄ Scene conversion lives in `formats/c3/src/scene_io.rs`.
- **Every project edit goes through `c3-edit` commands** (typed, serializable, undoable). Never
  mutate a document directly from UI code; add a command instead. AI/MCP will use the same commands.
- Studio layout: `studio/src/{main,app,panels,doc,editor,map_view,theme,tq_plugin}.rs`, plus
  `home.rs` (start page, project dialogs, File menu, Project inspector section), `maps.rs` (project
  vs client maps, `MapRef`), `gpu.rs` (engine
  renderer → egui texture), `model_view.rs` + `models_ws.rs` (Models workspace). The Studio always
  works on an open `c3_project::Project`; its TQ client source is opened through `tq_plugin`.
- Rendering goes through `c3-render` only (never egui painter for world content). The Studio shows the
  target's `display_view` (gamma-space view) so colours match.
- Screenshots for verification: `scripts/shot.ps1 -App client|studio -Out x.png -- <app args>` (`PrintWindow`, works when the window is behind others). `doc` = model + undo + save
  (unit-tested), `editor` = tools mapping pointer → edits, `map_view` = drawing + camera only. Reusable widgets go in `theme.rs`, the overlay palette in `map_view::colors`.
- Nothing in `engine/` or `studio/` references Conquer; game code goes in `games/`.
- Every format parser needs a round-trip test on real files from
  `D:\Conquer2\Tools\eosource\client5065` (skip gracefully if absent).
- Build: `cargo build` · Test: `cargo test` · Lint: `cargo clippy --all-targets`.
- Toolchain: stable-x86_64-pc-windows-msvc. If `cargo` isn't on PATH, add `%USERPROFILE%\.cargo\bin`.
