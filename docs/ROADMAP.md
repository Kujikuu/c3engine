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
- [x] NPC behaviour: TQ's NPC logic (`cq_action` / `cq_task` chains) converted into
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
  - [x] Every TQ action of the dump converts (0 left in NPC chains and triggers): NPCs made
        while playing (2007, 403; their tasks' blueprints `tq/dyn_<task>`, carried to their
        map), NPCs laid by players (401 runs the laying task at once, at the player's cell),
        NPCs counted (402), guild teleport beacons (208), TQ's unknown type 0 failing
  - [x] Destructible NPCs: dynamic NPCs with life (`[gameplay.npc_combat]`: training goals,
        the guild-war pole and gates, also those made by 2007) are hit by blows and skills,
        their life a world flag (`npc<id>.life`), shown to the client as MsgNpcInfoEx 1109;
        at 0 their `linkid` chain runs (`npc_destroyed_<linkid>`, sort bit 0x10; play-tested:
        a Magic target hit six times)
  - [x] NPC laying through the client's placement: 401 asks (`ask_placement`, MsgNpc 2031 EVENT_LAYNPC), the client's MsgNpcInfo answers (`Input::Place`), its action runs as `lay_<action>` and 403 places the NPC there (`placed`); the engine's own UI places at the hero
  - [x] The guild war (`tq/guild_war`, made from the poles): the war on in the pole's weekly window (map status bit 1, 306 `status`), hits on the pole given back outside it (play-tested: 130 a hit, life kept at 20,000,000), each guild's damage scored and 2% paid from the owner's fund, 207 hands the map to the top scorer with a tenth of the old owner's fund; NPCs among blocked cells hit from the nearest walkable ring
  - [x] The wanted list (3001-3008) on the shared world state (`wanted.*` nodes) and text
        variables kept with the player (`text_var`); play-tested: Sunfer takes a 200,000
        bounty on a name typed
  - [x] A wanted target's killer who took the entry on is paid 70% (`on_kill_player`, the
        rules blueprint's lane; not run in a two-player test yet)
  - [x] PK bounties: a red-named player killed by a player outside map 6002 comes back in
        the black jail and a bounty is recorded on the killer (the rules blueprint, on the
        shared bounties); 2201-2204 ask, set, clear and claim (play-tested: Captain Zhang
        refuses with no bounty); 2205 fails as TQ's does
  - [x] The police wanted list (3010-3012) on the shared world state: black names put on at
        login (PK 1000) or for killing an innocent, jailed and taken off when killed by a
        player, the hunter of its target loses PK by the target's level (the rules
        blueprint; one node per event, extra lanes after a sequence)
  - [x] PK: the wanted and police orders' names sent to the client (MsgName 8 and 15,
        `client_cue_text`), TQ's refusal and success lines (play-tested: a 50,000 bounty refused
        "You should offer a reward of 100000 silvers at least!"), the boards' lines
        (`%iter_wanted`, `%iter_police_wanted` with the guild and PK columns), taken off the
        police list at logout (`on_logout`)
  - [x] PK: where a wanted target is (STR_WANTED_INFO, the police order's map name): the
        shared world state knows who is online on which map (`player_where`, the host's map
        names)
  - [x] Map type tests (306 `type test N`: the `map_types` table from cq_map, a bit-and
        node), magic levels (1020), map-cell spawns (2006), team counts nearby (1102)
  - [x] Player homes: maps made while playing on the shared world state (a copy of a map by
        its game id, served under its own id and key `file@id`, loaded when first needed);
        1033 makes the home (`home_id`), 1034/1035 enter it or the spouse's, 306 reads and
        sets its level, portal and map doc, 2007 puts its item box in (play-tested: the
        House Admin sells a house to a married player; the server serves a copy in a test)
  - [x] Homes: the team its leader takes home follows from within 5 cells (1034, `teleport_team`; the team visits the home with it)
  - [x] Married (1025), divorce (1024) and the couple's team (1102 `mate`) on the spouse's
        name kept with the player (`mate`; teammates' names in scripts)
  - [x] Marrying: the client's Court and Marry (MsgInteract 8/9 → `Input::Propose` / `AcceptProposal`), a man and a woman only (rules `may_marry`), fireworks and MsgName 6, TQ's line to all; a divorce reaches the ex-spouse online or not (world-state deliveries: its mate cleared, a MeteorTear), told to all
  - [x] Walking a guild's members (722, `{member}` in text)
  - [x] Sub-guilds (708, 709): founded for the first teammate, handed to it
        (`create_sub_guild`, `change_sub_leader`, a guild's leader changed for every map)
  - [x] Business status (1001 `business`, an attribute); the account's billing (CP cards
        1078/1079, bonus rows 1023, DragonBall transfers 1088-1091, `%emoney_card`,
        `%dragon_type`) as TQ's with no rows; 1061 with worked-out values (`%user_map_x`,
        `%iter_rand`); delays from tokens (1071)
- [x] Conquer's rules as data and scripts on M7's gameplay core: formulas, professions, XP
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
  - [x] Lucky time: Bless (cast-pray) grows it 3 s a second and makes players within 3 cells pray (1 s a second while a caster is near), else it runs down (play-tested: 15 s after 5 s of Bless)
  - [x] The kill bonus doubled with a newbie teammate near (`team_newbie`, after the team's share) and ×10 at 0.5% in lucky time
  - [x] Training goals' cut: 10% of the life taken (an archer's half), none above the attacker's level (play-tested: 1 a hit at level 46 on a level-45 target, 0 at level 30)
  - [x] Team experience sharing by TQ's rule (rules `team_exp`: within 32 cells of the kill, newbie, spouse, healer and Rainbow factors)
  - [x] Moon gems' magic experience (their percent on a cast's skill experience)
  - [x] `+N` composing, setting and removing gems, enchanting (life), sockets' gems counted
        while worn (`Input::Combine`, MsgGemEmbed 1027, compose 2036, item action 28)
  - [x] Kylin gems' max durability (an item's own `max_dur`, set and taken back with the gem),
        the gem setter's nearness (`npc_near`, kind 7 within 20)
  - [x] Repairs (MsgItem 14): TQ's cost by lost durability and quality, 5 Meteors for a broken
        item, the max durability's decay, next to a blacksmith (play-tested: 61 silver for a
        blade at 3 durability; GM `/durability ITEM N`)
  - [x] Fusing gear into +1 stones (MsgFuse 1028, up to 20 items; `cq_fuse` as the `fuse` table; scripts read every item with `combine_items`; play-tested: full-durability helmets refused, worn ones erased and rolled)
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
  - [ ] 9 drafted buttons without a skin section, and 89 images the client fills at run time
        (dice, equipment, XP skills, NPC faces lists) without bindings
  - [x] Dialog pages (a dialog inside a dialog, `page:<id>` tabs) and bindings for any
        attribute (`hero.attr.<name>`), attack, defence, dodge, PK, profession; the status
        dialog's role page shows name, level, PK, attributes and points (screenshot)
  - [x] Status: the profession's name from the client's ProfessionalName.ini (`names=` on a
        text control: a file of value,name lines read in place)
  - [x] Status: spouse, guild and its title (StrRes.ini by `map=`), the details view
        (`view:` actions, `!` in `when`, the rules' `shown`: attack and magic with Dragon and
        Phoenix gems), adding points (`spend:`, `Input::SpendPoints`, MsgAllot 1024)
        (screenshot)
  - [x] Status: the weapon skills page (three classes of six, `Weapon<type>` icons and levels from the hero's `weapon_<type>` attributes; `hero.attr_at`, `place=`, `&` in `when`) and the equipment page (`hero.worn.<slot>`, a click takes an item off) (screenshot); item icons found as the client finds them (ItemMinIcon.Ani's `Item<key>`: helmets, armour and shields by colour × 1,000,000 + type, colour 3 by default; else ItemDefault; `item_look_attribute`)
  - [ ] Status: a weapon skill picked shows its name, level and progress
  - [x] Dialogs the game opens (`when=<binding>`, in place of the engine's window): an
        NPC's shop (152) with its wares (`shop.items`, a click buys) and the bag beside it;
        bag clicks use, wear or sell (screenshot: shop and bag); GM `/shop NPC`
  - [x] Talking with NPCs in CDlgTask (173): the NPC's words wrapped in yellow, its
        answers to click, shown while talking (screenshot: the Barber); GM `/talk NPC`
  - [x] NPC talk: the NPC's face (NpcFace.Ani's Face<n>, a bound image's `ani` prefix)
  - [x] NPC talk: typed answers (the text box shows when a page asks; `when` on controls)
  - [x] NPC talk: the dialog's height following its words and answers (`stack=1` controls,
        a `fit=104` dialog stretching its picture, as CDlgTask's m_nDlgHeight) (screenshot)
  - [ ] NPC talk: a typed answer checked live in the client
  - [x] The warehouse (CDlgDepot 161) while a storage NPC's is open: savings, an amount to
        deposit or withdraw, stored items (a click takes one out; a bag click stores one),
        the bag beside it (screenshot); GM `/storage NPC`
  - [x] Trading in CDlgTrade (137) while a trade is open: the partner's name, money, items
        and word, the hero's offer (a bag click offers an item), money, agree, close; asks
        to trade told in chat (layout screenshot; trade state unit-tested)
  - [ ] Trade: an ask accepted by asking back (the ask is told in chat); checked live,
        checked live with two clients
  - [x] The team (CDlgGroup 141): its members' names in the five places (a local game asks
        its simulation), make or leave a team, break it up (screenshot)
  - [x] Picking a player: a `pick:<what>` action arms it, the next click on a player trades
        with, invites, kicks or asks to join (the main bar's Trade, the team's invite and
        kick)
  - [x] Team: the members' faces (PlayerFace.Ani as a prefixed skin, `path#Prefix`), the
        buttons by the hero's place (Create/Join, Invite/Dismiss, Leave), the options menu
        (MenuGroup 268: closed, money, items, as the team's), the auto check taking invites
        and asks (screenshot)
  - [x] The guild (CDlgKnightage 133): its name, leader, the hero's rank, members, fund and
        member list bound from the shared world state, Quit leaves (layout screenshot)
  - [x] Guild tabs: Status, Allies and Enemies switch the list (members, allied and enemy
        guilds' names)
  - [ ] Guild: shown with a guild (no way to make one locally to look), the bulletin,
        donating, joining
  - [x] Windows the game opens by number (1046 → `ui.open`; MsgAction 126 to the Conquer
        client; `window=N` on a dialog); composing in CDlgItemCompose (275, window 1): a
        workbench (`work.N` slots filled by bag clicks, `combine:<kind>`) (screenshot)
  - [x] Gem setting in CDlgEmbed (229) on the workbench: each placed gem set in its socket,
        Remove takes socket 1's out (`combine:<kind>:<value>:<slots>`, `;` between actions)
  - [ ] Compose and gems checked with real items; choosing the socket to remove; the oven
        (windows 2, 3)
  - [x] The skill strip (CDlgMagic 188, the bar's Skill box): the hero's skills as Magic.Ani's
        icons (`skill.N`), a click casts (`cast:N`) (screenshot)
  - [ ] Skills: the strip right-aligned to its skill count, the XP and Kongfu strips
  - [ ] Data and actions per dialog: the
        warehouse grid and money, team members, friends, guild info, shop, trade, NPC talk,
        skills, compose, gem setting, PK mode; dialogs opened by the game (shop, warehouse,
        trade, NPC talk) rather than buttons
  - [x] A dialog opened closes those it would cover (TQ's side panels swap)
  - [x] UI sounds: settings [ui] sound_open / sound_close / sound_click, Conquer's
        DlgOpen, DlgClose and BtnClick (not heard in a test yet)
  - [x] A control's own sound (`sound=`, played in place of the click)
  - [x] The sound of selling ([ui] sound_sell; Conquer's Sell.wav)
  - [x] The sound of dressing ([ui] sound_equip on `Dressed`; Conquer's equip.wav)
  - [x] The minimap: the radar shows each map's picture from the client's MiniMap.ani, as
        its CRadar does
  - [x] The minimap's marks: the game's pictures for the hero, players and team leaders
        (`GameModule::radar_mark`; MiniMap.ani's hero, common.ani's PlayerFlag and
        TeamLeader) in place of dots (screenshot)
  - [ ] The minimap's tutor and student marks; Unequip.wav apart from equipping; Dlg_Ok (Dlg_Ok.wav isn't in
        its sound folder); sounds on the login screens
  - Audit 2026-10-20 (docs/conquer_ui_audit.md): every dialog the 5065 client creates, checked against its code; each one below is open until it matches.
  - [ ] UI engine: the client's window model
    - [x] dialogs own a part of the screen (`slot=left|middle|right`, from CDlgMain::CloseCurDlg) and clear others as they open (`clears=`); the overlap rule is gone, so HUD strips and child dialogs no longer close each other
    - [x] Esc closes the right part and the bag (`esc=1`) as CMyShellDlg::CloseCurDlg does, after a question, the menu and an NPC talk
    - [x] the generator (scripts/conquer_controls.py) writes the same slots into drafts
    - [ ] openers gated as the client gates them: status only over friends/enemy/mentor/business, shops/depot/compose not while trading, item lock only over its list
    - [ ] opening a shop, depot, oven, enchant or compose also shows the bag, and closing them hides it
    - [ ] Esc with nothing left to close asks to leave (EXIT_CONFIRM); Esc while trading refused (StrRes 10026)
  - [ ] UI engine: a yes/no prompt (CDlgConfirm 158)
    - [x] one question at a time (`prompt.*` bindings, `prompt_yes`/`prompt_no`, `ask:<kind>:<actions>` for the player's own actions); trade and team asks lapse after 10 s; a second trade ask is turned away (StrRes 10433)
    - [x] trade, team invite/apply, guild invite/apply and marriage asks put to the hero; leaving a guild and taking a chat name off asked first
    - [ ] an amount typed in the prompt (modes 5, 9, 10, 11: dropping money, donating, booth prices) and the gem take-out, item drop and important-item asks
    - [x] Enter/Esc answer it
    - [ ] refusing a team ask tells the asker (CMD_TEAM_CONFIRM_REJECT)
    - [ ] the engine's own UI (no game dialog) still turns asks into chat lines
  - [ ] UI engine: items in grids
    - [x] `pick=1` grids: the left button picks an item up (drawn at the pointer) and puts it on another grid: sold, stored, taken out, worn, taken off, bought, offered in a trade, put on a workbench; the right button uses (Esc or the right button lets it go)
    - [x] item tooltips under the pointer (what it is, its stack, quality and durability)
    - [ ] the tooltip drawn as TQ's (Dialog21 box, its colours and lines)
    - [ ] an item put down on the ground drops it (with the client's confirms); on the quick bar
    - [x] quality lights under item icons (`GameModule::item_glow`; Conquer: GetQualityEffect's effect.ani lights)
    - [ ] +N digits (6159's Equip_Num art; 5065 shows none); grid paging
  - [ ] UI engine: control states and kinds
    - [x] comparisons in `when` (`hero.level>=70`, `<=`, `!=`, `>`, `<`, `=`, against numbers or bindings); `hero.wallet.<currency>`
    - [x] images picked by a binding (an image's section from its value: the picked skill)
    - [x] disabled button frames (`disabled=`: frame 2, no clicks)
    - [x] latched button frames (`latched=`: frame 1, no clicks)
    - [x] sliders (`type=slider`)
    - [ ] multi-line edits
  - [ ] UI engine: the dialogs the server opens (window numbers and events) wired for every dialog that has one
  - [ ] UI engine: the engine's own widgets hidden where the game draws its own
    - [x] skill bar, radar circle (the game's box), chat log (the game's look), hint line and team list
    - [ ] status row and quest windows
  - [ ] UI 130 CDlgMain (main bar): 6 of 19 audit mismatches fixed (docs/conquer_ui_audit.md)
    - [x] life and mana orbs at the client's image sizes (36x74, 34x74; `area=`), life as the whole orb (frame 2, 86 wide) for a hero with no mana (DlgMain.cpp 166-172, 726-735)
    - [x] XP orb 1018 (Progress42, 92x92) and stamina 1025 (Progress46, 8x70, under the bezel) bound to the `xp` and `stamina` attributes (attribute defaults now read by `hero.attr.*`)
    - [x] exp 1019 a pixel up (y 96, DlgMain.cpp 796); Control.Ani has no Progress44, so like the client it draws nothing with the default skin
    - [x] message board 1006 toggles 240 and setup 1008 toggles 138 as OnMainLeaveWord / OnMainSetup do (both dialogs still drafts, below)
    - [x] map check 1015 shows and toggles the radar (frame from its visibility)
    - [x] a hero made without the create screen gets the game's profession (`new_character.profession`; Conquer: Trojan) so it has points and life
    - [ ] life orb's mana latch: the client keeps the whole-orb life until mana is first above 0, then never goes back (we follow max mana)
    - [ ] XP orb's 500 ms mode-2 flash on an XP change (DlgMain.cpp 849-864)
    - [ ] stamina x1.5 while Blessed, and extra stamina 1027 (Progress47) above 99 (DlgMain.cpp 703-709, 757-762)
    - [ ] life/mana forecast (value vs current, two-pass draw; MyProgress.cpp 105-136)
    - [ ] quick bar 1021 (1x10 at 91,101): items, skills and emotes dragged in, F1-F10, use effects
    - [x] skill box 1022 shows the skill picked from 188 (MagicSkillType) and the right button casts it (`cast_picked`, "Mouse Right"); opens 188 only with skills
    - [x] XP skills picked from 189 shown as XpSkillType in the skill box; the box's Magic0 art replaced by the skill picked
    - [ ] radar toggle transition (the client rolls it)
    - [ ] screen move 1023 (free camera, ON_MAIN_SCREENMOVE) and view equipment 1016 (next player clicked opens 258)
    - [x] PK button image by PK mode (Button47/49/48/412)
    - [ ] hover tooltips on exp, run, map, screen move, view equipment, XP, MP, HP (Dialog21, StrRes 10070-10074, 10370)
    - [ ] trade 1010: dead-hero refusal (StrRes 10712) and the 10065 prompt
    - [ ] button sounds as the client: BtnClick on most, DropMoney on 1010, none on 1011/1024
    - [ ] actions 1009, PK 1013 and friends 1026 open 140 / 191 / 160, still drafts
  - [ ] UI 145 CDlgTalk (chat bar): 10 of 16 audit mismatches fixed (docs/conquer_ui_audit.md)
    - [x] line 1542 takes 88 characters, name 1544 15, the name red while listed (`limit=`, `mark=`)
    - [x] Down/Up step through the last 20 lines said, as CDlgTalk does (Down older, Up newer)
    - [x] Ctrl+Up/Down cycle the channel, Ctrl+Left/Right the last 5 channel-and-name pairs
    - [x] lines half a second apart at least (StrRes 10044); line breaks taken out
    - [x] List 1185 opens the names list 148 (Dialog24, yellow names, a pick fills the name and hides it); Add 1545 switches Button57/58/59 and lists or takes off the name
    - [x] Log 1095 toggles 139; Action 1096 toggles 274 and closes 191 (`close:N`)
    - [ ] broadcast (ad_channel) mode: tip 11034, placeholder 11023, level 50 / 5 CPs / 80 bytes, prompt 296; and 174's [174-1549]
    - [x] friend channel (2009): friends and enemies in the game (`FriendAction`, `WorldChange::Friendship`/`Enemy`, a killer kept as the victim's enemy)
    - [ ] service channel (2014, 20 s cooldown StrRes 10444)
    - [ ] taking a listed name off asks first (CDlgConfirm 158, needs the yes/no prompt)
    - [ ] 148: kept between sessions, hidden when the mouse leaves, its arrows [148-1]/[148-2]
    - [ ] inline #NN emoticons in the line (MyCombo.cpp 98)
    - [ ] Clear 1092 clears every message area (system, GM) and resets banner 295
    - [ ] Shift+click a player, Alt+click a log name and the friend/guild/business/leave-word/pigeon chat routes fill the name
    - [ ] clicking the bar focuses the line; 1094 as one image changed by the channel
    - [x] the log as g_objGameMsg draws it: said lines `[Talk]Ann speaks to All: hi[h:m]` in yellow, 6 up from 130 above the bottom, shadowed; system notes ([System]) at the top left, 3, gone after 15 s (`GameModule::chat_look`, StrRes prefixes)
    - [ ] the GM area (2011) mid-left; the log's line count dragged by 5326; Alt for clickable names
    - [x] blacklist / channel filters (139, 239)
    - [x] 274 CDlgTalkAct (the emotion picker)
  - [ ] UI 174 CMenuTalk (channel menu): partial, 2 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 188 CDlgMagic (normal-magic skill strip): 6 of 11 audit mismatches fixed
    - [x] a click picks the skill for the skill box and hides 188 and 214 (ON_MAGIC_CHOOSE)
    - [x] the engine's egui skill bar is not drawn when the game UI shows the skills
    - [x] the opener does nothing without skills
    - [x] right-aligned and sized by skill count (x = 1024 - n*50 - 18; `stretch=`)
    - [ ] paging 1275/1276 past 10 skills
    - [x] only magic skills (use_xp 0) in the strip: skills carry the game's group (`SkillType::group`, `skill_in.<group>.N`); Kongfu in 214, XP in 189
    - [ ] skills listed as GetMagicByIndex does (no dwScreenRepresent 3, no transform magics while transformed; Kongfu skills in 214)
    - [ ] disabled skills covered (Image0) and not pickable
    - [ ] hover tooltip and drag to the quick bar
    - [x] the opener also toggles 214
    - [ ] the opener hides 189
    - [ ] 140 / 191 opening no longer close it (window model)
    - [ ] hidden on death and booth close
  - [ ] UI 5278 CDlgMipmap / radar (minimap): 3 of 5 audit mismatches fixed
    - [x] the radar as CRadar's crop: MiniMap.ani's picture upright in a 170x128 box top right, at 0xaa, around the hero and clamped to the picture (`ui.radar_box`)
    - [x] hero mark (MiniMap.ani hero) and teammates (PlayerFlag, TeamLeader); no monster/NPC dots, as the native client
    - [x] 130-1015 toggles it and its frame follows
    - [ ] the grow/shrink transition (SetTransition)
    - [ ] overview and full modes with Button01/Button02; MapModeRecord kept
    - [ ] the hero mark animated (all its frames); the spouse mark (PlayerFlag frame 2), the leader mark only for 8 s after MsgAction 101, Friend mark 6 s
    - [ ] maps with fewer than 4 minimap frames drawn as ShowFallback
  - [x] UI 304 CDlgAutoRunToggle5065 (auto-path HUD button): the 3 audit mismatches fixed (303 itself is its own item)
  - [ ] UI 325 mentor HUD button: 3 of 5 audit mismatches fixed
    - [x] always shown, btn-dssy alone (no Dialog531 frame, no Button532)
    - [x] its click opens the mentor contribution 319
    - [ ] tooltip StrRes 100220
  - [x] UI 328 CDlgEquipTie5065 (item-lock button): the 2 audit mismatches fixed (329 itself is its own item)
  - [ ] UI 272 CDlgInsHelp (instant help): 1 of 8 audit mismatches fixed
    - [x] the folded Button490 always shown
    - [ ] its click unfolds the 494x288 Dialog49 help with Tip/PageUp/PageDown/More and folds back on close
    - [ ] flashing at milestone levels; help text bound
  - [ ] UI 295 CDlgPigeonBanner5065 (broadcast banner line): 3 of 6 audit mismatches fixed
    - [x] always shown; ad_query toggles 294; the latest line heard (name in 0xaaffaa, StrRes 100018, the line); the chat menu's ad_channel picks the broadcast channel
    - [ ] placed by the line slider, Ctrl+click whisper, tip 11042 and the "Pigeon" effect
  - [ ] UI 289 CDlgEmoneyShop5065 (CP-Mall button): 1 of 2 audit mismatches fixed
    - [x] shown from level 70, after a second rebirth or with CPs, as RefreshEmoneyShopVisibility5065
    - [ ] its click opens the CP shop (2888) and plays BtnClick
  - [ ] UI 173 CDlgTask (NPC talk): 11 of 16 audit mismatches fixed
    - [x] words run together (`dialogue.said`) and wrapped at 420
    - [x] answers in two columns at 20 and 282, 210 wide, wrapped, from y 104 at least, 8 at most (`step`, `cell`, `wrap`, `limit`, `min_y` on lists)
    - [x] answer text red under the mouse (`hover=`), no lit box
    - [x] the typed answer: a yellow title, an Edit230 box sized by the page's limit (`limit_of`), Button230 1549 beside it (`beside=1`); not a list row; an empty answer not sent
    - [x] Esc closes it, as the shell's CloseCurDlg does first (DlgTask's own IDCANCEL is swallowed)
    - [ ] the typed box in the column of its answer index (x 40 or 282) and its 44 px slot; password typing from the server's data
    - [ ] fit=104 only when there are answers or a box
    - [ ] centred on the real screen width rather than the 1024 layout
  - [ ] UI 153 CDlgMpcGoods (bag): 9 of 14 audit mismatches fixed
    - [x] CPs 1129 and money with thousands commas (`commas=1`, `hero.wallet.CPs`)
    - [x] drop money 1561: asked with an amount (158 mode 5: Drop Silver, Dialog56, Edit150), then dropped
    - [x] close 1128 and the bag button hide it while a shop, warehouse or trade shows it, until that ends (dismissing a `when`-shown dialog)
    - [x] opened with compose 275, oven 278, enchant 105, improve 211, embed 229, booth 233 and item lock 329; oven, enchant and compose hide it again (`opens=`, `hides=`)
    - [x] the shop's close also hides the status 131
    - [ ] EmoneyNavigation 1562 opens the [Emoney] URL; help 1667 opens CDlgHelp topic 13
    - [x] grid gestures: left-click picks up and puts in the equipment, shop, warehouse; right-click uses
    - [ ] dropping on the ground and the hotbar
    - [x] hover tooltip (engine look)
    - [x] quality lights under icons
    - [ ] the suspicious-item reif effect
    - [ ] repair mode
    - [ ] slide-in from the right; bag and 287 hiding each other
  - [ ] UI 131 CDlgQuery (status): 6 of 11 audit mismatches fixed
    - [x] tab checks lit for their page, the role's first (check values take `when` expressions); the weapons' tab a pixel lower
    - [x] opens only when the right part is free of a shop, warehouse, workbench, lock list or guild (`blocked=`); the right part's rule closes others
    - [x] hidden by shop close and setup (`clears=131,160,269`); Esc closes it
    - [ ] skills tab 1028 and page 163
    - [ ] power button 1464, battle-power digits and 286/287
    - [ ] help 1667 by page (CDlgHelp 215)
    - [ ] slide-in, open/close sounds; hotkeys 20/21/22/43
  - [ ] UI 132 CDlgQuery_Role (role page): 3 of 5 audit mismatches fixed
    - [x] PK in the client's colours (red 20-99, yellow below 0, black from 100)
    - [x] spouse and guild "None" (StrRes 10075) when there is none (text bindings count as set when not empty)
    - [x] add buttons play AddPoint.wav
    - [ ] the PK title prefix from Common.ini [PkTitle]
    - [ ] the details from GetMinAtk/GetMaxAtk/GetDef with item additions and gem percents rather than the rules' shown values
  - [ ] UI 134 CDlgQueryWeapon (weapon skills page): 1 of 6 audit mismatches fixed
    - [x] "other" holds every type that is neither 4xx nor 5xx (attribute ranges between commas)
    - [ ] levels as Weaponlevel5/1/20 pips
    - [ ] a clicked icon's name, level, exp percent and description
    - [ ] paging by 6 while more exist
    - [ ] arrival order and level-0 skills
    - [ ] help topic 11
  - [ ] UI 135 CDlgEquip (equipment page): 3 of 7 audit mismatches fixed
    - [x] treasure 1679 and mount 1680 places
    - [x] left hand only for professions that hold one, over Image160
    - [x] hover tooltips; pick-up and put-down to wear and take off (UI engine: items in grids)
    - [ ] donation and nobility images (nob_<rank>64)
    - [ ] the hero's model at +80,+225
    - [ ] double-click to take off; requirement checks on wearing
    - [ ] colour variants and +N
  - [ ] UI 163 CDlgSkill (skills page of 131): missing, 1 mismatches (docs/conquer_ui_audit.md)
  - [ ] Battle power: the game module counts it (`GameModule::battle_power`, `c3_ui::Power`); Conquer as CHero::CalculateBattlePower (level, rebirths x5, quality / sockets / super gems / plus of what is worn, a two-hander twice, arrows and a shadowed left hand nothing) plus the noble rank
    - [x] the count, the status page's number (toggling 286) and the two dialogs
    - [ ] the mentor's shared battle power (none yet), TQ's 3D digits on the status page, another player's battle power (258)
  - [ ] UI 286 CDlgBattlePower: 4 of 5 audit mismatches fixed
    - [x] Dialog60, the seven parts and total, level (10083) and rebirths (10500), detail buttons (other only when it gives) opening 287 in their mode; toggled by 131-1464
    - [ ] placed beside the status page (262,95 as DlgQuery moves it), help topic 48
  - [ ] UI 287 CDlgBattlePowerDetail: 3 of 4 audit mismatches fixed
    - [x] Dialog61, seven rows (item, name coloured by quality or grey when shadowed, what it shows in the mode: 10075-10080 / 10494-10499 / 11055, score), the mode's total
    - [ ] the other mode's noble rank rows (SetOtherRows), the quality word before the name, hiding 258 and the bag on opening, the 300 ms delay
  - [ ] UI 152 CDlgNpcBuy (NPC shop): 6 of 14 audit mismatches fixed
    - [x] CP shops draw Dialog58 and hide Fix (`shop.priced_in.CPs`); wares are sold for the shop's currency (`cq_goods.moneytype` → the shops table's `currency`, items' `price_CPs`; `Rules::shop_currency`, `ItemType::price_in`)
    - [x] paging 20 wares with Button203/204 and ChangePage.wav (`offset=` grids, `view_step:` bounded by `count.<list>`)
    - [x] buying by the right button or a drag onto the bag; selling by a drag from the bag
    - [x] Esc closes it; the frame a pixel up
    - [ ] Fix 1105: the repair cursor and prompt
    - [ ] help 1667 (CDlgHelp, 45 for CP shops)
    - [ ] a CP buy asks the amount (290); a valuable sold asks first (IMPTITEM_CONFIRM)
    - [ ] price rows in the tooltip; +N and colour on wares
    - [ ] slide-in and DlgOpen/DlgClose; refused while trading or with an NPC booth open
  - [ ] UI 161 CDlgDepot (warehouse): 8 of 14 audit mismatches fixed
    - [x] page 1626 and page count 1627 (`view_page.`, `list_pages.` bindings)
    - [x] page buttons only with more than one page, paging 20 at a time
    - [x] money right-aligned with commas (`align=right`); the amount box 12 digits, digits only (`digits=1`)
    - [x] store and take out by drag or the right button; tooltips
    - [x] Esc closes the warehouse
    - [ ] help 1667
    - [ ] deposit only when 0 < amount <= money; live commas while typing; focus on open
    - [ ] bag-full / warehouse-full messages and the locked-item confirm
    - [ ] quality, +N, colour and reif on stored items
    - [ ] slide-in and sounds; refused while trading
  - [ ] UI 141 CDlgGroup (team): 8 of 13 audit mismatches fixed
    - [x] faces only, no names; a face clicked is picked, with the focus frame
    - [x] Kick sends out the member picked and is disabled (frame 2) until one who does not lead is (`disabled=`, a TQ button's disabled frame)
    - [x] setup 1648 and Kick 1649 for the leader only; Help as Button444
    - [x] the member's second button no longer leaves (Follow is not in the engine yet)
    - [x] closing it closes the options 268 (`hides=`)
    - [x] invite and apply asked with the yes/no prompt (158 modes 2/1)
    - [ ] each member's own face (the engine has no faces: every hero wears face 1)
    - [ ] Follow (TeamFollow) for a member
    - [ ] help 1667 (CDlgHelp DLG_GROUP)
    - [ ] the auto check's aura-team rules
    - [ ] double-clicking a face starts a team whisper; living-player checks on Join/Invite picks
  - [ ] UI 268 CMenuGroup (team options): 1 of 3 audit mismatches fixed
    - [x] opened by the leader only, and closed with 141
    - [ ] hidden when the team changes or the bag opens
    - [ ] first show after creating a team sends Money on / Item off
  - [ ] UI 158 CDlgConfirm (generic yes/no prompt): 7 of 14 audit mismatches fixed
    - [x] opens while a question is asked; Dialog15 at -5,-5 (Dialog45 at -25,-25 for a proposal); centred yellow text from StrRes (name, 10000/10002-10005/10365, 10001; 10009, 10008); OK runs it, Cancel drops it
    - [x] modes 0-2 hide after 10 s; a second trade ask refused
    - [x] amount edits and Dialog56/57 icons for dropping money (5), donating (9) and booth prices (10); the booth-open ask (13, StrRes 10364)
    - [ ] Exercise (27), detail rows (32), Cancel hidden in modes 30, 34, 35, 52-54
    - [x] Enter/Esc mapped to OK/Cancel
    - [ ] the other 40-odd modes (booth, mentor, item lock, offline training...) as their features land
  - [ ] UI 137 CDlgTrade (trade): 9 of 21 audit mismatches fixed
    - [x] placed at (10, 242) as CDlgMain moves it
    - [x] the partner's level (StrRes 10442) and money with commas
    - [x] the money box 11 digits only; OK refuses none (StrRes 10045) and latches once taken; Agree refuses money typed but not offered (10046) and latches (`latched=`)
    - [x] six goods a page on each side, with page buttons only when needed and the page numbers
    - [x] offering by putting a bag item on the hero's side; tooltips on both sides
    - [x] an incoming ask answered Yes/No (158 trade mode)
    - [ ] CPs: the partner's 1150 and the hero's 1573 (trading CPs in the engine)
    - [ ] the edits disabled once money is set and the hero's grid once it agreed
    - [ ] help 1667; nobility icons 1619/1566; the business partner button 2070 and assessment 2087
    - [ ] the name's block and invalid-glyph marks
    - [ ] quality, +N; 20-item cap (10390); buddy/protection tips and the locked-item confirm on offering
    - [ ] the dead-hero refusal (10712) and the 10065 prompt from the main bar's trade button
    - [ ] checked live with two players
  - [ ] UI 140 CDlgAct (emotes): 3 of 6 audit mismatches fixed
    - [x] emotes in the game (`Input::Emote`, `Event::Emoted`; the view plays the action once, held or looping by `Looks::emote_play`: Conquer holds sit, kneel and faint and loops the dance); Conquer's MsgAction 81 both ways
    - [x] the ten ButtonA buttons show their emotes
    - [x] 188/191 no longer close it; Esc as the client
    - [ ] dragging an emote onto the quick bar
    - [ ] emote hotkeys (user actions 10-19)
    - [ ] BtnClick.wav from the main bar's button
    - [ ] checked live: the pose seen in a running client (the start-up --act runs before the actors load)
  - [ ] UI 191 CDlgPk (PK modes): 8 of 9 audit mismatches fixed
    - [x] PK modes in the game: free, peace (monsters only), team (spares team, guild, spouse), arrest (bad names only) (`Input::SetPkMode`, `Player::pk_mode`, `PkModeSet`); Conquer's MsgAction 96 both ways; players enter in arrest mode (`pk.start_mode`)
    - [x] the four buttons (Button47/49/48/412) set their mode and hide the picker
    - [x] the main bar's PK button shows the mode
    - [x] 140/188 no longer close it (window model)
    - [ ] PK-cycle hotkey (user action 40)
  - [ ] UI 189 CDlgXp (XP / revive strip): 3 of 6 audit mismatches fixed
    - [x] death revive mode: shown while the hero is dead at (906, 595), 118 wide; XpSkillType0 revives (`revive` action)
    - [x] placed bottom right as the client moves it, not by GUI.ini
    - [ ] XpSkillType5 revive-here (needs Blessed and a revive-here map; StrRes 10484/10485), and the 20 s ghost gate message (100033)
    - [x] XP-full mode: the XP skills (`skill_by.xp.N`) as XpSkillType<type>; one aimed at the hero cast at once, another picked into the skill box (`use_skill:`), shown on XP full
    - [x] the strip as wide as its XP skills (MoveActionStrip)
    - [ ] paging 1277/1278
    - [ ] land, abort-transform and booth-close modes
    - [ ] checked live with a dead hero
  - [ ] UI 214 CDlgKongfu (Kongfu skill strip): 4 of 5 audit mismatches fixed
    - [x] opened with 188 by the skill box
    - [x] the hero's Kongfu skills (use_xp 2) as MagicSkillType; a click picks one for the skill box and hides both strips
    - [x] right-aligned at y 768-223, as wide as its skills
    - [x] a Kongfu-only skill list (`skill_in.kongfu.N`)
    - [ ] paging 1279/1280 past 10
  - [ ] UI 133 CDlgKnightage (guild): 17 of 20 audit mismatches fixed
    - [x] one page at a time (attributes, announcement, editing, allies, enemies, members) with Dialog28 / Image80 / Image81; opens on the announcement
    - [x] 1039 the hero's contribution, 1040 the rank title (StrRes); guilds keep members' contributions (`Guild::contributions`, `WorldChange::Contribution`)
    - [x] the announcement shown (StrRes 100158 when none) and edited by the leader (`Input::GuildNotice`, `WorldChange::GuildNotice`), OK / Cancel
    - [x] allies and enemies in their five statics
    - [x] Leave / Members and Apply / Edit by rank (Button87/89/81, Button82/88/89); a member leaves only after giving 20,000 (StrRes 10341) and is asked first; Apply picks the guild to join
    - [x] Donate asks an amount (confirm mode 9, StrRes 10010)
    - [x] members 10 a page with level, paged; a row picked; the leader's Delete (disabled until one is picked) asks first (10360)
    - [x] buttons disabled without a guild; closing hides 257; guild join/invite asks (158)
    - [ ] members' online state in the list
    - [x] clicking a member opens 257 with its details (a list runs its action for the row clicked)
    - [ ] help 1667 (topic 8)
    - [ ] slide-in, sounds, y 58
  - [x] UI 257 CDlgFacState (guild member info): the 4 audit mismatches fixed (opened by a member clicked on 133's member page; name, level, rank title, contribution; closed with 133; no longer closes 133)
  - [ ] UI 160 CDlgFriend (friends): 9 of 12 audit mismatches fixed
    - [x] GeneralBG frame and the DialogFriend title image
    - [x] the friends in yellow, online first
    - [x] Add picks the player to ask (its ask put to the other as 158's friend question, StrRes 100101)
    - [x] Enemy opens 269; mentor 312 and business 326 open (their dialogs are their own items)
    - [x] a name clicked opens 209; closing hides 209
    - [ ] whisper and leave-word row actions
    - [ ] help 1667
    - [ ] slide-in, sounds; list arrows [160-1]/[160-2]
    - [x] the Conquer front's MsgFriend (1019): the lists at login, made / lost / online / offline as they change, apply / accept / break / enemy delete from the client, a friend ask told
    - [ ] online / offline notices to friends (StrRes 100103/100104)
  - [ ] UI 269 CDlgEnemy (enemies): 6 of 8 audit mismatches fixed
    - [x] GeneralBG and the title image; enemies listed (a killer kept)
    - [x] Friend opens 160; mentor and business buttons
    - [x] a name clicked opens 270
    - [ ] help
    - [ ] slide-in
  - [ ] UI 209 CDlgFrdState (friend info): 5 of 7 audit mismatches fixed
    - [x] opened by a friend clicked; no longer closes 160
    - [x] name, level, profession, PK, guild, rank, spouse (what only a player here shows is empty elsewhere)
    - [x] Delete asks first (10007) and lets the friend go
    - [ ] face 1684 (the engine has no faces)
    - [ ] view equipment 1209; nobility / business images; the frame a pixel down
  - [ ] UI 270 CDlgEnmState (enemy info): 4 of 6 audit mismatches fixed
    - [x] opened by an enemy clicked; no longer closes 269
    - [x] its details as 209's
    - [x] Delete asks first (10400) and forgets the enemy
    - [ ] face 1427
    - [ ] view equipment 1429; nobility / business images
  - [ ] UI 138 CDlgSetup (options): 11 of 17 audit mismatches fixed
    - [x] reachable from the main bar's setup button; middle part, hiding the status and friends' pages
    - [x] music and sound sliders (a slider control kind: Slider0 along 80) bound to the player's volumes, live
    - [x] the music switch (off, or back to its level); window / full screen buttons set the window
    - [x] Return and close; Exit leaves the game; Hotkey opens 231, Other 255
    - [x] Dialog10 two pixels down; the spurious drafted 1075/1084-1086 gone
    - [ ] delete role (confirm, password)
    - [ ] shadow buttons 1072/1077, show-names 1083 and KO-count 1087 checks (engine options)
    - [ ] the resolution radios 1533/1534 (Picture100)
    - [ ] help 1667 (topic 19); user hotkey 45
  - [ ] UI 231 CDlgSetup_Hotkey (hotkeys): 5 of 8 audit mismatches fixed
    - [x] opened by setup's Hotkey
    - [x] the groups of actions and the picked group's actions (the engine's input actions), the picked action's keys
    - [x] Change waits for the new key (the engine's rebinding, shown at +246,+126); Revert sets every key back
    - [ ] TQ's own hotkey categories (StrRes 10300-10305, 11018) rather than the engine's action groups
    - [ ] Delete (clearing a key)
    - [ ] help 1667 (0x19)
  - [ ] UI 255 CDlgOther (furniture): 2 of 4 audit mismatches fixed
    - [x] opened by setup's Other; hides the status and friends' pages as it opens
    - [ ] 1694 delete furniture (the delete-NPC cursor; needs furniture in homes)
    - [ ] help 1667
  - [ ] UI 302 CDlgSetup_Graph (low-FPS restart prompt): draft only, 5 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 274 CDlgTalkAct (emotion icon picker): 5 of 6 audit mismatches fixed
    - [x] Dialog50 at (+3,-5); 32 of EmotionIco.ani's icons a page in eight columns, a frame each 300 ms (`animate=`); one clicked put in the chat line as `#NN` (88 letters at most); Last / Next pages; Close
    - [x] said lines draw their `#NN` emotions in place (CMyBitmap::ShowString: below EmotionIco.ini's count, a frame each 200 ms, a font and a half wide); the log's lines in their channels' colours
    - [ ] the emotion put at the caret rather than the line's end; help 1719; emotions in the talk bubble over a speaker
  - [x] UI 148 CDlgListBox (whisper-name history): the 3 audit mismatches fixed (opened by 145-1185, Dialog24 at -3,-4, names bound and picked); what is left is under 145
  - [ ] UI 139 CDlgChat (chat options: filters and colours): 9 of 11 audit mismatches fixed
    - [x] Dialog36 with Image36; a check per channel (G/B/D/H/S/Z, on at first) hiding its lines, SF whispers from friends only; each channel's colour (Image36N, talk 7, the rest 6) opening the eight swatches, one picked colouring its lines (`ChatFilter`, `ChatLook::palette`); Sift to 239; closes
    - [x] Record 1336 opens the chat log (218)
    - [ ] help 1667; the action channel's lines (the engine has none yet)
  - [ ] UI 239 CDlgBlack (blacklist): 7 of 8 audit mismatches fixed
    - [x] Dialog36 with Image37's two frames; the names listed, the name typed (15), Add asked first (StrRes 11017 in 158), Del the picked one; a blacklisted name's lines unheard; closes back to 139
    - [ ] help 1667
  - [ ] Message boards (packets spec 1111, B10): `gameplay.boards`, a guild's own board, posting from `board_min_level`, one post per author on the world state (ChatChannel::Board, Input::TakeDown, GM take-down); the Conquer front lists pages of 8 and reads posts (MsgMessageBoard 1111, MsgTalk 2201-2205)
    - [x] engine, UI and front
    - [ ] posts' text checked (CheckText) and the 2206 system board
  - [ ] UI 240 CDlgLevWord (message board): 8 of 10 audit mismatches fixed
    - [x] title Image371-373 by mode; tabs with frame 2 on the one chosen (`chosen=`); list, write and read modes; Send (StrRes 10362), Watch (10363 when none picked), Del, Up/Down/Refresh; OK posts (lines made one), Cancel Button7 / Button2; a row double-clicked whispers its author
    - [ ] the dim block behind the edit in write and read modes (ShowBlock 0x99000000), the list arrows [240-1]/[240-2], Help
  - [ ] UI 5326 CDlgNorMsg (chat-log line handles): missing, 3 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 218 CDlgLog (chat history viewer): the session's log
    - [x] the Win32 dialog's bitmaps as they are (`file=`): LogBk.bmp centred, Close, Delete (the channel shown) and Delete All, a tab per channel (Talk, Whisper, Guild, Friend, Team, All); the day shown; the lines said (`[Talk]Ann speaks to All: hi[h:m]`) on the edit's brush (`back=`), the newest last, the wheel going back (`tail=1`); opened by 139's Record
    - [ ] a log file a day (Record 1654 and its LogRecord setting, the date list's other days, LoadLog), the Win32 scroll bar, the chosen tab's look (its Click bitmaps are missing from 5065's data), Chat.dll's whisper/team windows choosing the channel
  - [ ] UI 2232 / 2233 / 2234 native private chat windows and tray: missing, 5 mismatches (docs/conquer_ui_audit.md)
  - [ ] Broadcasts (adapters spec §4, Pigeon.dll): `gameplay.broadcast` (Conquer: 5 CPs from level 50, 80 letters, 5 waiting each, a minute apart, 15 CPs to the front, 5 up 5 places); one queue on the world state that every map tells when due (ChatChannel::Broadcast); the Conquer front carries MsgPigeon 2050 / MsgPigeonInfo 2051 and MsgTalk 2500
    - [x] engine, UI and front
    - [ ] being banned from broadcasting (STR_PIGEON_FORBID), a log of lines told kept past 50
  - [ ] UI 294 CDlgBroadcast5065 (pigeon broadcast board): 7 of 9 audit mismatches fixed
    - [x] ad_MainBG, the four tabs (frame on the one chosen) and Pic_channel by mode, ten rows a page (place, name, line in red, time left), the hero's waiting lines' move buttons, another's name whispering it, the empty-page words (11027-11030), paging; opened from 295
    - [ ] the combined first page (6 waiting, 3 heard, inner arrows, 11114), tooltips (11036, the whole line), the help
  - [x] UI 296 CDlgPigeonPrompt5065 (broadcast prompts): below level 50 (11035, OK alone), the line asked (11043, cost 11024), moving forward (11032 / 11033); Dialog15, buttons by mode
  - [x] UI 293 CDlgOfflineMessage5065 (whisper to an offline friend): a whisper to an offline friend asks (StrRes 11022, Dialog15 at -5,-5) whether to leave it as words; OK leaves them, Cancel drops it
  - [ ] UI 229 CDlgEmbed (gem sockets): 5 of 13 audit mismatches fixed
    - [x] opened by a gem NPC (`gameplay.npc_windows`: an NPC kind opening a window in place of a dialogue)
    - [x] the bag opened with it
    - [x] embedding closes it; closing empties the workbench
    - [x] items put on its slots (pick and put) or by the bag's right button
    - [x] tooltips on its slots
    - [ ] only socketed gear on 1688, gems routed by type
    - [ ] sockets only where the gear has them, over Dialog30; gems already set shown
    - [ ] socket picked (ImageFocus); embedding only newly placed gems; Take Out from the picked socket, asked first (158 mode 17)
    - [ ] double-click to take an item back without shifting
    - [ ] help 1667; +N
  - [ ] UI 275 CDlgItemCompose5065 (+N composing): 4 of 12 audit mismatches fixed
    - [x] the bag opened with it; right part (closes the right owner)
    - [x] composing closes it; closing empties the workbench
    - [x] items put on its slots
    - [x] tooltips and quality lights on its slots
    - [ ] refused while trading
    - [ ] slots by kind: main equipment only, treasures after it (Image510), gems from +5 (Image511), routed by type
    - [ ] compose only with main and both treasures
    - [ ] help 1667; +N
  - [ ] UI 211 CDlgImprove (raise quality / level): 4 of 6 audit mismatches fixed
    - [x] opened by a forge NPC, with the bag
    - [x] the gear (1628) and the DragonBall or Meteor (1629) put on it
    - [x] OK uses the second on the first (`use_on:1:0`) and closes
    - [x] closing empties the workbench
    - [ ] 1628 only gear at full durability, 1629 only DragonBall / Meteor / MeteorTear
    - [ ] help 1667; double-click to take back
  - [ ] UI 278 CDlgOven (refine / fuse): 4 of 6 audit mismatches fixed
    - [x] opened by MsgAction 126 data 2 / 3 (`window=2,3`, its mode kept for the fuse) with the bag; Dialog54, up to 20 bag items in the 4x5 grid (`work.all`; one clicked comes out), the confirm check, the level while checked (10454), Refine needing the check (else 10452) fusing in the mode and closing
    - [ ] the probability (10453 from the client's fuse values), the add gates (10447, 10448, 10450, 10451, 10456, 10464), help
  - [ ] UI 105 CDlgEnchant: 3 of 4 audit mismatches fixed (unreachable in 5065, as in the client)
    - [x] the equipment and gem slots (bag items put on them), Enchant (combine 4, MsgItem ENCHANT) and close
    - [ ] the slots' gates (equipment sort 1-8, full durability, enchant < 255; gems only), help
  - [x] UI 298 CDlgBrokenRepair5065: the shop's Repair (152-1105) turns the next bag item clicked into a repair; a broken one asks first (`gameplay.broken_repair`: Conquer's 5 Meteors; 11053 with Confirm / Cancel, or 11052 with Close)
  - [x] Faces: `PlayerLook::face`, `gameplay.faces` by body and `face_cost` (Conquer: 102 male faces from 1, 90 female from 201, 500 silver, paid even for a wrong face as TQ's MsgAction 142); team faces and the Conquer look face use it
  - [ ] UI 212 CDlgNpcFace (change avatar): 5 of 6 audit mismatches fixed
    - [x] opened by face NPCs; Dialog27, six faces of the hero's body (PlayerFace), the clicked one framed (ImageFocus), Up / Down by six, OK takes it and closes
    - [ ] the slide-in
  - [ ] Booths: players sell from booth spots (engine/game booths.rs, `gameplay.booth_spots`; Conquer's ShopFlag NPCs, kind 16)
    - [x] a free spot offers a booth (StrRes 10364, DlgConfirm mode 13); its owner stands beside it; wares from the bag at a price in money or another currency, up to 20; others who come to the spot see them and buy (the item and the money change hands); the booth closes when its owner closes it, walks, jumps or leaves
    - [x] TQ's placement: the owner two cells out from the spot along its facing, the booth one more; wares drop off when their items leave the bag; the owner told who paid (STR_BOOTH_BUY_sds)
    - [x] the Conquer front: MsgAction 111/114/115, MsgItem 21/22/23/24/29, MsgItemInfoEx 1108, the booth NPC (kind 14, look 400 + facing) shown and gone, the cry on MsgTalk 2104 (docs/conquer5065_protocol.md)
    - [ ] the engine's own client draws no booth carpet and the owner doesn't sit; the cry isn't shown to others there
    - [ ] the front's booth flow checked with the live Conquer.exe
    - [ ] the lowest price (item value / 5, STR_BOOTH_LOW_PRICE) and the owner's 1e9 money limit
    - [ ] items locked, suspicious or protected refused (MyShellDlg.cpp 4421-4461); a CP ware asks first (BOOTH_EMONEY_BUY_CONFIRM_5065)
  - [ ] UI 233 CDlgMpcBooth (own booth): 9 of 11 audit mismatches fixed
    - [x] shown with the bag while the hero keeps a booth; Dialog34, six slots a page (Up/Down), each item (tooltip, glow), its price with commas and its Dialog56/57 coin; GetBack under filled slots; Change never drawn (DlgMpcBooth.cpp 252-347)
    - [x] a bag item put on a slot asks its price in silver or CPs (DlgConfirm mode 10: Dialog56/57, edits 1666/1667)
    - [x] Hawk opens 235; both close buttons close the booth, hiding the cry (`hides=` on a `when` dialog)
    - [ ] Help (1667) once the help dialog lands; slide-in and sounds
  - [ ] UI 234 CDlgNpcBooth (another player's booth): 7 of 8 audit mismatches fixed
    - [x] shown when the hero comes to another's booth, from its first page: items, prices, coins, a Buy under each, paging; closing leaves it (DlgNpcBooth.cpp 213-522)
    - [ ] Help (1667) and slide-in
  - [ ] UI 235 CDlgHawk (booth cry-out): 2 of 3 audit mismatches fixed
    - [x] opened by the booth's Hawk, a 64-letter edit, Agree sets the cry and closes (DlgHawk.cpp 98-167)
    - [ ] the text filtered and the edit cleared on show
  - [ ] Leave words (social spec §5): words for an offline friend wait on the world state and come at its next login (ChatChannel::LeaveWord, one a minute, STR_MSG_SENT / STR_MSG_FAILED); the Conquer front carries them on MsgTalk 2110 and holds them until the client has loaded its map
    - [x] engine, UI and front
    - [ ] trade partners may get words too (no trade partners yet), words kept 31 days, the received words logged to disk as the client's chat log does
  - [ ] UI 232 CDlgMsgSend (leave a word): 5 of 6 audit mismatches fixed
    - [x] opened by double-clicking an offline friend (CMyList OnLButtonDblClk) and by Reply in 238; the friend's name, a 256-letter multi-line edit (`wrap=1` edits), Send leaves the words and closes, View All only with words received
    - [ ] Help (1667) once the help dialog lands
  - [ ] UI 238 CDlgMsgReceive (received words): 5 of 6 audit mismatches fixed
    - [x] who left the words and what; Previous / Next enabled as there are more; Reply only to an offline friend; Delete All
    - [ ] Help (1667)
  - [ ] UI 290 CDlgItemPurchase5065 (+292 CDlgBuyMessage5065) (CP buy amount): draft only, 10 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 330 CDlgEmoneyWithdraw5065 (CP withdraw button): draft only, 2 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 277 CDlgPortableChest5065 (portable chest): missing, 6 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 329 CDlgItemLock5065 (item lock): draft only, 7 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 258 CDlgNpcEquip (another player's equipment): draft only, 10 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 241 CDlgEffigy (statue placement): draft only, 9 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 242 CDlgLay (place an NPC): draft only, 6 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 262 CDlgDice: draft only, 9 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 267 CDlgDiceList: draft only, 3 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 284 CDlgOfflineTrainingResult: draft only, 8 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 285 CDlgOfflineTrainingConfirm: draft only, 5 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 303 CDlgAutoRun5065 (auto-path destinations): draft only, 7 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 334 CDlgFurnishment5065 (item-bind list): draft only, 5 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 380 CDlgQuestList5065 / 381 CDlgQuestTrack5065: draft only (non-native additions), 7 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 215 CDlgHelp: draft only, 2 mismatches (docs/conquer_ui_audit.md)
  - [ ] Mentors (adapters spec §5, Guide.dll): `gameplay.mentor` (Conquer: level 70, gaps 30/20/10 by rebirths, cq_tutor_type students, cq_student_type shares, 50,000 silver and 72 hours to leave); offer / ask / answer, leave, expel; a student's exp gives its mentor and its mentor's mentor time at its level (flushed each minute), claimed as experience (TQ's AwardTimeExp, `Rules::level_time`), blessed hours or +1 stones; the Conquer front carries MsgMentor 2065, the relations (2066) and the claims (2067)
    - [x] engine, UI and front for the above
    - [ ] blessed hours from prayer items and item points from +N drops (no hooks yet), battle power shares (no battle power yet), a mentor taking back a student who left (actions 5/6), the timers and notices of actions 10-23
  - [ ] UI 312 CDlgSchoolList (mentor): 7 of 9 audit mismatches fixed
    - [x] Dialogteacher, the mentor's name and leave days (100187), detail button (313), students in yellow with detail buttons (314), Follow / Betray shown by whether there is a mentor (100188 asked first), Take In under the limit, Dismiss (100254), contribution (319), friend / enemy / business pages; closing hides 313, 314 and 319
    - [ ] the mentor's face, list clicks opening the grand-student list 315
  - [ ] UI 313 / 314 / 316 mentor state pages, 315 grand-student list: 313 and 314 done (DialogStateBg, title, name, spouse, level, profession, PK, guild, rank; Betray / Dismiss)
    - [ ] 313's follow date and shared battle power, 314's contributions, 315 and 316
  - [ ] UI 317 CDlgMentorNotice5065 / 318 CDlgMentorConfirm5065: 318 done (Dialog15, StrRes 100195 / 100196 with the asker's name, OK / Cancel)
    - [ ] 317's notices (actions 10-17)
  - [ ] UI 319 / 320 / 322 / 323 / 324 mentor contribution pages: 319 done (dssy-lqgxBG, amounts as exp balls / hours / stones, claim buttons enabled with something to claim, the top giver of each kind); opened from 312 and the 325 HUD button
    - [ ] 320 (online page), 322-324 (rankings, history, second generation)
  - [ ] Trade partners (social spec §3): asked and asked back, 20 at most, on trial for `partner_probation_secs` (Conquer's 3 days), ended by either side (an offline one told by leave words), leave words to partners; the Conquer front lists them with their trial minutes and online state (MsgBusiness 2046) and takes asks, refusals and ends
    - [x] engine, UI and front
    - [ ] what a formal partnership grants: trading no-trade items, a spouse's NPC storage (needs item monopoly flags and NPC-owned storages)
    - [ ] refusing an ask tells the asker from the game UI (the prompt's No sends nothing yet)
  - [ ] UI 326 CDlgBusiness (trade partners): 6 of 7 audit mismatches fixed
    - [x] GeneralBG and its title, the partners in yellow (online first), assessment icons by hours of trial left, a row opening 327 and double-click whisper / leave words, Add under 20 (StrRes 11145), friend / enemy / mentor pages
    - [ ] Help, slide-in, the hours tip over an assessment icon (StrRes 11147), rows 11-20 scrolled
  - [ ] UI 327 CDlgBusinessState: 3 of 4 audit mismatches fixed
    - [x] GeneralShuxingBG1, the partner's name, level, profession, PK, guild, rank, spouse and assessment icon; Delete asks first (StrRes 11146)
    - [ ] the partner's face (the engine has no faces), placed beside 326 (205 < 242)
  - [ ] UI 331 CDlgBusinessTip5065: draft only, 4 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 332 CDlgBusinessConfirm: draft only, 4 mismatches (docs/conquer_ui_audit.md)
  - [ ] UI 333 CDlgBusinessShare5065: draft only, 3 mismatches (docs/conquer_ui_audit.md)
  - [ ] Nobility (adapters spec §2, Peerage.dll): `gameplay.nobility` (Conquer: from level 70, 3,000,000 at least or a CP each 50,000, a board of 60: 3 Kings, to 15 Princes, to 50 Dukes, then Earls / Barons / Knights by amount); donations on the world state, ranks by place then amount, a rise announced (En_Res notices); the Conquer front carries MsgPeerage 2064 (donation result, board pages, the hero's standing, a rank's price)
    - [x] engine, UI and front
    - [ ] Queens and female notices (the engine keeps no sex), the battle power a rank gives (no battle power yet), the rank shown on others
  - [ ] UI 307 CDlgNobilityMain: 5 of 6 audit mismatches fixed
    - [x] checont_checontribute at (-5,-5), the amount and Contribute (in CPs asked first, 308), the six rank buttons (frame 2 on the one asked about, disabled at or below the hero's rank) and what that rank needs (11094); closing hides 310; opened by the hero's rank icon from level 70
    - [ ] the King / Queen art by sex, help
  - [x] UI 308 CDlgNobilityConfirm: Dialog15, 11090 with the silver and CPs, OK gives, ReInput goes back to the amount, Close drops it
  - [ ] UI 309 CDlgNobilityRanking: 4 of 5 audit mismatches fixed
    - [x] checont_exploitbead, ten places a page (place, icon, title, name, donation with commas), the page and its arrows, the donation block from level 70; opened by the rank icon below level 70 and 310's More
    - [ ] the eleventh row, King art by sex
  - [x] UI 310 CDlgNobilityCompact: che_checontribute2 at (-5,-5), the top five, More opens 309
  - [ ] The nobility status icon (StatusTips5065.cpp 595-604): stood in for by an always-shown rank icon at the top left; its tip (11095) and place among the status tips
  - [x] UI engine: names files split `key=text` at the first `=` or `,` (StrRes texts with commas showed nothing); `fill_in` fills printf blanks (`%d`, `%u`, `%0.2f`), several in turn (`a||b`); TQ's written `
` breaks lines; `chosen=` tab frames; `double=` list actions; `wrap=1` multi-line edits
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
  - [x] Teams: the money and item pick-up locks (10 to 13): a killer's loot is its own for 30 s, its spouse's and its team's as the team shares (money unless kept, items when shared, never DragonBalls or Meteors), TQ's 20% team money cut (`[gameplay]` loot settings)
  - [x] Guilds shown: each player's guild and rank in MsgPlayer, the hero's guild
        (MsgSyndicateAttributeInfo 1106), guild names (MsgName 3, asked by MsgSyndicate 6)
  - [x] Guild actions (engine `GuildAction`, MsgSyndicate 1, 2, 3, 4, 7 to 11): the
        leader invites, kicks and names allies and enemies; members join, leave and
        donate; the hero's guild info follows its fund and members
  - [x] The guild member list (MsgName 11: pages of 10, name, level and online, by rank and level; levels kept on the world state) and applying to a guild's leader or deputy (MsgSyndicate 1, `GuildAction::Apply`; deputies of rank 90 invite; TQ's 100,000 fund and 800 members)
  - [ ] Each of the above checked with the real Conquer.exe

## M13: Ship
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

## M14: Any game of the kind (Eudemons-like or the user's own)
- [ ] Social and companion systems as engine modules a game switches on and shapes with data:
      teams, trade, guilds, friends, mail, pets and companions (summon, fight, level, evolve,
      equip), mounts, crafting, events and schedules
- [ ] Prove it: a second, small sample game made only in the Studio from its own art (no TQ
      files), with pets
- [ ] Easy for indies: New Project templates (Conquer-like, Eudemons-like, empty), the sample
      game, a getting-started guide in the Studio, tooltips, errors that say how to fix them