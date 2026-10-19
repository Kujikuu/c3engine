# Roadmap

Status keys: `[ ]` todo · `[~]` in progress · `[x]` done

## M0: Setup
- [x] Rust stable MSVC toolchain installed (rustc 1.99.1)
- [x] Workspace scaffold + docs
- [x] Local CI: `scripts/check.ps1` (fmt --check, clippy -D warnings, tests)

## M1: TQ formats [x] (complete 2026-10-18) (`formats/c3`)
Reference assets: `D:\Conquer2\Tools\eosource\client5065`
- [x] WDF package read + write (byte-identical on c3.wdf, data.wdf)
- [x] DNP package: not implemented (no .dnp files in the 5065 client)
- [x] DDS (DXT1/3/5, raw) + TGA decode: all 20,144 packed DDS decode
- [x] ClientFs VFS (loose files first, then WDF) + `mapshot` example (map ground → PNG)
- [x] MSK masks, TME terrain effects, RGN regions
- [x] ANI index files (51 files / 56,971 sections; GBK-safe; frame paths resolve in WDF)
- [x] ini tables, .scene/.Part terrain objects, GameMap.dat, game .dat tables: 4 binary (MagicType,
      Action, AutoAllot, Tips) and 4 XorRot-encrypted text tables. Not yet: the GUI dialog/control
      ini tables, cursors (.ani/.cur), fonts, minimap/radar art; writers for ini and DDS
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
Order set by the user (2026-10-18): a complete engine that can truly create Conquer Online or
an Eudemons-like game (or any of the kind) from the Studio, with everything, Unity-style and
easy for indies. The foundation (M7) comes first, because every later step needs it.
- [x] M7 step 1: the game-module seam (done 2026-10-18)
- [x] M7 step 2: Project Settings (done 2026-10-18)
- [x] M7 step 3: Input System (done 2026-10-18)
- [x] M7 step 4: Native asset pipeline (done 2026-10-18)
- [x] M7 step 5: Multi-map worlds (done 2026-10-18)

Why this order: see "Review (2026-10-18)" below.

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
      map where the Scene view looks (Stop ends it; now Play runs inside the Game view, see (7)); Unity-like dark theme; bundled Phosphor icon
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

M3 complete (2026-10-18), on a scratch project; see the review below for what it lacks.

## M5: Gameplay framework (shared by client and server) [x] (complete 2026-10-18; thin, see the review below)
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

## M6: Networking + engine server [x] (complete 2026-10-18; one map per player, see the review below)
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
      Skills over this protocol come with M7 step 6

## Review (2026-10-18): where the engine really stands
A code review against VISION.md found that M3, M5 and M6 were proven on a scratch project with
four hand-made NPCs, and that the result is a Conquer viewer and editor with a game simulation,
not yet an engine anyone can make a game with:
- **Hard-wired to Conquer.** `apps/client`, `apps/server` and the Studio's Play, Scene and
  Preview depend on `game-conquer` (refusing projects without Conquer content); there is no
  game-module seam. `engine/project` knows one content source, a TQ client.
- **Nothing can be made from scratch.** No new map anywhere; images are DDS/TGA only, no
  PNG/glTF/OGG; the engine has native formats only for scenes, tables, scripts and dialogues;
  no asset database, handles, import settings or packaging; `engine/core` is an empty stub.
- **No settings.** `c3project.toml` holds a name, a version and sources. Every gameplay, camera,
  input, UI and audio value is a Rust constant (view range 18, revive 10 s, bag 40, tick 50 ms,
  zoom 0.05..4, keys, HUD sizes, "silver"...).
- **One map per world.** A `Sim` holds one map; portals have no destination and no code uses
  them; cross-map teleports are refused; the server never moves a player between maps.
  (Closed by step 5.)
- **Thin gameplay.** Entity kinds are fixed (player, monster, ground item); melee auto-attack
  only; no skills, status effects, PvP, pets, teams, trade, guilds, mounts; one money field;
  items have no stacks or durability; scripts run only when talking to an NPC; game rules are
  Rust code only (`ConquerRules` reads hard-coded `cq_*` table names).
- **Hard-coded game UI.** `c3-ui` is fixed egui code with constant sizes and 14 colours; no
  layouts, skins or editor. The client has no login screen, character creation or settings.
- **Studio gaps.** No drag and drop, multi-select, copy/paste, ground painting, cell
  height/surface editing, regions, layers, map properties or map list; sounds can't be created;
  tables can't be created or given columns (SQL import only); no effect, animation, model, UI or
  settings editors; no build. Undo gaps: dialogues use their own snapshot stack, scripts none,
  Data row drags make one step per frame. File > Save saves only the map.
- **Server.** No database (account files), no admin console beyond `quit`, GM commands open to
  every player.

Kept as they are: M1 formats and round-trips (exporters exist only for the classic formats; DNP
is not implemented), the M2 renderer, the scene/command layer, the simulation core and the
engine protocol. The milestones below build the engine on them first, then Conquer as a
complete project on that engine, then any game of the kind.

## M7: Engine foundation (make it an engine, not a Conquer tool) [x] (complete 2026-10-18)
- [x] (1) Game-module seam: a `GameModule` interface the client, the server and the Studio
      (Scene, Play, Preview) go through: content and asset loading, rules, character looks,
      sound mapping, UI hooks. The generic parts stuck in `games/conquer` (world, actor, screen,
      session, sim view, serve, event sounds) move into the engine; the Studio stops using
      `game-conquer` outside its plugin. A built-in data-driven module runs projects with no TQ
      content, so the apps open any project
  - Done 2026-10-18: `engine/play` (c3-play) holds the seam and the runtime;
    `ConquerModule` implements it; `games/registry` (c3-games) picks the game per project.
    `BasicGame` plays projects without TQ content with placeholder figures and a built-in
    ground (its rules are still Rust; data-driven rules are step 6). File > New Map makes a
    blank walkable map. Verified with screenshots on a from-scratch project (New Map, Scene
    with NPCs, Play local and through a local server, Preview, `c3-server` + `c3-client`) and
    on the Conquer project (Scene with NPC models, Play, Preview unchanged)
- [x] (2) Project Settings, Unity-style: a settings file in the project and an Edit > Project
      Settings window, applied live in Play mode and read by the built client and server.
      Every hard-coded value found in the review moves there:
  - [x] Player: game name, icon, window size, full screen, resizable (login and loading screens
        come with M9's client front end)
  - [x] Cursors: normal, attack, talk, pick up, portal from project assets (PNG or Windows
        `.cur`/`.ani`, animated), hotspots; drawn by the game in the client and the Game view
        (a busy cursor comes with M9's loading screens)
  - [x] Camera: projection (new-map cell size, tilt), start zoom and limits, wheel speed, pan
        speed, free pan, follow smoothing and dead zone, map edge clamping, shake on hit
  - [x] Input: the project's actions and their bindings per device (step 3)
  - [x] Gameplay: tick, view range, ranges (talk, trade, jump, attack search, monster leash
        and wander), timings (corpse, revive, item life), bag size, the currency's name, start
        map and cell, new character template (level, money, bag, worn), GM permissions
        (several currencies come with step 6)
  - [x] Graphics and audio: frame cap, vsync, background colour; master, music and sound
        volumes, hearing range (effect quality and lighting: step 8)
  - [x] Network: port, protocol fronts (the 5065 front is a setting of Conquer projects), save
        interval, join timeout
  - [x] Localization: game fonts in order with fallbacks for other scripts (string tables come
        with M9's data-driven game UI, where the UI's text becomes data)
  - Done 2026-10-18: `c3_core::settings` (types, defaults, rules), `settings.toml` in the
    project (`c3-project`), `c3_edit::SettingsDoc` (undoable `set`/`reset` by path, for the
    Studio, AI and MCP alike), the game's defaults (`GameModule::default_settings`: Conquer's
    cursors, Silver, newbie gear, 5065 front), the Studio's Edit > Project Settings window
    (eight sections, reset to default, cursor previews, saved as you go, Ctrl+Z). Verified on
    a from-scratch project (bag 12, Coins, zoom 2, window title and size in the client) and
    the Conquer project (its cursors, Silver and gear; the server serves the 5065 front from
    the settings)
- [x] (3) Input System, like Unity's: games read actions (act, move, interact, attack nearest,
      targets, walk/run, turn, camera pan and zoom, bag, quests, chat, menu...), never raw
      keys; the project defines each action's bindings, players rebind them in game, saved per
      player (`controls.toml` in the player's config folder)
  - [x] Devices: keyboard and mouse; gamepads (Xbox, PlayStation, Switch Pro and generic, via
        `gilrs`) with sticks, triggers and a dead zone; touch (tap, joystick, buttons, pinch);
        hot-plug; the device used last switches the hint line and the Controls page
  - [x] Control schemes for a click-to-walk game: mouse (click to walk, attack, talk); gamepad
        (stick or D-pad steps the hero cell by cell, interact and attack the nearest, cycle
        targets with the bumpers); touch (tap to walk and act, virtual joystick, on-screen
        buttons, pinch zoom)
  - [x] Game UI navigable with a gamepad (D-pad and stick move the focus, A confirms, B goes
        back) and usable by touch; the Studio's Play mode simulates touch (`--touch`, the hand
        button) and the game menu (Menu action) has Controls and Leave
  - [x] Studio: Project Settings > Input (each action's bindings per device, added from the
        controls that fit or by pressing a key, removed, reset; dead zone, repeat, when touch
        controls show), live in Play mode
  - Later: rumble, long press and pen, a skill wheel or shoulder pages (skills come with step
    6), modifiers (Shift+key), UI scale and safe areas (with M9's game UI)
  - Done 2026-10-18: `c3_core::input` (actions, bindings as text like `Keys W A S D`),
    `c3_play::input` (gilrs pads, the mapper from devices to actions), `controls` (player
    rebinds), `touch` (the on-screen overlay), `GameScreen` reading actions only. Verified on
    a from-scratch project and the Conquer project: the Input page, Play with touch controls,
    the Controls page
- [x] (4) Native asset pipeline, the Unity way: plain PNG/JPG sprites, sprite sheets and
      tilesets, glTF models and animations, WAV/OGG/MP3, TTF fonts, used in place with per-asset
      import settings (a sidecar file, like Unity's `.meta`); an asset database with stable ids
      and references; hot reload for every asset; a native effect format; the Project panel
      imports, renames, moves and deletes with references kept; TQ formats stay importers
  - Done 2026-10-18 in seven commits (4a-4g): `c3_core::asset` (sidecars: id + importer:
    image filter, pivot, grid, animations; sound volume, loop, stream; model scale; effect,
    font, data), `c3_project::AssetDb` (sidecars written beside the project's files, never in a
    TQ client; rename/move/delete to `.c3/trash`/import; references in maps, settings, tables
    and scripts follow a rename), `c3_edit::AssetDoc` (undoable `AssetCommand`s),
    `c3_play::native` (images and sheet frames `path#i`, sprite sets, tilesets, glTF models,
    sounds, fonts) layered over every game by `ProjectGame`, `c3-formats-gltf` (Y-up metres to
    −Z-up pixels, two-joint skinning, rigid parts on animated nodes, 30 fps clips, textures
    `model#texture<i>`), OGG Vorbis, `.c3fx` effects (flipbooks + glTF layers), sharp-pixel
    sampling. Studio: Inspector Import section (frame grid over the preview, animations,
    sound Listen), glTF in the Model view (animation picker), effects in the Preview, hot
    reload of every view, Project panel + menu, drop-to-import, drag-to-move, rename, delete.
    Verified on the scratch and Conquer projects (native sprites beside TQ art in the Scene
    and Play, a skinned and a rigid glTF, a two-layer effect, a font, a live recolour in Play,
    a folder rename that the map followed)
  - Later: the Undo History panel lists asset steps (step 7); placing assets by dragging
    them into the Scene (M8); roles drawn from sheets or glTF models (step 6 / M9); map music
    for native projects (done in step 5: Project Settings > Maps)
- [x] (5) Multi-map worlds: the map list as an engine registry (ids, names, properties);
      portals with a destination map and cell; players move between maps through portals and
      teleports, in Play mode and on the server (one `Sim` per map, transfers between them)
  - Done 2026-10-18 in five commits (5a-5e): `Portal.to` (a map id and cell) and
    `Population.links` (Conquer's `cq_passway` + `cq_portal`, set on import), Project
    Settings > Maps (`MapEntry`: key, id, name, music, PvP) and `c3_play::map_list::MapList`
    (listed maps first, the game's own ids, then free ids); `c3_game::portals` (walk into a
    portal or `Input::EnterPortal`; same map: placed; another: `Event::Travel`); scripts'
    teleports travel too. Play and the client follow travel (`Session::follow_map_change`:
    the hero carried over, the old map's textures freed, map music). The server
    (`GameServer`) moves players between `MapWorld`s and loads maps when first needed
    (`MapLoader`, `serve::project_maps`); players keep a server-wide id; fronts get `moved`
    (the engine client re-welcomes, Conquer's flies with action 86 and enters portals with
    85). Studio: the portal Inspector (destination, arrival, open it), destinations in the
    Hierarchy, new maps listed at once. Verified on the scratch project (Meadow ↔ Grove,
    local and online) and the Conquer project (Twin City → forum, local and online)
  - Later: two-way links and a World view (M8); weather, light and safe zones per map (M8
    regions); the original Conquer client walking through a portal is covered by a protocol
    test, not yet by a session with the real client
- [x] (6) Data-driven gameplay core: attributes and stats defined in data; formulas as scripts
      (rules from tables + scripts, so a game needs no Rust); items with stacks, durability and
      quality; equipment slots defined in data; currencies; skills and magic (targeting, area,
      cooldowns, costs, effects); status effects and buffs; PvP and PK; monster behaviour from
      data (passive, aggressive, ranged, boss); script events beyond talking (kill, use item,
      enter region, timers, login)
  - Done: `c3_play::data_rules` reads the tables levels, monsters, items, shops, skills and
    statuses (Data panel > New makes each with an example row) and `scripts/rules.rhai`
    formulas (hit, experience, levels, player stats, skill damage and heals); items stack,
    wear and have qualities; equipment slots and currencies are settings; skills are cast
    from a HUD bar (keys 1–8) at the target, the pointer or oneself, cost mana, rest, reach
    an area and show effects; statuses change stats, tick life or stun, shown on the HUD;
    PvP on maps marked `pk` (or `/pk`), killers marked red; monsters assist, flee, cast and
    bosses hold on; `scripts/events.rhai` hears login, map entry, kills, item use and timers.
    Conquer reads cq_magictype (skills, statuses), MagicType.dat (skill effects and sounds),
    monster magic, and draws the client's portal effect. Verified on the scratch project and
    the Conquer project, local and online
  - Later: regions (M8) bring `on_region` events; skill levels; professions
- [x] (7) Undo everywhere: dialogues, scripts, table rows, settings and assets through c3-edit
      commands; File > Save saves everything dirty
  - Done: c3-edit gains `DialogueDoc` (add / set / remove nodes, start node; canvas edits
    become those commands, one step each, named after them) and `ScriptDoc` (set text; a
    run of typing is one step, an inserted call its own); maps, tables, settings and assets
    already were. Undo and Redo act on the focused document, and the Undo History panel
    lists that document's steps (map, table, script, dialogue, settings or assets). File >
    Save writes every dirty document; settings and assets save as they change
- [x] (8) Renderer gaps the settings and UI need: world text drawn by the engine with project
      fonts (names, damage numbers, chat bubbles), custom cursors drawn in the game, a minimap /
      radar, configurable lighting (today one fixed light)
  - Done 2026-10-18: names, damage numbers and chat bubbles (8 s over the speaker, kept below
    the hint line) draw in the project's fonts; the game's cursors were done earlier. A radar
    in the top-right corner shows walkable ground turned like the map, monsters, NPCs, other
    players and portals. Graphics settings gain the scene light (a colour over the map,
    sprites and solid meshes; effects keep their glow) and sun shading for models. Verified
    with screenshots on the from-scratch project and the Conquer project

## M8: Complete map and world editor [x] (complete 2026-10-18)
Every edit one undo step through c3-edit; maps saved to their own files (native scene, or DMap /
PUL for the classic client).
- [x] New map from scratch (size, default ground) with the project's own art; resize; map
      properties (name, id, flags, music, weather, light); minimap generation; the map list
      (create, clone, rename, delete)
  - Done 2026-10-18: New Map covers the ground with a project image; Resize is an undoable
    scene command (a one-image ground grows with the map); Project Settings > Maps gains a
    per-map light (name, id, music, PvP were there; weather comes with regions); Generate
    Minimap renders the map in pieces into `maps/<name>.minimap.png`, the radar shows it (a
    Conquer map without one shows the client's own `ani/MiniMap.Ani` picture); the Project
    panel duplicates, renames and deletes maps with undo, the map list entry following
- [x] Drag and drop from the Project panel into the Scene view and the Hierarchy: sprites,
      objects, effects, sounds, models, NPC looks (makes an NPC), monsters (makes a spawn area),
      tiles; a ghost preview snapped to the cell under the cursor
  - Done 2026-10-18: images, sprite sets, terrain objects, sounds and effects drop from the
    Project panel (client pictures stand on their cell; DMaps refuse plain pictures, which the
    classic client can't show); a monster type dragged from the Data panel makes a spawn area;
    Create > NPC picks from the game's NPC looks (the client's Npc.ini). Tiles come with
    ground painting below; 3D models as NPC looks come with M9's character forms
- [x] Placement tools: box and multi-select, move with snapping, pixel nudge, footprints, draw
      order, hide and lock, copy / paste / duplicate (also between maps), align
  - Done 2026-10-18: box select (Shift adds), Shift-click toggles; dragging moves the group
    cell by cell, arrows step it on screen, Alt+arrows nudge cover objects a pixel; selected
    cover objects show their footprint; bring to front / send to back, line up in a row or
    column; H hides and L locks (Shift: all back), marked in the Hierarchy; Ctrl+C / Ctrl+V
    paste under the pointer on any map, Ctrl+D duplicates; each change one undo step
- [x] Ground: paint tiles from tilesets (brush, fill, erase, pick)
  - Done 2026-10-18: the Ground tool (6) paints with a brush of tiles, fills joined
    look-alike tiles, erases and picks; its palette is the map's tileset (plus the project's
    images on engine maps, added to the tileset as needed). A client map paints with its own
    puzzle tiles and saving writes its PUL back (the original kept as .bak)
- [x] Cells: height and surface brushes beside the walkability brush; overlays
  - Done 2026-10-18: Surface (7) and Height (8) brushes paint the value set on the toolbar
    (Alt+click takes a cell's), one undo step per stroke; overlays colour surface types and
    shade heights (values written in the cells when zoomed in), shown while the tool is on or
    from Layers
- [x] Layers: parallax backgrounds (sky, water) and their items; layer order
  - Done 2026-10-18: the Hierarchy lists the map's parallax layers and Create > Layer adds
    one; the Inspector edits a layer's order and scroll rate and its items (a tiled ground
    file, or a sprite on a cell with its offset) added from the Project selection; undoable
    commands. Layer sprites (classic `C2DMapSceneObj`) now draw and animate. Dropped
    pictures stand on their cell by their pivot
- [x] Regions: music, sound, safe zone, PK-free, booth, weather, light, script triggers
  - Done 2026-10-18: regions live with the map's population (beside a DMap too); the Region
    tool (9) drags one out, the Hierarchy lists them and the Inspector sets name, cells,
    safe zone (no one fights), no PK, market (booths), music, looped sound, weather (rain,
    snow, leaves) and light. In play: fights stop in safe and no-PK zones, region music and
    ambience play, light and weather follow the hero, and events.rhai hears
    on_enter_region(id) / on_leave_region(id). Booths themselves come with M12's trade
- [x] Portals: destination picker, two-way links, jump to the destination
  - Done 2026-10-18: the Inspector picks the destination map from the map list and the
    arrival cell, opens the destination at the arrival, and Link both ways makes (or reuses)
    a portal by the arrival on the destination map leading back; travellers arrive beside
    portals, never on them. Map keys compare without case (DMap paths)
- [x] World view: every map as a node with its portal links; open a map from it
  - Done 2026-10-18: the World tab shows every listed map as a card and portal links as
    arrows (both ways drawn once), read from project scenes and, for client maps, their
    population files (TQ's cq_passway/cq_portal links); the open map shows as edited. The
    most linked map sits in the middle with its neighbours around it; cards drag (kept in
    the project's world.ron), Fit frames the linked maps, double-click opens a map

## M9: Content editors and the game UI [x] (complete 2026-10-19)
- [x] Table and schema editor: create tables, add, rename and type columns; references between
      tables (an item's monster drop, a shop's goods) as pickers
  - Done 2026-10-18: right-click a column's name to rename, describe, change its type (values
    converted), make it refer to another table, insert or delete it; + Column adds one; all
    undoable table commands. A column referring to a table shows each row's name
    ("1000010 · Resolutive") and the row Inspector picks from that table (searchable) and
    jumps to the row. New tables came earlier
- [x] Forms with previews over the tables: characters and classes, monsters (look, stats, drops,
      where they spawn), items (icon and model, stats, which shops sell them), skills, shops,
      drops, levels, NPCs (one list across all maps, jump to the map)
  - Done 2026-10-18: a row of the game's monsters, items, skills or levels table shows, above
    its fields, what the game reads from it: a monster's stats, look (previewed in 3D),
    drops and its spawn areas on every map; an item's icon, stats, "Try it on" on the hero,
    the NPCs selling it and the monsters dropping it; a skill's numbers and its effect
    (previewed); a level's stats. Links jump to the row or open the map with the thing
    selected. The World tab lists every map's NPCs (search, click to open). Each game names
    its tables (Conquer: cq_monstertype, cq_itemtype, cq_magictype by type, cq_levexp).
    Classes have no data in the engine yet: their form comes with professions (M12)
- [x] Effect editor (layers, particles, trails, timing, preview) and animation set editor
      (actions, frames, sockets, sounds per action)
  - Done 2026-10-18: a project `.c3fx` opens beside its preview: timing and layers
    (flipbooks with bursts, models, trails), each change one undo step written at once. An
    animation set (`.c3char`) dresses roles of a look (players, monsters or NPCs) in a glTF
    model: each action's animation, frame time and sound, and models held at its parts (a
    sword in the hand); it opens on the Preview stage with the same editor column. Native
    parts draw in their material colour
- [x] Data-driven game UI: windows, HUD and dialogs as layout files (images, text, bars, grids,
      buttons, data bindings), skins from images; a Studio UI editor with live preview
  - Done 2026-10-18, in TQ's model as decided by the user: a GUI.ini-form layout
    ([dialog-control] x/y/w/h), an Ani-form skin (FrameAmount/FrameN images) and a controls file
    in the same INI form for what TQ's client says in code (kind: dialog, image, button,
    check, progress, text, grid; skin section, frame or piece; binding like hero.life; action
    like bag or toggle:<dialog>). Settings [ui] names the files; without them the engine's own
    UI shows. Conquer draws its 5065 main bar and bag from the client's own GUI.ini and
    Control.Ani. The Studio's Game UI panel draws the dialogs as the game does, with sample
    values: select, drag (one undo step), edit every field; files are patched byte for byte
    and written into the project's assets, so a client used in place is never written
- [x] Client front end from data: login and server list, character creation and selection,
      loading screens, settings menu (keys, video, audio); the game UI's minimap / radar
  - Done 2026-10-19: `c3-client --project P --front` starts at the front end: log in, pick a
    server (settings [[network.server]]), pick or make a character (name and one of
    settings [characters] looks; accounts keep up to per_account), wait while the world
    loads. Protocol 2 lists an account's characters and logs in naming one to enter or make.
    Screens come from the game UI's dialogs (screen=login/servers/characters/create/loading,
    with edit and list controls), else the engine's own. Conquer uses its native (not
    Flash) 5065 dialogs: CDlgLogin 129 and CDlgRoleCreate 151 from the client's GUI.ini and
    Control.Ani, one character per account entered at once, loading as the client's
    centred line. The menu's Settings page keeps the player's volumes and full screen; the
    radar shows over every game UI (the 5065 minimap window is M12, as decided)
  - [x] The 3D preview of the bodies on the creation screen (a model control, `front.look:N`),
        framed to the role's size and clear behind it
  - [x] Classes before looks (TQ's RoleChoose): `characters.class` (name, description, profession,
        looks, showcase figure), a `classes` screen, professions on players (saves, scripts'
        `profession()`, the 5065 hero info); Conquer's dialog 150 with 3DSimpleRole's figures
        and StrRes.ini's descriptions (`@file:key` texts)
  - [x] Server groups and pictures (Server.dat's model): `network.group`, each server's group and
        picture; image controls bound to data, clickable, with ImageFocus; Conquer's login shows
        CDlgLogin's group and server slots from ani/Server.ani, the engine's list has group tabs

## M10: Visual scripting (Unreal Blueprints-style; a whole Conquer game without code) [x] (complete 2026-10-19)
Decided 2026-10-18: every game rule and behaviour can be made in node graphs; Rhai stays as
an option, never a requirement. Graphs compile to the same commands and events scripts use,
run on the server and the local session alike, and are edited through c3-edit with undo.
- [x] Graph runtime: typed pins (number, text, bool, entity, item, map, cell, list), exec
      flow, variables (graph, player, world, saved), functions and macros, events, latent nodes
      (wait, timers, move-to), compiled to a checked form; errors point at the node
  - Done 2026-10-19 (decided with the user: graphs compile to Rhai): `scripts/blueprints/
    <name>.c3bp` compiles to `scripts/graphs/<name>.rhai`, run after `events.rhai` (and as
    formulas). Pins: exec, number, text, bool, entity, item, map, list, record (a cell is
    its x and y). Variables local, player (saved) and world; functions with inputs and an
    answer; waits and walks resume from on_timer / on_arrive; type and wiring problems name
    their node
  - [x] A cell pin type: cells as values ([x, y]), made, read, measured, walked to,
        teleported and spawned at
  - [x] Macros: a function marked macro is written out at each call (its nodes named
        apart per call); it may not wait or give back a value
- [x] Node library over the whole gameplay core: players, monsters, NPCs, items, skills,
      statuses, quests, money, teams, maps, portals, spawns, chat, dialogue pages, UI windows,
      effects and sounds, random, math, tables (read rows by key)
  - Done 2026-10-19: every script function is a node, and the script API grew for it (experience,
    life, statuses, world variables, random, effects, sounds, announcements, opening game
    UI dialogs, spawning monsters, walking)
  - [x] Team nodes, on a new team system: /team create, invite, join, leave, kick (up to 5);
        teammates near a kill share its experience; the team shows down the left; scripts and
        nodes see the team (in a team, size, leader) and tell it
  - [x] Teams across maps: a hero keeps its team when travelling (logging out leaves it)
  - [x] Team actions between maps: the server shares each map's roster and carries invites,
        notices and membership to the player's map; the team list shows teammates anywhere
        (kills are still shared only with teammates in view, as TQ does); GM `/map ID X Y`
- [x] Event graphs for every hook: login, enter/leave map and region, kill, death, use item,
      talk, dialogue choice, timer, level up, attack/skill formulas (damage, hit, XP, drops)
  - Done 2026-10-19: new hooks on_leave, on_death, on_level_up; dialogue choices are custom
    events; formula events (player stats, level exp, hit, kill exp, skill hit and heal) end
    at a Return
  - [x] A drops formula event: `drops(monster)` answers a list of item types and money
        (Drop items / Add money nodes); hit experience is a formula event too
- [x] Graph editor in the Studio: Blueprint-like canvas (search palette, typed wiring,
      reroutes, comments, collapse to function), live debugging in Play (breakpoints, watched
      values, the path that ran lit), find references
  - Done 2026-10-19 on egui-snarl (the user's choice, vendored for egui 0.37): the Blueprints
    panel, right-click palette with search, typed coloured pins and wires, values typed into
    unwired inputs, variables and functions, reroutes (typed by their wire), comments,
    collapse to function, Find uses across blueprints (click to go there), the path that ran
    glowing, watched output values and breakpoints pausing Play; undo, save and compile on
    every change
  - [x] Breakpoints on the very first events of a Play (the game starts with them; a hit
        pauses Play whichever panel shows)
- [x] Prove it: the Conquer project's NPCs, quests, formulas and events made only with graphs
  - Done 2026-10-19: the Conquer test project's events (welcome, map arrival, kill count,
    the timed gift) and NPCs (the Shopkeeper's lines, answers, purchase and loyalty reward;
    the Armorer's shop) are blueprints, with no hand-written script left; its quest is a
    dialogue graph. Played in the Studio: the same messages, dialogue and purchase as the
    scripts gave. An NPC names a blueprint as it names a script
  - [x] Conquer's formulas as formula graphs: a project's formulas (rules.rhai or formula
        blueprints) now answer over any game's own rules, Conquer's included

## M11: MCP and AI integration (moved before Conquer; the Studio assistant panel is dropped) [x] (complete 2026-10-19)
- [x] MCP server (`apps/mcp`): exposes c3-edit commands and queries to external agents (Claude
      Code): maps, tables, settings, assets, dialogues, scripts and graphs, play-testing
  - Done 2026-10-19 on rmcp (stdio): `c3-mcp <project>` opens a project on its own (headless),
    `c3-mcp --live [addr]` drives the open Studio (Window > Live Link, `--live`), whose
    documents take the calls so they show at once and undo with Ctrl+Z; a call never
    discards a person's unsaved work. 26 tools in `engine/agent` (c3-agent): query/edit/create
    maps, read/edit/create tables, settings, dialogues, scripts, blueprints, assets, undo,
    redo, save, and `playtest` (a game-master player doing inputs on a map without a window,
    answering each step's events and dialogue page and the player at the end). Maps save
    through the game (`GameModule::save_map`), as the Studio's do
- [x] Project context for AI: schemas, tables, map summaries, the node library and docs the
      model can query
  - Done 2026-10-19: project_overview, command_schemas (every command and query kind),
    read_table (columns with meaning and references), query_map summaries, node_library,
    script_reference, read_docs (docs/AGENTS.md, the agent's guide, and the engine's docs)

## M12: Conquer as a complete project (on the engine)
- [x] Full import in one step: the client's files plus TQ's server data (an SQL dump such as
      `tq_re/db/cq_finalized_mariadb.sql`): every map's NPCs (`cq_npc`, `cq_dynanpc`), spawn
      areas (`cq_generator`), portals and destinations (`cq_passway`, `cq_portal`), the map list
      and properties (`cq_map`), and the tables (monsters, items, magic, shops, drops, levels,
      stat points), mapped onto the engine's data-driven systems; an import report
  - Done 2026-10-19: File > Import Server Data… (and the agents' `import_server_data`) reads
    a game's server data through `GameModule::import_server_data` (Conquer: the TQ dump) and
    writes it in one step: 194 maps of 96 map files into Project Settings > Maps (id, name,
    PvP from `cq_map.type`), copies of a map file (mines, arenas, prisons) as maps of their
    own (`file@id`, their own population); 1512 NPCs (858 dynamic), 1695 spawn areas and 276
    portal links into 130 maps' populations, merged with the project's own NPCs and scripts;
    the 8 data tables; `import_report.txt` says what was left out (8 maps the 5065 client has
    no file for; rows of player homes, maps made while playing)
- [ ] NPC behaviour: TQ's NPC logic (`cq_action` / `cq_task` chains) converted into
      blueprints (user choice, 2026-10-19: blueprints, not dialogue graphs), editable in the
      Blueprints panel; the converter reports what it can't convert
  - Done so far (2026-10-19): the import makes each NPC's tasks one blueprint (`tq/npc_<task>`):
    tasks' checks (items by name, money, profession, PK range) as branches, answers leading to
    `task_<id>` events, shared chains as functions, delays as timers. 26,886 actions of 560
    NPCs convert: dialogue, pictures, chance, time windows, items (by type, name, range,
    worn), money, CPs, experience, life, mana, attributes, PK, profession, weapon skills,
    skills, teleport and return points, variables, statistics and timestamps, hair, sex, NPC
    numbers. The rest become comments, counted in the report:
  - [x] Moving NPCs (301, 259 actions): NPCs moving within and between maps, seen by clients
  - [x] Rebirth (1040, 200): the `rebirth` signal the rules blueprint answers
  - [x] Durability tests and the rest of equipment tests and numbers (510, 517, 516)
  - [x] Statistics tokens (528, 529), item overrides (501: gems, ident, bless, enchant,
        colour), level/experience sums (1084, 1086, `exp += %iter_var_data`), NPC upgrades
        (509: the `upequipment` signal; Meteors, DragonBalls, down-levels, durability)
  - [x] Map populations (302: the server tells each map every map's count), map effects
        (312: effects on a cell), lottery (1508: TQ's `cq_lottery` prizes drawn by weight)
  - [x] Typed answers (103): text boxes in dialogues (`input_option`, `accept()`,
        `Input::AnswerText`, MsgDialog's edit line), `%accept`, NPC data set from it (201)
  - [x] Client-only messages (1075 MsgAction 1000/1001, 1072 map item action 13) sent to
        the Conquer client as TQ sends them (`client_cue`)
  - [x] Guilds on a shared world state the server keeps (`_world.ron`): founding,
        disbanding, donations, ranks, leaders, enemies and allies, fund checks and gifts,
        finding guilds (701-707, 711-721), `%syn_id`, `%map_owner_id`, 1001 `rank`/`rankshow`,
        NPC owners (201); world variables reach every map
  - [x] Events (2003 compares, 2006 spawns, 2008 monster counts), team actions (1101-1107),
        NPC removal (205), task mask bits, sounds, heaven's blessing, the lock key, the timed
        experience percent, weapon sockets, typed answers, worked-out cells (1003/1004)
  - [x] NPC looks and life (201 `lookface`, `life`, `%npc_life`), `%user_id` (a lasting
        player number), transformations (513), unlearning weapon skills, point resets, the
        dying record
  - [x] TQ's trigger map beside NPC talks: items used (814 item types, `use_<type>`),
        monsters killed (281 types, `killed_<type>`, their loot 801), the login, death,
        revive and level-up hooks, the system timer every minute (52 chains, world events
        with no player), delayed chains (126), traps (424 placed, 144 types, `trap_<type>`,
        2101/2102), map effects, fireworks, chains for a map's players (2013)
  - [ ] Left in the trigger chains: drops on a map cell (304), statuses by TQ's bit ids
        (1082), deleting monsters (2009), moving a map's players (2012), destructible NPCs
        (training goals' link chains), NPCs placed by players (401/402)
  - [ ] PK bounties (2201-2205) and the wanted list (3001-3012)
  - [ ] Player homes (306 on `%user_home_id`, 1033-1035, 2007 item boxes)
  - [ ] Marriage (1024, 1025, 1102 `mate`), magic levels (1020 `check type lev`),
        unlearning weapon skills (1043), resetting points (1047), the dying record (1070)
  - [ ] Sub-guilds (708, 709), walking a guild's members (722)
  - [ ] The warehouse (1054, 1001 `storage_money`), business status (1001 `business`),
        cards and account transfers (1078/1079, 1088-1091, 1023), `%emoney_card` (1061)
- [ ] Conquer's rules as data and scripts on M7's gameplay core: formulas, professions, XP
      skills, PK and name colours, ghost and revive, item upgrades
  - Done so far (2026-10-19): the import writes the engine's rule tables from TQ's (items with
    requirements and CP prices, monsters, levels, shops, skills, statuses, class points) and
    the `conquer/rules` blueprint: life and mana by points and profession, attack and dodge,
    hit chance and damage, experience, the equip check, class points by level; Conquer's
    Rust rules are gone. Engine systems for it: player and item attributes, crime and PK
    points with name colours and guards hunting by name, ghosts and revive points, death's
    costs and jail, skills costing an attribute (XP skills) with the XP bar, upgrades by
    using one item on another (Meteor, DragonBall)
  - [x] TQ's damage and experience formulas: melee, archery and magic damage with the level
        difference table (`cq_disdain`), Dragon/Phoenix/Tortoise/Rainbow gems, magic defence,
        Superman, the armour floor, rebirth defence, blessing; battle experience
  - [x] Weapon proficiency: 1% damage a level above 12, experience from hits with Violet
        gems, learnt on the first hit, levels to 20 (`on_hit` / On hitting a monster)
  - [x] The left hand's half share of weapon experience; lucky time's double hits and
        dodged blows (`lucky_time` attribute)
  - [x] Wearing in a chosen slot (`Input::Wear`, items' `alt_slot`, `worn_at` kept in
        saves): one-handed weapons in the left hand, from the client's use-in-slot
  - [ ] Training goals' cut, what gives lucky time and its countdown
  - [ ] Team experience sharing by TQ's rule, Moon gems' magic experience
  - [x] `+N` composing, setting and removing gems, enchanting (life), sockets' gems counted
        while worn (`Input::Combine`, MsgGemEmbed 1027, compose 2036, item action 28)
  - [ ] Kylin gems' durability, the gem NPC's nearness check, fusing gear into +1 stones
        (Msg 1028)
  - [x] Profession promotions and rebirth end to end (NPC chains converted and played):
        WarriorGod promotes a Warrior with TQ's checks, skills and reward; Eternity's whole
        rebirth conversation reborns a Trojan King as a Warrior (stone, gem, skills, points,
        equipment levels), both play-tested through the MCP
- [ ] Conquer's GUI as a skin and layouts of M9's game UI (from the 5065 GUI ini and images),
      with its cursors and sounds
  - [x] Every 5065 dialog GUI.ini places drafted from the client's source
        (`scripts/conquer_controls.py`: 86 dialogs, 780 controls with their kinds, skins,
        backgrounds and grids; close buttons close); the main bar opens Status, Friend,
        Action, Team, PK and Guild; `c3-client --dialog N` opens one (screenshots: warehouse,
        team, actions)
  - [ ] 146 drafted controls without a skin section (built at run time in the client)
  - [x] Dialog pages (a dialog inside a dialog, `page:<id>` tabs) and bindings for any
        attribute (`hero.attr.<name>`), attack, defence, dodge, PK, profession; the status
        dialog's role page shows name, level, PK, attributes and points (screenshot)
  - [x] Status: the profession's name from the client's ProfessionalName.ini (`names=` on a
        text control: a file of value,name lines read in place)
  - [ ] Status: spouse, guild and rank, the details view, adding
        points, the weapon skill and equipment pages' contents
  - [x] Dialogs the game opens (`when=<binding>`, in place of the engine's window): an
        NPC's shop (152) with its wares (`shop.items`, a click buys) and the bag beside it;
        bag clicks use, wear or sell (screenshot: shop and bag); GM `/shop NPC`
  - [x] Talking with NPCs in CDlgTask (173): the NPC's words wrapped in yellow, its
        answers to click, shown while talking (screenshot: the Barber); GM `/talk NPC`
  - [x] NPC talk: the NPC's face (NpcFace.Ani's Face<n>, a bound image's `ani` prefix)
  - [x] NPC talk: typed answers (the text box shows when a page asks; `when` on controls)
  - [ ] NPC talk: a typed answer checked live, the dialog's height
        following its text
  - [x] The warehouse (CDlgDepot 161) while a storage NPC's is open: savings, an amount to
        deposit or withdraw, stored items (a click takes one out; a bag click stores one),
        the bag beside it (screenshot); GM `/storage NPC`
  - [x] Trading in CDlgTrade (137) while a trade is open: the partner's name, money, items
        and word, the hero's offer (a bag click offers an item), money, agree, close; asks
        to trade told in chat (layout screenshot; trade state unit-tested)
  - [ ] Trade: asking and accepting from the UI (no way to start a trade from it yet),
        checked live with two clients
  - [x] The team (CDlgGroup 141): its members' names in the five places (a local game asks
        its simulation), make or leave a team, break it up (screenshot)
  - [ ] Team: inviting and kicking from the dialog (they need a player picked), the
        members' faces, the auto-join and pick-up options
  - [x] The guild (CDlgKnightage 133): its name, leader, the hero's rank, members, fund and
        member list bound from the shared world state, Quit leaves (layout screenshot)
  - [ ] Guild: shown with a guild (no way to make one locally to look), its tabs (allies,
        enemies, bulletin), donating, joining
  - [ ] Data and actions per dialog: the
        warehouse grid and money, team members, friends, guild info, shop, trade, NPC talk,
        skills, compose, gem setting, PK mode; dialogs opened by the game (shop, warehouse,
        trade, NPC talk) rather than buttons
  - [x] A dialog opened closes those it would cover (TQ's side panels swap)
  - [x] UI sounds: settings [ui] sound_open / sound_close / sound_click, Conquer's
        DlgOpen, DlgClose and BtnClick (not heard in a test yet)
  - [x] A control's own sound (`sound=`, played in place of the click)
  - [ ] The minimap window; the client's Dlg_Ok, Sell and Equip sounds (Dlg_Ok.wav isn't in
        its sound folder); sounds on the login screens
- [ ] The 5065 front at parity: skills, items and equipment, shops, teams, trade, guilds,
      warehouse, cross-map travel; checked with the real Conquer.exe (still unchecked there:
      jump, teleport, items and equipment, shop buy/sell, other players)
  - [x] Spells: MsgInteract MAGICATTACK unscrambled into casts, MsgMagicEffect with each
        hit, the hero's skills as MsgMagicInfo (on entering and when learned)
  - [x] Trade (MsgTrade 1056), warehouse (MsgPackage 1102, saved money), gems (1027),
        compose (2036) and enchant
  - [x] Weapon skills as MsgWeaponSkill 1025 (on entering, and as hits raise them)
  - [x] Skill levels and experience: casts raise `magic_exp_<type>` by what they did, the
        `magic_levels` table (cq_magictype) says when a level goes up (play-tested through
        the MCP: Thunder 1950 + a hit → level 1), MsgMagicInfo carries both
  - [x] Skills by the caster's level of them: the engine's `skill_ranks` table (range,
        area, cooldown, mana, power per rank; `rank_attribute` names the level), filled
        from cq_magictype (play-tested: Thunder 29 at level 0, 33 at level 3)
  - [x] Items and money dropped on the ground (MsgItem 3 and 12; a dropped item keeps its
        plus and gems) and picked up (MsgMapItem 1101)
  - [x] Teams: MsgTeam 1023 (create, invite, accept, leave, dismiss, kick) and the member
        list MsgTeamMember 1026 with each member's life
  - [x] Teams: TQ's apply-to-join (1, 5) and closing a team to newcomers (8, 9)
  - [ ] Teams: the money and item pick-up locks (10 to 13)
  - [x] Guilds shown: each player's guild and rank in MsgPlayer, the hero's guild
        (MsgSyndicateAttributeInfo 1106), guild names (MsgName 3, asked by MsgSyndicate 6)
  - [x] Guild actions (engine `GuildAction`, MsgSyndicate 1, 2, 3, 4, 7 to 11): the
        leader invites, kicks and names allies and enemies; members join, leave and
        donate; the hero's guild info follows its fund and members
  - [ ] The guild member list (MsgName 11) and applying to a guild's leader
  - [ ] Each of the above checked with the real Conquer.exe

## M13: Any game of the kind (Eudemons-like or the user's own)
- [ ] Social and companion systems as engine modules a game switches on and shapes with data:
      teams, trade, guilds, friends, mail, pets and companions (summon, fight, level, evolve,
      equip), mounts, crafting, events and schedules
- [ ] Prove it: a second, small sample game made only in the Studio from its own art (no TQ
      files), with pets
- [ ] Easy for indies: New Project templates (Conquer-like, Eudemons-like, empty), the sample
      game, a getting-started guide in the Studio, tooltips, errors that say how to fix them

## M14: Ship
- [ ] Build from the Studio: package a playable client and server from a project (assets
      packed, settings applied), with per-platform build settings
- [ ] Platforms: Windows, macOS and Linux desktop clients; Android and iOS clients (wgpu on
      Vulkan/Metal/GLES, touch input, app lifecycle, texture compression and per-platform asset
      variants, store packaging); the server on Windows and Linux. Consoles (Xbox,
      PlayStation, Switch) need each maker's licensed SDK and dev kits, so the engine stays
      portable for them (no Windows-only code in the runtime, input and storage behind the
      engine) without promising a console build
- [ ] Server for real use: a database option (SQL) beside account files, an admin console
      (online players, kick, broadcast, save; `c3-server`'s doc already promises `online`, which
      doesn't exist), GM tools with permissions, logs
- [ ] Launcher and patcher
- [ ] glTF export (Blender pipeline); exporters back to the classic client where useful (today
      only DMap is exported: no C3, ANI, PUL-from-scene, DDS or WDF repack from engine content)
