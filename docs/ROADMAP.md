# Roadmap

Status keys: `[ ]` todo · `[~]` in progress · `[x]` done

## M0: Setup
- [x] Rust stable MSVC toolchain installed (rustc 1.99.1)
- [x] Workspace scaffold + docs
- [x] Local CI: `scripts/check.ps1` (fmt --check, clippy -D warnings, tests)

## M1: TQ formats [x] (complete 2026-10-18) (`formats/c3`)
Reference assets: `D:\Conquer2\Tools\eosource\client5065`
- [x] WDF package read + write (byte-identical on c3.wdf, data.wdf)
- [x] DNP package: n/a, no .dnp files in the 5065 client
- [x] DDS (DXT1/3/5, raw) + TGA decode: all 20,144 packed DDS decode
- [x] ClientFs VFS (loose files first, then WDF) + `mapshot` example (map ground → PNG)
- [x] MSK masks, TME terrain effects, RGN regions
- [x] ANI index files (51 files / 56,971 sections; GBK-safe; frame paths resolve in WDF)
- [x] ini tables, .scene/.Part terrain objects, GameMap.dat, all game .dat tables (incl. XorRot-encrypted)
- [x] DMap (cells, heights, portals, interactive + scene layers): 144/144 client maps byte-identical
- [x] PUL puzzle (143/143 byte-identical; every used tile resolves in its ANI)
- [x] C3 chunks: PHY, MOTI, SHAP, SMOT, PTCL, CAME
- [x] Round-trip tests on real client files for every format (`formats/c3/tests/real_files.rs`)

## Course correction (2026-10-18), see DECISIONS.md
- [x] (1) `engine/scene` + `engine/assets` + TQ importer/exporter: 144/144 maps round-trip
- [x] (2) Studio edits engine `Scene`s; TQ access isolated in `studio/src/tq_plugin.rs`
- [x] (3) C3 models: container + PHY + MOTI (incl. XKEY), engine `c3-anim` Clip/skinning and
      `c3_assets::model`, TQ importer `model_io`; all 4,516 files verified; `modelshot` renders posed models
- [x] (3b) Typed SHAP/SMOT/PTCL/CAME (PTCX/PTC3 absent in 5065)
- [x] (3c) Remaining M1 tables and formats
- [x] (4) M2 renderer: `engine/render` (wgpu 30) with sprite + lit mesh passes into offscreen targets
      (headless GPU test). Studio ground and the new Models workspace render through it

## Next
- [ ] M3 step 4: Unity-style editor shell (docking, menus, toolbar, theme, icons), then the Project panel

## M2: Renderer [x] (complete 2026-10-18)
- [x] wgpu window + camera: `apps/client` (winit 0.30 + surface), drag/WASD pan, wheel zoom, fit;
      content via `games/conquer::Content`; shared `c3_render::ground` streaming (180 fps on Twin City)
- [x] Puzzle ground + map layers (classic iso): cover sprites + terrain prefabs from ANI with the
      classic footprint draw order (`c3_scene::draw_order`), frame animation, parallax sky/water layers
      with scroll; engine `SpriteSet`/`Prefab` assets, `c3_render::{objects, parallax}` behind `SceneAssets`
- [x] Skinned C3 meshes with armor/weapon layering: actors in the 2D world (classic role transform +
      ortho camera, painter's order with map objects via depth slices); Conquer character assembly
      (`games/conquer::character`: armor/armet(hair)/weapon tables, weapon family, body + weapon
      motions with fallbacks, socket attachment `Role·Bone0`). Mounts: Mount.ini is a stub in 5065
- [x] Effects: engine `Effect` asset (layers of keyed meshes, baked particles, ribbon trails;
      per-layer texture + blend; delay/loop clock), TQ importer (`3DEffect.ini` + `3DEffectObj.ini`),
      `c3_render::effect` playback (billboards, camera-facing particles, 30 ms trail steps), blend-mode
      pipelines; map effect entities play in the client, `--effect NAME` previews any effect
- [x] Map + character viewer app: `apps/client` hero (default player or `--player`/`--actor`)
      walks where you click: `c3_scene::path` A* (8 directions, blocked cells, climb ≤ 25,
      nearest-cell fallback), one cell per walk/run action eased over frames × `Action.dat`
      interval (walk 500 ms, run 200 ms per cell), R toggles run, camera follows (Space)
- [x] Effects carried by actors, sharing the actor's depth so they wrap around it: part action
      effects from `Action3DEffect*.ini` (`Content::part_action_effects`, e.g. blade attack →
      `Flash4101` trail in the weapon's space) and role effects at the feet (`--effect LevelUp`);
      Enter attacks once, looping actions restart with their effects

## M3: Studio core, Unity/UE5-style editor (see VISION.md)
Done so far:
- [x] Studio shell (eframe/egui): toolbar, map list, inspector, status bar, theme, shortcuts
- [x] Map viewer: streamed ground tiles at client-exact placement, cell grid, blocked cells,
      portals (jump-to), objects, hover inspector, fit/zoom/pan
- [x] Map editing: Block / Walkable brush (1–21 cells), portal add/move/delete, undo/redo
      (strokes merge into one step), Save (Ctrl+S) with atomic write + first-save `.bak`,
      unsaved-changes guard on map switch and quit
- [x] Models workspace: skinned model preview through `c3-render`

Toward the Unity/UE5-style editor, in order:
- [x] (1) Project system: `engine/project` (c3-project: `c3project.toml` manifest, standard
      folders, content sources, recent list); Studio start page (New / Open / Import Conquer 5065,
      recent projects), File menu, project dialogs (`--new`, `--import <client>`), Project section
      in the inspector; the project's TQ client source feeds maps and models
- [x] (2) Command layer `engine/edit` (c3-edit): `SceneCommand` (set_blocked over cells/square/
      rect areas, add/move/remove portal by id) applied by `SceneDoc::execute` with exact undo,
      labelled history with groups (a brush stroke = one step), batches, saved marker;
      `SceneQuery` (summary, cell, portals, walkability text map); JSON in and out + JSON schemas.
      Studio map editing runs on it; History panel (click to step back/forward), labelled
      undo/redo; `--exec commands.json` runs a command file on the open map
- [x] (3) Content in the project, the Unity way: the original files are the assets, used as they
      are by the format importers (no conversion). "Import Conquer 5065" copies the client's
      content (ani, c3, data, ini, map, sound, *.wdf; 7,617 files, 1.3 GB in ~9 s) into `assets/`
      with a progress window and Cancel, or uses the client folder in place. Maps are edited and
      saved straight to their DMap (`.bak` only for content used in place). The engine's own
      scene format (`c3_scene::native`, `.scene.ron` + `.scene.bin`, all 144 maps lossless) is for
      maps made in the engine. `c3-client --project <folder>` plays a project
- [ ] (4) Unity-style editor shell: dockable panels (Hierarchy, Scene, Game, Inspector, Project,
      Console) with saved layouts, menu bar, Play/Pause/Step toolbar, crafted dark theme, a real
      icon font replacing every emoji glyph in the Studio
- [ ] (4b) Project panel (asset browser): folder tree, search, thumbnails, live preview of models,
      effects, sprites, maps
- [ ] (5) Scene view on the shared renderer: maps draw objects, parallax, effects and actors exactly
      as the client does
- [ ] (6) Scene editor: place/move/delete objects, effects, NPCs, monster spawns and portals;
      inspector for the selection; export DMap + Redux data
- [ ] (7) Play mode (offline): walk the current map inside the Studio, stop to return to editing
- [ ] (8) Model / animation / effect viewers: actions, sockets, equipment, timeline

## M4: AI integration
- [ ] MCP server (`apps/mcp`): exposes c3-edit commands and queries to external agents (Claude Code)
- [ ] Studio assistant panel (`engine/ai`, Claude API): chat that edits the project through c3-edit
      tools; shows each change, applies on confirm, one undo step per request; API key in settings
- [ ] Project context for AI: schemas, tables, map summaries and docs the model can query

## M5: Gameplay framework (shared by client and server)
- [ ] `engine/game`: entities, stats and levels, combat and skills, items and inventory, drops,
      shops, trade, teams, guilds, chat; rules from project tables + scripts
- [ ] Data table editors: items, skills, monsters, NPCs, shops, drop lists
- [ ] Scripting host (Rhai or Lua): quests, NPC dialogue, events; gameplay without recompiling
- [ ] Quest / dialogue editor
- [ ] Game UI system (`engine/ui`) + UI editor: windows, HUD, dialogs
- [ ] Audio (`engine/audio`): music regions, action and effect sounds

## M6: Networking + engine server
- [ ] `engine/net`: transport, packet framework, cipher trait
- [ ] Game server (`apps/server`) built from a project: accounts, database, maps, spawns, NPCs
- [ ] Client ↔ server play: login, spawn, walk/jump, other players, monsters, combat
- [ ] Play mode with a local server inside the Studio
- [ ] Conquer 5065 protocol + cipher in `games/conquer` (Redux-compatible client)

## M7: Prove + ship
- [ ] Conquer feature parity as a project (combat, items, UI)
- [ ] Prove generality: a small second sample game made only in the Studio
- [ ] Build from the Studio: package a playable client and server from a project
- [ ] Launcher/patcher
- [ ] glTF import/export (Blender pipeline) for models + animations
