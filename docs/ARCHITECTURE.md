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
  formats/
    c3/        c3-formats-tq: TQ format plugins (read + write)
  games/
    conquer/   game-conquer: 5065 protocol + crypto, rules, UI, content → Redux server
  studio/      c3-studio: egui editor (map, model, anim, effect, UI, tables, scripts)
  apps/
    client/    c3-client: generic runtime launcher that loads a game project
  docs/        this documentation (source of truth for context)
```

## Dependency rules
- `engine/*` may depend only on other `engine/*` crates and third-party crates.
- `formats/*` depends on `engine/assets` (and `engine/scene` / `engine/anim` for conversion).
- `games/*` depends on engine + formats. **Engine never depends on games.**
- `studio` depends on engine + formats; games register editors/inspectors through plugins.

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
