# Vision

C3Engine is a game engine and studio written in Rust, modelled on TQ Digital's
"Conquer III (C3)" engine, which powered Conquer Online, Eudemons and others.

**Goal:** TQ's C3 engine with a Unity/UE5-style editor and AI built in. One person can make a
complete online game like Conquer (client, server, content and rules) entirely in the Studio,
controlling everything visually, as easily as possible, without touching engine code.
Conquer Online 5065 is the first game project and the proving ground, never part of the engine.
Beyond Conquer, the engine must be able to make any game of this kind (Eudemons-like with pets,
or an indie's own design), built from scratch in the Studio rather than imported, with
everything a shipped game has (cursors, camera, settings, UI, login screens, sounds), all set
from the Studio the way Unity's Project Settings work.

Scope is the kind of games C3 made: 2D/2.5D isometric online RPGs (tiled maps, 3D
characters, effects, cell-based movement, client/server play). Full 3D is not a goal for now.

## A complete game, all from the Studio
- **World:** maps, objects, effects, portals, regions, music, NPCs, monster spawns.
- **Characters:** looks, equipment, actions, animations, effects.
- **Gameplay:** stats, levels, combat and skills, items and inventory, drops, shops, trade,
  quests and NPC dialogue, teams, guilds, chat. Defined as data tables plus scripts.
- **Game UI:** designed in a UI editor (windows, HUD, dialogs), not hard-coded.
- **Server:** the engine has its own game server, built from the same project (maps, tables,
  scripts), with accounts and a database. Redux stays usable for Conquer 5065 meanwhile.
- **Ship:** build the client and server from the project in one step.

## The Studio, Unity/UE5-style
Native (egui), laid out and styled like Unity's editor: dockable Hierarchy, Scene, Game,
Inspector, Project and Console panels, a menu bar and a Play/Pause/Step toolbar.
- **Projects:** each game is a project folder with its own assets, tables and scripts. Like
  Unity, files put in `assets/` are used as they are (importers per file type, no conversion);
  Conquer is imported by copying the 5065 client's content files into one.
- **Asset browser** with live previews; **scene editor** with an inspector for the selection;
  **table editors** for items, skills, monsters; **script editor**; **UI editor**.
- **Play mode:** press Play to run the game (client and a local server) inside the Studio.
- **Easiest possible:** sensible defaults, templates, previews everywhere, undo for everything,
  clear errors that say how to fix them.

## AI built in
- **Studio assistant:** a chat panel (Claude) that creates and edits game content on request
  ("make a fire dungeon map", "add a level-30 boss with these drops", "write this quest"). It
  works through the same editor commands as a person: every change is previewed, undoable and
  shown in the history.
- **Driven by AI agents:** the Studio exposes its commands as an API / MCP server, so tools
  like Claude Code can open projects, query them and edit them from outside.
- **Foundation:** every edit in the Studio is a typed, serializable command with undo. The UI,
  the assistant and external agents all use that one command layer, so AI can do anything a
  person can, and nothing a person couldn't undo.

## Principles
1. **Engine is game-agnostic.** Nothing under `engine/` or `studio/` may reference Conquer.
   Game-specific code lives in `games/<name>/`, and over time moves into project scripts and data.
2. **Native formats + TQ importers.** The engine has its own clean asset formats.
   TQ formats (C3, DMap, PUL, ANI, WDF, DNP, ini/dat) are import/export plugins in `formats/c3/`,
   so legacy content loads and new content can be exported back to the classic client.
3. **Data-driven.** Items, skills, monsters, UI layouts are tables + scripts edited in Studio.
4. **Studio = the engine in editor mode.** What you edit is exactly what players see.
5. **One command layer.** All edits (UI, assistant, agents) go through typed commands with undo.
6. **Round-trip safety.** Every format parser must load → save byte-identical on real files.
7. **No encryption** of the engine's own files (see DECISIONS.md).

## Non-goals (for now)
- Full 3D games.
- Mobile / web targets.
- Building our own physics engine. Use one if a game needs it.

**Final success criterion (2026-10-18):** a complete Conquer Online game exactly as client 5065, losing nothing, made with the engine and its Studio.
