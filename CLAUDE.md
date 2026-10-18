# C3Engine

General-purpose Rust game engine + studio (TQ C3-engine style). Conquer is the first game.

**Read before working:** `docs/VISION.md`, `docs/ARCHITECTURE.md`, `docs/ROADMAP.md`,
`docs/DECISIONS.md`, `docs/FORMATS.md`. Keep them updated: tick roadmap items,
append decisions, document formats as you implement them.

## Rules
- **Done means done on two projects:** a feature counts only when it works on a project made from
  scratch in the Studio (no TQ content) and on the imported Conquer project, verified with
  screenshots. Never tick a roadmap item proven only on a hand-made scratch project.
- UI/UX matters: clear hierarchy, hints, shortcuts, legends, empty/loading states. Verify UI changes with screenshots.
- Clean code: focused modules, doc comments on public items, `cargo fmt` + `cargo clippy --workspace --all-targets` with zero warnings.
- Editors touch engine types only (`c3_scene`, `c3_assets`). In the Studio, **only `tq_plugin.rs` may use
  `c3_formats_tq`**. TQ ⇄ Scene conversion lives in `formats/c3/src/scene_io.rs`.
- **Every project edit goes through `c3-edit` commands** (typed, serializable, undoable). Never
  mutate a document directly from UI code; add a command instead. AI/MCP will use the same commands.
- Studio UI is Unity-inspired: `shell.rs` (menu bar, toolbar, `Panel` enum, default layout, Play,
  console), `dock.rs` (generic docking), `panels.rs` (one fn per panel), `browser.rs` (Project
  panel asset browser), `hierarchy.rs` (Hierarchy + Create menu), `selection.rs` (what is
  selected, picking, move/duplicate/remove commands), `scene_inspector.rs` (selection Inspector,
  Population import/export), `live_scene.rs` (Scene view drawing: SceneView + NPC actors),
  `play.rs` (Play mode: hosts `c3_play::screen::GameScreen` in the Game view), `stage.rs` (Preview stage), `new_map.rs` (File > New Map), `data_panel.rs` (Data
  panel: game tables, SQL import, row Inspector), `script_panel.rs` + `code_view.rs` (Script
  panel: Rhai editor with live problems and function reference), `dialogue_panel.rs` +
  `graph_canvas.rs` + `node_editor.rs` (Dialogue panel: dialogue graphs and quests), `icons.rs` (bundled
  Phosphor font; **never emoji as icons**), `theme.rs` (Unity-like dark theme + shared widgets).
- Studio layout: `studio/src/{main,app,panels,doc,editor,map_view,theme,tq_plugin}.rs`, plus
  `home.rs` (start page, project dialogs, File menu, Project inspector section), `maps.rs` (project
  vs client maps, `MapRef`), `gpu.rs` (engine
  renderer → egui texture), `model_view.rs` + `models_ws.rs` (Models workspace). The Studio always
  works on an open `c3_project::Project`; its TQ client source is opened through `tq_plugin`.
  The Scene view, Play and the Preview go through `Studio::game` (a `c3_play::GameModule`:
  Conquer's from `tq_plugin`, else `c3_play::basic::BasicGame`); never call game code directly.
- Rendering goes through `c3-render` only (never egui painter for world content). The Studio shows the
  target's `display_view` (gamma-space view) so colours match. Maps draw through
  `c3_render::scene_view::SceneView` in both the client and the Studio; never fork that path.
- Screenshots for verification: `& .\scripts\shot.ps1 -App client|studio -Out x.png -AppArgs @(<app args>)` (`PrintWindow`, works when the window is behind others).
  Studio test flags (project path first): `--map <path> --cell x,y --pick x,y --online --play --talk <npc>
  --attack --bag --quests --say "/cmd" --script <name> --table <name> --dialogue <name> --node <id>
  --show <panel> --exec <commands.json> --new-map [name] --make-map <name>`. `doc` = model + undo + save
  (unit-tested), `editor` = tools mapping pointer → edits, `map_view` = drawing + camera only. Reusable widgets go in `theme.rs`, the overlay palette in `map_view::colors`.
- Nothing in `engine/` or `studio/` references Conquer (except `studio/src/tq_plugin.rs`, the
  Studio's game plugin); game code goes in `games/`. Generic play code (world, actors,
  sessions, serving) lives in `engine/play`; a game plugs in by implementing `GameModule` and
  being picked in `games/registry`.
- Every format parser needs a round-trip test on real files from
  `D:\Conquer2\Tools\eosource\client5065` (skip gracefully if absent).
- Build: `cargo build` · Test: `cargo test` · Lint: `cargo clippy --all-targets`.
- Toolchain: stable-x86_64-pc-windows-msvc. If `cargo` isn't on PATH, add `%USERPROFILE%\.cargo\bin`.
- Online: `c3-server <project> --map map/map/newplain.DMap --cell 405,357 --listen 127.0.0.1`
  (port 7101), then `c3-client --project <project> --server 127.0.0.1:7101 --account alice
  --password pw`. `--conquer <public ip>` also serves the original 5065 client (account server
  9958, game server 5816; `games/conquer/src/protocol`). Debug builds use line-tables-only debug info (full info overflows the MSVC
  PDB); `target/` grows fast, and D: has filled up before: `cargo clean --profile dev` frees it.
