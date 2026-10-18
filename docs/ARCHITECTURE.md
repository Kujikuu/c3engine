# Architecture

What exists today; "(planned)" marks what the roadmap still has to build (see the 2026-10-18
review in ROADMAP.md).

```
c3engine/
  engine/            game-agnostic engine crates (c3-*)
    core/      empty stub (planned: the game-module seam, settings, shared services)
    assets/    decoded asset types (image, model, sprite set, prefab, effect) and the
               `AssetSource` read trait (planned: asset database, ids, import settings, hot
               reload, packaging, plain PNG/glTF/OGG/TTF)
    render/    wgpu renderer: instanced sprites, streamed ground tiles, objects in painter's
               order, parallax, CPU-skinned meshes (one fixed light), effects, offscreen targets,
               `Camera2d`
    scene/     maps: cell grid (flags/surface/height), iso projection, ground tiles, portals
               (no destination yet), entities, parallax layers, population; native
               `.scene.ron` format; A* paths
    anim/      skeletal clips (key-frame lerp) and skinning (planned: sprite animation, blending)
    net/       c3-net: TCP server/client (tokio on a background thread; the game polls), framing
               and cipher traits, fixed-layout packet reader/writer
    ui/        c3-ui: in-game UI on egui (HUD, chat, bag + equipment, shop, NPC dialogue,
               quest log + tracker, world overlays and quest marks); returns UiActions; owns
               the shared icon font
    audio/     c3-audio (rodio): one-shot sounds with distance falloff, music, music regions
    script/    Rhai script host: sandboxed, hot reload, NPC dialogue + quest API, editor
               diagnostics; dialogue graphs (`dialogues/*.dialogue.ron`) compiled to scripts;
               the quest model (`tables/quests.table.ron`, state in player variables)
    online/    c3-online: the engine's client/server protocol over c3-net: GameServer (maps,
               logins, per-player views, saves), Connection (a client's end), Accounts
    project/   c3-project: game projects on disk (c3project.toml manifest, folders, content
               sources, recent projects)
    game/      c3-game: the simulation shared by Play mode, client and server: one map's
               players, monsters and drops in fixed ticks (movement, spawns, monster AI,
               melee combat, exp/levels, bag, equipment, shops, jump); a game's `Rules` (Rust)
               decide numbers (planned: multi-map transfers, data/script rules, skills)
    edit/      c3-edit: headless editing API. Typed serializable
               commands with undo/redo, queries and their schemas. Used by the Studio UI,
               the AI assistant and the MCP server alike
    ai/        (planned) c3-ai: Claude API client and the bridge exposing c3-edit commands
               and queries to the model as tools
  formats/
    c3/        c3-formats-tq: TQ format plugins (read + write)
  games/
    conquer/   game-conquer: rules, content, session (local or joined), `game.rs` (maps,
               simulations and new characters for every host), `serve.rs` (a server thread),
               `protocol/` (the 5065 client's front: ciphers, framing, packets, per-client
               translation of world updates into packets and requests into `Input`s). Also
               holds generic parts (world, actor, screen, session, sim view, serve) that move
               into the engine with M7 step 1
  studio/      c3-studio: egui editor: maps (cells, portals, objects, effects, sounds, NPCs,
               spawns), models (view), preview stage, tables (spreadsheet), scripts, dialogue
               graphs + quests, Play (planned: settings, effect/animation/UI editors, form
               editors, build, assistant)
  apps/
    client/    c3-client: plays a game project (`game.rs`: GameScreen + egui-winit/egui-wgpu
               UI layer) or views a Conquer client's maps (`main.rs` viewer). Needs Conquer
               content today (M7 step 1)
    server/    c3-server: serves a game project's maps to clients (engine protocol, optional
               Conquer 5065 front; accounts and characters in `<project>/server/accounts`).
               Needs Conquer content today (M7 step 1)
    mcp/       (planned) c3-mcp: MCP server exposing c3-edit to external AI agents
  docs/        this documentation (source of truth for context)
```

## Editing: one command layer
```
Studio UI ─┐
Assistant ─┼─► c3-edit commands (typed, serializable, undoable) ─► project (engine types) ─► save
MCP agents ┘        ▲ queries (read-only, schema-described)
```
Every change to a project is a command: the Studio's tools build commands, the assistant gets
them as tools, and external agents call them over MCP. One undo history records all of them.

## Data flow (maps)
```
client files ──ClientFs (AssetSource)──► scene_io::import_map ──► c3_scene::Scene ◄── Studio edits
                                         scene_io::export_dmap ◄──────────┘ (save)
```
`Scene` = Grid (flags/surface/height) + IsoProjection + ground TileLayer/Tileset + Portals +
Entities (Sprite/Prefab/Sound/Effect) + ParallaxLayers + `meta` (format values the exporter needs).

## Drawing a scene: one path for the game and the editor
```
Scene + SceneAssets (game's loader) ──► c3_render::scene_view::SceneView::prepare
    parallax ─► ground ─► objects + map effects + caller's actors (isometric painter's order)
                                     └─► mesh_draws + frame ─► Renderer (window or offscreen)
```
`SceneView` owns the streamed art caches, the map's playing effects and the `EffectLibrary`.
Actors are game code: the caller passes their cells and poses them in a callback (`Poser`).
The game world (`game_conquer::world::World`: scene view + actors + a clock the host advances)
runs in the client and in the Studio's Play mode; the Studio's Scene view (`studio/src/
live_scene.rs`) draws the same `SceneView` with NPC actors, so the editor shows exactly what the
game shows.

## Dependency rules
- `engine/*` may depend only on other `engine/*` crates and third-party crates.
- `formats/*` depends on `engine/assets` (and `engine/scene` / `engine/anim` for conversion).
- `games/*` depends on engine + formats. **Engine never depends on games.**
- `studio` depends on engine + formats; game-specific code is reached only through its plugin
  module (`studio/src/tq_plugin.rs`, which uses `game-conquer`'s content loader). **Currently
  broken** by `play.rs`, `live_scene.rs` and `stage.rs`, which use `game-conquer` directly;
  M7 step 1 restores it through the game-module seam.
- Editing goes through `engine/edit`; no editor (UI, assistant, MCP) mutates projects directly.
- The server and client share `engine/game`; neither depends on the Studio.

## Third-party stack
| Area | Crate |
|---|---|
| GPU | `wgpu` |
| Window/input | `winit` |
| Editor UI | `egui` / `eframe` (+ `egui-wgpu`) |
| Math | `glam` |
| World model | typed simulation, no ECS (DECISIONS.md) |
| Async/net | `tokio` |
| Images | `image`, `ddsfile` |
| Interchange | `gltf` |
| Serialization | `serde`, `ron` / `toml` |
| Errors / logs | `thiserror`, `anyhow`, `tracing` |
