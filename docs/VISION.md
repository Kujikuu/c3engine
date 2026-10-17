# Vision

C3Engine is a general-purpose game engine and studio written in Rust, modelled on
TQ Digital's "Conquer III (C3)" engine, which powered Conquer Online, Eudemons and others.

**Goal:** create *any* game with it. Conquer Online (5065, served by Redux) is the
first game and the proving ground, but it is a game module, never part of the engine.

## Principles
1. **Engine is game-agnostic.** Nothing under `engine/` or `studio/` may reference Conquer.
   Game-specific code lives in `games/<name>/`.
2. **Native formats + TQ importers.** The engine has its own clean asset formats.
   TQ formats (C3, DMap, PUL, ANI, WDF, DNP, ini/dat) are import/export plugins in `formats/c3/`,
   so legacy content loads and new content can be exported back to the classic client.
3. **Data-driven.** Items, skills, monsters, UI layouts are tables + scripts edited in Studio.
4. **Studio = the engine in editor mode.** What you edit is exactly what players see.
5. **Project system.** Studio opens a game project folder, edits it, and packages a client.
6. **Round-trip safety.** Every format parser must load → save byte-identical on real files.

## Non-goals (for now)
- Mobile / web targets.
- Building our own physics engine. Use one if a game needs it.
