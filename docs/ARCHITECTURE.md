# Architecture

```
c3engine/
  engine/            game-agnostic engine crates (c3-*)
    core/      ECS, scheduler, events, time, logging, plugin registry
    assets/    asset database, handles, hot reload, import/export pipeline, packaging
    render/    wgpu renderer: 2D sprites/tiles, 2.5D iso, 3D skinned meshes, particles, lighting
    scene/     maps, tile/height/cell grids, scene objects, portals, cameras
    anim/      skeletal + sprite animation, blend/state machines
    net/       transport (tokio), packet codec framework, cipher trait, prediction
    ui/        c3-ui: in-game UI on egui (HUD, chat, bag + equipment, shop, NPC dialogue,
               world overlays); returns UiActions; owns the shared icon font
    audio/     sound/music playback
    script/    Rhai script host: sandboxed, hot reload, NPC dialogue API, editor diagnostics
    project/   c3-project: game projects on disk (c3project.toml manifest, folders, content
               sources, recent projects)
    game/      c3-game: the simulation shared by Play mode, client and server: one map's
               players, monsters and drops in fixed ticks (movement, spawns, monster AI,
               combat, exp/levels, bag, equipment, shops); a game's `Rules` decide numbers
    edit/      c3-edit: headless editing API. Typed serializable
               commands with undo/redo, queries and their schemas. Used by the Studio UI,
               the AI assistant and the MCP server alike
    ai/        (planned) c3-ai: Claude API client and the bridge exposing c3-edit commands
               and queries to the model as tools
  formats/
    c3/        c3-formats-tq: TQ format plugins (read + write)
  games/
    conquer/   game-conquer: 5065 protocol + crypto, rules, UI, content (Redux compatible)
  studio/      c3-studio: egui editor (map, model, anim, effect, UI, tables, scripts, assistant)
  apps/
    client/    c3-client: plays a game project (`game.rs`: GameScreen + egui-winit/egui-wgpu
               UI layer) or views a Conquer client's maps (`main.rs` viewer)
    server/    (planned) c3-server: game server built from a project (accounts, database)
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
  module (`studio/src/tq_plugin.rs`, which uses `game-conquer`'s content loader).
- Editing goes through `engine/edit`; no editor (UI, assistant, MCP) mutates projects directly.
- The server and client share `engine/game`; neither depends on the Studio.

## Planned third-party stack
| Area | Crate |
|---|---|
| GPU | `wgpu` |
| Window/input | `winit` |
| Editor UI | `egui` / `eframe` (+ `egui-wgpu`) |
| Math | `glam` |
| ECS | `bevy_ecs` standalone, or custom (DECISIONS.md) |
| Async/net | `tokio` |
| Images | `image`, `ddsfile` |
| Interchange | `gltf` |
| Serialization | `serde`, `ron` / `toml` |
| Errors / logs | `thiserror`, `anyhow`, `tracing` |
