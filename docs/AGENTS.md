# Working on a C3Engine project as an agent

`c3-mcp` serves a project's tools over MCP (stdio). Every change goes through the same typed,
undoable commands the Studio's own tools use (`c3-edit`), so nothing an agent does is out of
reach of Undo, and nothing reaches the files until `save`.

```
c3-mcp <project folder>            # headless: the project on its own
c3-mcp --live [127.0.0.1:7390]     # drive the open Studio (Window > Live link)
```

Live, each change shows in the Studio as it happens and is undone there with Ctrl+Z; the
Studio opens whatever a tool works on (the map, the table, the dialogue).

## First calls
1. `project_overview`: the game, maps (key, ids), tables, dialogues, scripts, blueprints.
2. `read_docs agents` (this page), `read_docs architecture` for how the engine fits together.
3. `command_schemas` for the exact JSON of a kind of command before sending it.

## Documents
Tools name documents as `map:<key>`, `table:<name>`, `settings`, `dialogue:<name>`,
`script:<name>`, `blueprint:<name>` and `assets` (for `undo` and `redo`, and in `save`'s
answer). A map key is a project scene's name (`Meadow`) or a content map path
(`map/map/newplain.DMap`).

## Common tasks
- **Place an NPC:** `query_map` `{"query":"summary"}` and `{"query":"npcs"}` for free ids and
  cells, then `edit_map` with `{"op":"add_npc", ...}` (see `command_schemas map`). Its
  `script` names a dialogue (`dialogue/<name>`), a script or a blueprint's graph
  (`graphs/<name>`).
- **Write what an NPC says:** `create` a dialogue, then `edit_dialogue` with `add_node` and
  `set_start`; `read_dialogue` lists what is wrong (answers leading nowhere).
- **A game rule:** a blueprint (`create` kind blueprint, `node_library` for the nodes,
  `write_blueprint`) or a Rhai script (`script_reference`, `write_script`). Event hooks are
  functions such as `on_login`, `on_kill(monster)`, `on_enter_map(map)`.
- **Data:** `read_table` (columns say what refers to what), `edit_table`, `create_table`.
- **Settings:** `read_settings` with a path (`gameplay.new_character`), `edit_settings`.

## Checking work
`save`, then `playtest`: a game-master player on a map doing steps (`{"Walk":[x,y]}`,
`{"Talk":npc}`, `{"Answer":0}`, `{"Say":"/level 10"}`, `{"Wait":5000}`); it answers each
step's events and open dialogue page, and the player at the end (cell, level, money, bag,
flags). Problems the game found in the data come back with it.
