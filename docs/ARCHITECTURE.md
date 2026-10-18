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
    ui/        data-driven in-game UI (layouts, widgets, fonts)
    audio/     sound/music playback
    script/    scripting host (Rhai or Lua; see DECISIONS.md)
    game/      (planned) gameplay framework shared by client and server: entities, stats,
               combat, items, quests, spawns; rules come from project tables + scripts
    edit/      (planned) c3-edit: headless editing API. Project model, typed serializable
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
    client/    c3-client: generic runtime launcher that loads a game project
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

## Dependency rules
- `engine/*` may depend only on other `engine/*` crates and third-party crates.
- `formats/*` depends on `engine/assets` (and `engine/scene` / `engine/anim` for conversion).
- `games/*` depends on engine + formats. **Engine never depends on games.**
- `studio` depends on engine + formats; games register editors/inspectors through plugins.
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
