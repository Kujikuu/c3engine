# TQ Formats Reference

Living notes for each format we reverse/implement. Fill these in as parsers land.
Sources: `D:\Conquer2\Tools\eosource\client5065` (assets), `...\cosource` (client source),
`...\tq_re` (Ghidra workspace), `...\Original_TQ_Conquer_Online_Source`.

| Format | Purpose | Status | Notes |
|---|---|---|---|
| WDF | packed archive | **done** | `wdf.rs` |
| DNP | packed archive | todo | |
| DDS/TGA | textures | **done** | `texture.rs`; client uses DXT1 (data.wdf) + DXT3, no mips |
| MSK | masks | todo | |
| ANI | animation/frame index (text) | **done** | `ani.rs` |
| DMap | map cells, heights, portals, scene refs | **done** | `dmap.rs` |
| PUL | puzzle ground tiles | **done** | `pul.rs` |
| C3 PHY | mesh + skinning | todo | |
| C3 MOTI | skeletal motion keyframes | todo | |
| C3 PTCL | particles | todo | |
| ini/dat tables | model/texture/motion/effect wiring | todo | |

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
