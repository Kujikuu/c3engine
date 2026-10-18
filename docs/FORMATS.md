# TQ Formats Reference

Living notes for each format we reverse/implement. Fill these in as parsers land.
Sources: `D:\Conquer2\Tools\eosource\client5065` (assets), `...\cosource` (client source),
`...\tq_re` (Ghidra workspace), `...\Original_TQ_Conquer_Online_Source`.

| Format | Purpose | Status | Notes |
|---|---|---|---|
| WDF | packed archive | **done** | `wdf.rs` |
| DNP | packed archive | **n/a** | no `.dnp` files exist in the 5065 client; not implemented |
| DDS/TGA | textures | **done** | `texture.rs`; client uses DXT1 (data.wdf) + DXT3, no mips |
| MSK | 1-bit hit masks | **done** | `mask.rs`; 173/177 paired + round-trip (4 lack images, 1 truncated in client) |
| ANI | animation/frame index (text) | **done** | `ani.rs` |
| DMap | map cells, heights, portals, scene refs | **done** | `dmap.rs` |
| PUL | puzzle ground tiles | **done** | `pul.rs` |
| C3 container | chunked model/effect file | **done** | `c3/mod.rs`; all 4,516 files byte-exact |
| C3 PHY | mesh + skinning | **done** | `c3/phy.rs`; 7,308 chunks byte-exact; → engine `SkinnedMesh` |
| C3 MOTI | skeletal motion keyframes | **done** | `c3/motion.rs`; 11,093 chunks byte-exact; → engine `Clip` |
| C3 SHAP/SMOT/PTCL/CAME | trails, particles, camera | **done** | `c3/effect.rs`; every chunk byte-exact. PTCX/PTC3 absent in 5065 (raw) |
| ini tables | model/texture/motion/equipment wiring | **done** | `ini.rs` (Ini, PathTable, PartTable) |
| .scene / .Part | terrain objects (binary / editor text) | **done** | `scene.rs`; 194 + 126 files; 9,884 DMap refs resolve |
| GameMap.dat | map id → DMap catalog | **done** | `gamemap.rs`; 153 maps byte-exact |
| .TME | terrain magic effect groups | **done** | `tme.rs`; 88 files byte-exact |
| .rgn | GUI window regions (RGNDATA) | **done** | `rgn.rs`; 5 files byte-exact |
| text .dat | Play.dat (+6 shift), Shop/Server (plain ini) | **done** | `textdat.rs` + `ini.rs` |
| game .dat tables | itemtype, Monster, MagicType, Action, MapDestination, levexp, AutoAllot, Tips, UserHelpInfo, LevelExp | **done** | `cipher.rs` + `dat.rs`; all byte-exact (encrypted ones re-encrypt exactly) |

## Engine formats (native, unencrypted)

### c3project.toml (project manifest)
TOML in the project folder root, next to `assets/`, `maps/`, `tables/`, `scripts/`.
```toml
name = "Conquer"
engine = "0.1.0"            # engine version last saved with

[[source]]                  # optional external content, read until imported
kind = "tq-client"
game = "conquer-5065"
path = 'D:\Conquer2\Tools\eosource\client5065'
```
Recent projects: `<config dir>/C3Engine/recent.toml`, `[[project]]` entries (name, path, opened).

### Editor commands and queries (c3-edit JSON)
The AI assistant, MCP agents and `c3-studio --exec` use this JSON (schemas:
`c3_edit::scene_command_schema()` / `scene_query_schema()`). Cells are `[x, y]`.
```json
[
  {"op": "set_blocked", "area": {"shape": "rect", "min": [250, 600], "max": [400, 750]}, "blocked": true},
  {"op": "set_blocked", "area": {"shape": "square", "center": [10, 10], "radius": 2}, "blocked": false},
  {"op": "set_blocked", "area": {"shape": "cells", "cells": [[1, 2], [3, 4]]}, "blocked": true},
  {"op": "add_portal", "cell": [330, 680]},
  {"op": "move_portal", "id": 0, "to": [331, 680]},
  {"op": "remove_portal", "id": 0}
]
```
Queries: `{"query": "summary"}`, `{"query": "cell", "cell": [x, y]}`, `{"query": "portals"}`,
`{"query": "walkability", "min": [x, y], "max": [x, y]}` (rows of `.` walkable, `#` blocked,
`P` portal; at most 128×128).

## Per-format sections
(Add `## <FORMAT>` with header layout, field table, quirks, and test files.)

## Path hashing
Entries are keyed by `legacy_string_id(normalize(path))` (lowercase ASCII, `\` → `/`).
Port of `C3LegacyStringId` in `cosource/C3_CORE_DLL/include/c3_hash.h`. Lookup: try the
archive named by the first path segment (`c3/...` → `c3.wdf`), then fall back to all archives.

## WDF
Header `{ magic 'PFDW' (u32 0x57444650), count i32, index_offset u32 }`, then blobs, then
`count × { uid, offset, size, space }` (all u32, sorted by uid). `space` = slack after blob.
Source: `C3_CORE_DLL/source/c3_datafile.cpp`, `c3_archive.cpp`.

## DMap
Source: `3dgamemap/3DGameMap.cpp` `LoadDataMap`/`SaveDataMap`, plus layer loaders.
- `version u32 (1004)`, `data u32`, `puzzle_file char[260]`, `width u32`, `height u32`
- Per row: `width × {mask u16, terrain u16, altitude i16}`, then `check u32` where
  `check = Σx mask*(terrain+y+1) + (altitude+2)*(x+1+terrain)` (wrapping i32)
- Passages: `n i32`, `n × {x, y, index}` (i32)
- Interactive layer: `n i32`, each `type i32` +
  - 4 Cover: `ani char[260]`, `title char[128]`, `cell 2×i32`, `base 2×u32`, `offset 2×i32`, `interval u32`
  - 1 Terrain: `file char[260]`, `cell 2×i32` (parts live in the referenced `.Scene` file)
  - 15 Sound: `file char[260]`, `world 2×i32`, `range i32`, `volume i32`
  - 10 Effect: `index char[64]`, `world 2×i32`
  - 19 EffectNew: Effect + 6×f32 (vertical twice, horizontal, scale xyz)
- Scene layers: `n i32`, each `{layer_index i32, type i32 (4), move_rate 2×i32, count i32, objs}`
  - 3 Scene: `ani char[260]`, `title char[128]`, `cell 2×i32`, `offset 2×i32`, `interval u32`, `show_way u32`
  - 8 Puzzle: `file char[260]`
- Fixed strings keep raw bytes after the NUL (`FixedStr`) so writes stay byte-identical.

## PUL (puzzle ground)
Source: `3dgamemap/PuzzleBmp.cpp` `CPuzzleBmp::Load`/`Save`.
- `header char[8]`: `"PUZZLE"` (v1) or `"PUZZLE2"` (v2); `ani_file char[256]`
- `width u32`, `height u32` (in grids), then `width*height × u16` tile indices, row-major
- v2 only: `roll_speed_x i32`, `roll_speed_y i32` (scrolling background layers)
- Tile `n` renders ANI section `Puzzle<n>` from `ani_file`; `0xFFFF` = empty. Grid = 256 px.
- 142 of 143 client files use the `.pul` extension and one uses `.Pul`, so match it case-insensitively.

## ANI (frame index)
Source: `3DBaseCode/Ani.cpp`. GBK text, CRLF.
```
[Title]
FrameAmount=N
Frame0=data/...dds
```
- `//` lines are comments. `FrameAmount` is taken modulo `_MAX_FRAME`.
- Title lookup is exact; the first matching section wins.
- Paths are packed-archive paths (resolve with `wdf::Wdf::find`) or loose files.
- We round-trip semantically (comments are dropped); output is canonical CRLF.

## C3 (models, motions, effects)
Sources: `3DBaseCode/3DEffect.cpp` (chunk walk), `C3_CORE_DLL/source/c3_phy.cpp`
(`Phy_LoadVersioned`, `MotionLoadBuffer5065`, `PhySeedBindPose5065`, `Phy_Calculate`).

Container: `header char[16]` ("MAXFILE C3 00001") + chunks `{id char[4], size u32, body}`.
Seen in 5065: `PHY ` 7,308 · `MOTI` 11,093 · `CAME` 1,717 · `SHAP`/`SMOT` 543 · `PTCL` 458.
Mesh n in a file is animated by motion n. Character animations are separate files (`ini/3dmotion.ini`).

**PHY** (`PHY `/`PHY2` have 3 legacy morph positions after `pos`; `PHY2`/`PHY3` store a normal):
name (u32 len + bytes) · blend u32 · opaque_verts u32 · alpha_verts u32 ·
vertex {pos f32×3, [morph f32×9], uv f32×2, color u32, bones u32×2, weights f32×2, [normal f32×3]} ·
opaque_tris u32 · alpha_tris u32 · indices u16 · texture (u32 len + bytes) · bbox f32×6 ·
bind matrix f32×16 · tex_row u32 · 3 key lists (u32 n + n×{frame i32, f32, bool u32, i32}:
alpha, draw, texture) · optional `STEP` f32×2 · optional `2SID` · optional `BILB`|`BIB2`.
**At load the client multiplies every position by the bind matrix**; motions animate those positions.

**MOTI**: bones u32 · frames u32 · one of
`KKEY` (n, n×{frame u32, bones×mat4}) · `XKEY` (n, n×{frame u16, bones×3×4 floats; 4th column 0,0,0,1}) ·
`ZKEY` (n, n×{frame u16, bones×{quat xyzw, translation xyz}}) · untagged (frames×bones mat4, bone-major) ·
morphs u32 · frames×morphs f32. `XKEY` isn't in the FILE* loader, only in the package-buffer loader.

**Sampling / skinning (classic):** bone matrix = linear blend of the surrounding key matrices
(frames wrap at `frames`); vertex = pos × Σ wᵢ·boneᵢ (row vectors), bone 0 if no weights apply.
Matrices are D3D row-major/row-vector = same memory as glam column-major/column-vector.
Models are **−Z up**.

## Terrain objects (.scene / .Part)
Source: `3dgamemap/2DMapTerrainObj.cpp`, `2DMapTerrainObjPart.cpp`. DMap `Terrain` entries point at `.scene`.
`.scene`: `parts i32` × { `ani char[256]`, `title char[64]`, `offset i32×2`, `interval u32`, `base u32×2 (w,h)`,
`thick i32`, `scene_offset i32×2`, `height i32`, `h×w × {mask u32, terrain i32, altitude i32}` }.
`.Part` is the editor text form (`AniFile=`, `AniTitle=`, `OffsetX/Y=`, `AniInterval=`, `Width/Height=`, `Thick=`,
`Cell[x,y]={mask,terrain,alt}`), without scene offset/height.

## GameMap.dat
`count u32` × { `id u32`, `path_len u32`, `path`, `puzzle_grid_size u32` } (`GameMapCatalog.cpp`).
5065 lists 153 maps; `kunlun1..9.DMap` are referenced but absent from the client.

## C3 effect chunks
SHAP: name · lines{points × f32×3} · texture · segments u32. SMOT: frames × mat4.
PTCL: name · texture · rows · capacity · frames × {n, if n>0: pos×n, age×n, size×n, mat4}.
CAME: name · fov f32 (ignored) · n · from×n · to×n. (Strings are u32 length + bytes.)

Playback (`C3DEffect::Draw`): per layer, trails, then particles, then meshes. Mesh n pairs with
MOTI n, SHAP n with SMOT n; only SHAP line 0, points 0–1 form the trail edge; segments ×5, max 800.
Particles are view-aligned quads (half-size × average matrix scale), atlas cell = age·rows².

## 3DEffect.ini
`[name]` · `Amount` · per part n: `EffectIdn` (3DEffectObj.ini) `TextureIdn` (3dtexture.ini)
`ASBn`/`ADBn` (D3D blend, default 5/6) · `Delay` `LoopTime` `FrameInterval` `LoopInterval` (ms)
· `OffsetX/Y/Z` · optional `ShapeAir`. A `+owner` suffix on the name is ignored. Frames per loop =
first part's longest element. Frame = (t−delay) mod (interval·frames + gap) / interval; done after
LoopTime loops. Map effects (DMap type 10/19) loop forever; type 19 adds angles (the first read
twice: vertical, vertical, horizontal; radians) and a scale.

## MSK / TME / RGN
- MSK (`AlphaMask.cpp`): headerless bits for the same-named DDS/TGA; bit `x+y·w` → byte `/8`, bit `%8` (LSB
  first), set = opaque. The client reads `w·h/8+1` bytes.
- TME (`TerrainEffectManager.cpp`): `count u32` × {`effect char[128]`, `delay u32`, `random_delay u32`, `offset i32×2`};
  the client negates `offset.x` on load. Loaded from `ini/tme/<name>`.
- RGN (`Game/GameDlg.cpp`): `payload u32` + Win32 `RGNDATAHEADER` (32 bytes, type 1) + `RECT`s.

## Text .dat
`ini/Play.dat`: each byte stored +6 (`textdat::PLAY_DAT_SHIFT`). `Shop.dat`, `Server.dat`, `AutoPatch.dat`,
`version.dat`, `res.dat` are plain text.

## Game .dat tables
**XorRot cipher** (`cipher.rs`): key[128] = MSVC `rand()` after `srand(seed)` (`state = state*214013+2531011; (state>>16)&0x7FFF`);
byte i: `ror8(data[i] ^ key[i%128], i%8)`. Seed 0x2537 for itemtype/Monster/MapDestination/UserHelpInfo, 0x4D2 for levexp.
Decrypted payloads are CRLF text:
- `itemtype.dat`: `Amount=N` + N rows of 39–40 whitespace tokens (`~` = space, description last). 6,865 items.
- `Monster.dat`: ini sections `[Name]` with fixed key order (SizeAdd … StcType). 418 monsters.
- `MapDestination.dat`: ini sections (id, mapid, npcid, parentid, radius, posx, posy, mark, title). 174 entries.
- `UserHelpInfo.dat`: ini, `[Header] HelpInfoAmount=867`, `[HelpInfoN]`. (`UserHelpInfo.ini.dat` is an unused older copy.)
- `levexp.dat`: lines `level exp minutes` (levels 1–150).

Binary (`dat.rs`):
- `MagicType.dat`: `count · keys(id*10+level)[count] · MagicTypeInfo[count]` (0x630 bytes, `3drole/MagicData.h`). 657 records.
- `Action.dat`: `count · {check1, index, check2, interval, check3}`; checks c1=idx*2+t+i, c2=i*c1+idx*t−i, c3=c1*idx+c2*7+t*i.
- `AutoAllot.dat`: `profs · levels · prof_ids · {force,speed,health,soul}[profs][levels]` (9 × 120).
- `Tips.dat` (unused by 5065): `char[32] "Tips" · version 1000 · count · {n · {len, bytes incl. NUL}}`.
- `LevelExp.dat` (legacy, unused): u32s XOR sign-extended GBK bytes of `★就这样被你蒸熟★`.

The engine never encrypts its own files; ciphers exist only for TQ import/export.
