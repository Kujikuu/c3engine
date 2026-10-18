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
Order set by the user (2026-10-18, after a review): the engine must make a complete game like
Conquer.exe in the Studio before more server work. M7 steps 1 to 6 in order, then M8 (prove +
ship); AI integration (was M4) stays last, now M9.
- [ ] M7 step 1: full Conquer import (client content + TQ server data, all maps)

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
- [x] (4) Unity-style editor shell: own dock system (`studio/src/dock.rs`: tab groups, resizable
      splits, drag tabs to a group or an edge, close/reopen from Window, layout saved to
      `<config>/C3Engine/layout.ron`, Reset Layout); panels Hierarchy (portals, effects, objects,
      sounds, layers; click to jump), Scene (own bar: overlays, brush, zoom), Game, Inspector,
      Undo History, Project, Console (messages/errors with filters); menu bar (File/Edit/Window/
      Help), toolbar (tools, Play/Pause/Step, Layout); Play launches the game client on the open
      map where the Scene view looks (Stop ends it); Unity-like dark theme; bundled Phosphor icon
      font replaces every emoji. (egui_dock/egui_tiles/egui-phosphor don't support egui 0.37 yet)
- [x] (4b) Project panel (`studio/src/browser.rs`): folder tree + grid/list (size slider), breadcrumb,
      search, lazy thumbnails for textures (DDS/TGA/PNG/JPG), type icons, packed WDF files shown in
      their folders with a badge (names from ANI frames + ini tables: 23,655 of 25,013 named, 0.6 s),
      double-click opens maps/models, context menu (Open, Show in Explorer, Copy Path), Inspector
      shows the selected asset (size, storage, texture preview, C3 contents); `--browse`, `--select`
- [x] (5) Scene view on the shared renderer: `c3_render::scene_view::SceneView` draws a scene the
      way the client does (parallax sky/water, ground, objects, map effects, and the caller's actors
      in one painter's order); the client's `World` and the Studio's Scene view both draw through it.
      `EffectLibrary` moved into the engine (`c3_render::effect_library`), `SceneAssets` gained
      `effect`. Scene bar: Layers menu (sky, ground, objects O, effects E, anchor markers M) and an
      Animate toggle; effect load errors go to the Console; `--cell x,y` frames a cell
- [x] (6) Scene editor: engine `Population` (NPCs, monster spawn areas) in scenes and, for DMaps,
      `maps/<name>.population.ron`; c3-edit add/move/set/remove commands for entities, NPCs and
      spawns + objects/npcs/spawns queries. Studio: Select tool (click, drag to move, Del, Esc,
      F frames, Ctrl+D duplicates), NPC models and spawn areas in the Scene view, Hierarchy with
      selectable rows and a Create menu (portal, NPC, spawn area, effect picker, object from the
      Project selection), Inspector editing (one drag/text visit = one undo step). Population
      import from TQ server SQL dumps (`cq_npc`/`cq_generator`, Twin City: 49 NPCs, 61 spawns)
      and SQL export for Redux (`game_conquer::server_data`). NPC looks: `Npc.ini` →
      `3DSimpleObj.ini` → model + stand-by motion (`game_conquer::npc`); actors moved to
      `game_conquer::actor`, shared by the client (which now shows project NPCs) and the Studio
- [x] (7) Play mode in the Game view: Play (Ctrl+P) builds a `game_conquer::world::World` from the
      open map as it is (unsaved edits, NPCs) with the hero where the Scene view looks; click to
      walk, R run, Enter attack, Q/E turn, Space follow, wheel zoom; Pause (Ctrl+Shift+P) and Step
      drive the world's own clock (`World::advance`); Stop drops it so playing never changes the
      map. "Play in the game window" still runs the saved project in `c3-client`. `World` and the
      engine `c3_render::camera::Camera2d` are shared by the client and the Studio
- [x] (8) Preview panel (`studio/src/stage.rs`): a stage drawn exactly like the game (shared
      `World`) for characters from look + equipment ids, any action (named list from BaseFunc.h,
      once or looping, restart), the 8 facings, and effects on the character or the ground
      (searchable list, loop, clear); pause, step, speed. The Models workspace keeps the
      single-model viewer with parts, sockets and frame scrubbing. `--show <panel>` option

M3 complete (2026-10-18).

## M5: Gameplay framework (shared by client and server) [x] (complete 2026-10-18)
In order; each step is playable in the Studio's Play mode:
- [x] (1) Game data: `engine/data` typed tables (schema + rows, `tables/*.table.ron`), table
      commands with undo in c3-edit, SQL dump import; Studio Data panel (spreadsheet: search,
      inline edit, row Inspector); Conquer tables imported from the TQ dump (items, monsters,
      magic, levels, shop goods)
- [x] (2) Scripting: `engine/script` Rhai host, `scripts/*.rhai`; NPC dialogue API (text,
      options, give/take, teleport); Studio script editor with errors; NPC scripts run in Play mode
- [x] (3) Simulation: `engine/game` world (players, monsters, NPCs, ground items, cell index),
      tick loop, spawns from spawn areas, monster AI, movement rules, combat + death + respawn,
      exp/levels, drops, inventory/equipment, money, shops; game rules trait implemented by
      `games/conquer` (TQ formulas); Play mode becomes a view of the simulation
- [x] (4) Game UI: `engine/ui` HUD (HP/MP/exp), inventory, NPC dialogue, shop, chat windows in
      the client and the Game view
- [x] (5) Audio: `engine/audio` music regions, action and effect sounds
- [x] (6) Quest and dialogue editor: Studio Dialogue panel (node graph: Say/Branch nodes,
      conditions, actions, drag-to-link, undo, problems), dialogue graphs compiled to
      `scripts/dialogue/*.rhai`; quest table (kill/collect/talk goals, rewards) with script API,
      kill counting in the simulation, `/quest` command; quest log, tracker and NPC quest marks

## M6: Networking + engine server [x] (complete 2026-10-18)
- [x] `engine/net`: transport (tokio on a background thread, poll/send API), framing trait
      (length-prefixed), cipher trait with re-keying hook, fixed-layout packet reader/writer
- [x] Game server (`apps/server`) built from a project: maps (scenes + population), spawns,
      NPC scripts, quests, rules; accounts with salted password hashes and saved characters
      (`<project>/server/accounts/*.ron`, autosave + on logout); console `quit`
- [x] Client ↔ server play (`engine/online`): login, spawn, walk, other players (names over
      heads), monsters, combat, dialogue, shops, chat and commands; the server sends each
      player its view (`Sim::update_for`) and the client mirrors it (`Sim::apply_update`), so
      the same UI and views run online; `c3-client --server host:port --account <name>`
      (jumping included: `Input::Jump`, 16 cells)
- [x] Play mode with a local server inside the Studio (toolbar toggle; `--online`)
- [x] Conquer 5065 protocol + cipher in `games/conquer::protocol`: the original client logs in
      (account server 9958: TQ cipher, RC5 password, token) and plays on the engine's server
      (game server 5816: DH key exchange, Blowfish-CFB64): hero, items, map entry, NPCs,
      players, monsters, ground items, walk/jump, combat, chat, NPC dialogs; one more `Front`
      of the same `GameServer` (`c3-server --conquer <public ip>`). Spec:
      `docs/conquer5065_protocol.md`. Checked with the real Conquer.exe (2026-10-18): login,
      map entry, NPCs, monsters, walking, combat, level up, death, chat, quest dialogue. Still
      to check there: jump, teleport, items and equipment, shop buy/sell, other players.
      Skills over this protocol come with M7 step 5

## M7: Complete engine: Conquer first, then any game of its kind
Review (2026-10-18): M3, M5 and M6 were proven on a scratch project with four hand-made NPCs. A
real imported Conquer project had empty maps, tables and scripts: the import brings only the
client's files, and the world (NPCs, spawns, tables, NPC logic) lives in TQ's server data. This
milestone makes Conquer itself a complete, editable project, then makes sure any game of the
kind can be built from scratch (user, 2026-10-18: "a complete game engine that can truly
create Conquer Online or Eudemons, with everything, like Unity, easier for indies"; not an
import of the existing Eudemons client, but the possibility to create any). In order:
- [ ] (1) Full import in one step: "Import Conquer 5065" takes the client folder and TQ's server
      data (an SQL dump such as `tq_re/db/cq_finalized_mariadb.sql`) and fills the project:
      every map's NPCs (`cq_npc`, `cq_dynanpc`), spawn areas (`cq_generator`), portals and their
      destinations (`cq_passway`, `cq_portal`), the map list and properties (`cq_map`), and the
      game tables (monsters, items, magic, shops, drops, levels, stat points). Any of the 144
      maps opens with its population and plays; an import report says what came in and what
      didn't. The per-map SQL buttons stay for later merges
- [ ] (2) Complete map and world editor (user, 2026-10-18). Today the Scene view edits blocked
      cells, portals, objects, effects, sounds, NPCs and spawn areas, added from menus; there is
      no drag and drop, no ground painting and no new maps. To do, every edit one undo step
      through c3-edit and saved to the map's own files (DMap, PUL, scene parts):
  - [ ] Drag and drop from the Project panel into the Scene view: objects (scene parts, ANI
        sprites), effects, sounds, models, NPC looks (makes an NPC), monsters (makes a spawn
        area), ground tiles; a ghost preview snapped to the cell under the cursor; drop on the
        Hierarchy too
  - [ ] Placement tools: box and multi-select, move with cell snapping, pixel nudge of sprite
        offsets, footprint editing, draw order, hide and lock, copy / paste / duplicate (also
        between maps), align
  - [ ] Ground: paint the puzzle ground with tiles from tilesets (brush, fill, erase, pick);
        tilesets from the project's assets
  - [ ] Cells: height and surface brushes beside the walkability brush; overlays for each
  - [ ] Layers: parallax backgrounds (sky, water) and their items; interactive layer order
  - [ ] Regions: music and sound regions, map regions (safe zones, PK-free, booth), weather,
        light; drawn and edited in the Scene view
  - [ ] Portals: destination map and cell picker, two-way links, jump to the destination
  - [ ] Maps: new map wizard (size, default ground), resize, properties (name, id, flags,
        music, weather), minimap generation; the map list (`GameMap.dat`) with create, clone,
        rename and delete
  - [ ] World view: every map as a node with its portal links; open a map from it; players
        travel between maps through portals in Play mode and on the server
- [ ] (3) Content editors with previews (forms over the tables, every edit an undoable c3-edit
      command): NPCs (one list across all maps, jump to the map), monsters (look preview,
      stats, drops, where they spawn), items (icon and model preview, stats, which shops sell
      them), skills, shops, drops, levels
- [ ] (4) NPC behaviour: convert TQ's NPC logic (`cq_action` / `cq_task` chains) into dialogue
      graphs (`dialogues/*.dialogue.ron`), editable in the Dialogue editor; actions the graphs
      can't express yet become new node and action kinds; the converter reports anything left
      over (user choice 2026-10-18: graphs, the easiest to edit later)
- [ ] (5) Gameplay parity in the simulation, then over the 5065 front: skills and magic
      (`cq_magictype`), XP skills, PK and name colours, teams, trade, warehouse, guilds,
      mounts, item upgrades, ghost and revive, monsters as TQ (passive unless their type says
      aggressive; today every monster attacks)
- [ ] (6) Conquer's look and a UI editor: the engine's client draws Conquer's GUI (the 5065 GUI
      ini and images) as a c3-ui skin; a Studio UI editor for windows, HUD and dialogs (layout,
      images, data bindings)
- [ ] (7) Project Settings, Unity-style (Edit > Project Settings; saved in the project, applied
      live in Play mode, read by the built client and server):
  - [ ] Player: game name, icon, window and full screen, resolutions, login and loading screens
  - [ ] Cursors: normal, attack, talk, pick up, portal, busy and more, from the project's
        assets (ANI / cursor files), with hotspots; per-state preview
  - [ ] Camera: projection (cell size), zoom levels and limits, follow smoothing and dead
        zone, map edge clamping, shake; a camera preview in the Game view
  - [ ] Input: key and mouse bindings (walk, run, attack, skills F1..F10, UI windows), players
        rebind them in game
  - [ ] Graphics and audio: frame cap, vsync, effect quality; mixer volumes (master, music,
        effects, UI)
  - [ ] Gameplay and network: start map and cell, new character templates (classes, looks,
        starting items), tick rate, view range, ports and protocol fronts
  - [ ] Localization: string tables (the client's StrRes.ini and the game's own), fonts
        (GBK and Unicode)
- [ ] (8) Any game of the kind, from scratch: gameplay systems a game switches on and shapes
      with data instead of Conquer-specific code: classes and stats, skills, pets and companions
      (summon, fight, level, evolve, equip: an Eudemons-like game), mounts, crafting, guilds,
      events; the game's own formulas as scripts; new content (characters, monsters, items,
      maps) made in the Studio from the project's own art, not only from TQ files
- [ ] (9) Easy for indies: New Project templates (Conquer-like, Eudemons-like, empty), a sample
      game, in-Studio getting-started guide and tooltips, clear errors that say how to fix them

## M8: Prove + ship (was M7)
- [ ] Prove generality: a small second sample game made only in the Studio
- [ ] Build from the Studio: package a playable client and server from a project
- [ ] Launcher/patcher
- [ ] glTF import/export (Blender pipeline) for models + animations

## M9: AI integration (was M4, then M8; the user keeps it last)
- [ ] MCP server (`apps/mcp`): exposes c3-edit commands and queries to external agents (Claude Code)
- [ ] Studio assistant panel (`engine/ai`, Claude API): chat that edits the project through c3-edit
      tools; shows each change, applies on confirm, one undo step per request; API key in settings
- [ ] Project context for AI: schemas, tables, map summaries and docs the model can query
