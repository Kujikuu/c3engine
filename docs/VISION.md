# Vision

C3Engine is a game engine and studio written in Rust, modelled on TQ Digital's
"Conquer III (C3)" engine, which powered Conquer Online, Eudemons and others.

**Goal:** TQ's C3 engine with a Unity-style editor. Anyone can make games of the kind C3
made (2D/2.5D isometric online RPGs: tiled maps, 3D characters, effects, cell-based
movement, client/server play) entirely in the Studio, without touching engine code.
Conquer Online (5065, served by Redux) is the first game and the proving ground, but it is a
game project, never part of the engine.

Full 3D games (perspective cameras, physics, open worlds) are out of scope for now. The
renderer and scene model must not rule them out, so they can come later.

## The Studio, Unity-style
- **Projects:** create or open a game project folder; it holds the game's own assets,
  tables and scripts. Conquer is one project, imported from the 5065 client.
- **Asset browser:** every model, effect, sprite, map, sound and table, with live previews.
- **Scene editor:** maps with their ground, objects, effects, NPCs, monsters and portals,
  placed and edited on engine types; an inspector for whatever is selected.
- **Play mode:** press Play to run the game inside the Studio, and stop to return to editing.
- **Game logic in the project:** scripts and data tables, not engine code.
- **Build:** package a playable client (and server data) from the project.

## Principles
1. **Engine is game-agnostic.** Nothing under `engine/` or `studio/` may reference Conquer.
   Game-specific code lives in `games/<name>/`, and over time moves into project scripts and data.
2. **Native formats + TQ importers.** The engine has its own clean asset formats.
   TQ formats (C3, DMap, PUL, ANI, WDF, DNP, ini/dat) are import/export plugins in `formats/c3/`,
   so legacy content loads and new content can be exported back to the classic client.
3. **Data-driven.** Items, skills, monsters, UI layouts are tables + scripts edited in Studio.
4. **Studio = the engine in editor mode.** What you edit is exactly what players see.
5. **Round-trip safety.** Every format parser must load → save byte-identical on real files.
6. **No encryption** of the engine's own files (see DECISIONS.md).

## Non-goals (for now)
- Full 3D games (see above).
- Mobile / web targets.
- Building our own physics engine. Use one if a game needs it.
