# Roadmap

Status keys: `[ ]` todo · `[~]` in progress · `[x]` done

## M0: Setup
- [x] Rust stable MSVC toolchain installed (rustc 1.99.1)
- [x] Workspace scaffold + docs
- [ ] CI-like local script: `cargo fmt --check`, `cargo clippy`, `cargo test`

## M1: TQ formats [~] (`formats/c3`)
Reference assets: `D:\Conquer2\Tools\eosource\client5065`
- [x] WDF package read + write (byte-identical on c3.wdf, data.wdf)
- [ ] DNP package
- [x] DDS (DXT1/3/5, raw) + TGA decode: all 20,144 packed DDS decode
- [x] ClientFs VFS (loose files first, then WDF) + `mapshot` example (map ground → PNG)
- [ ] MSK masks
- [x] ANI index files (51 files / 56,971 sections; GBK-safe; frame paths resolve in WDF)
- [ ] ini / dat tables (3DObj, 3DTexture, 3DMotion, 3DEffect, armor, weapon, GameMap.dat)
- [x] DMap (cells, heights, portals, interactive + scene layers): 144/144 client maps byte-identical
- [x] PUL puzzle (143/143 byte-identical; every used tile resolves in its ANI)
- [ ] C3 chunks: PHY (mesh), MOTI (skeletal motion), PTCL (particles), camera
- [ ] Round-trip tests on real client files for every format

## Course correction (2026-10-18), see DECISIONS.md
- [x] (1) `engine/scene` + `engine/assets` + TQ importer/exporter: 144/144 maps round-trip
- [x] (2) Studio edits engine `Scene`s; TQ access isolated in `studio/src/tq_plugin.rs`
- [x] (3) C3 models: container + PHY + MOTI (incl. XKEY), engine `c3-anim` Clip/skinning and
      `c3_assets::model`, TQ importer `model_io`; all 4,516 files verified; `modelshot` renders posed models
- [ ] (3b) Typed decode of SHAP/SMOT (trails), PTCL/PTCX/PTC3 (particles), CAME (camera). Currently raw
- [ ] (3c) Remaining M1 tables: ini/dat wiring (3DObj, 3DMotion, 3DTexture, armor, weapon), DNP, MSK
- [ ] (4) M2 renderer (wgpu)

## M2: Renderer
- [ ] wgpu window + camera
- [ ] Puzzle ground + map layers (classic iso)
- [ ] Skinned C3 meshes with armor/weapon/mount layering
- [ ] Effects / particles
- [ ] Map + character viewer app

## M3: Studio v1
- [x] Studio shell (eframe/egui): toolbar, map list, inspector, status bar, theme, shortcuts
- [x] Map viewer: streamed ground tiles at client-exact placement, cell grid, blocked cells,
      portals (jump-to), objects, hover inspector, fit/zoom/pan
- [x] Map editing: Block / Walkable brush (1–21 cells), portal add/move/delete, undo/redo
      (strokes merge into one step), Save (Ctrl+S) with atomic write + first-save `.bak`,
      unsaved-changes guard on map switch and quit
- [ ] Draw scene objects (cover/scene ANI sprites) so maps look complete
- [ ] Project system (open/save game project)
- [ ] Asset browser (packages, textures, models)
- [ ] Map editor (cells, objects, portals, export DMap + Redux tinymap/SQL)
- [ ] Model / animation viewer

## M4: Conquer game module
- [ ] 5065 cipher + packet framework
- [ ] Login / account server → game server
- [ ] Spawn, walk, jump, other players, NPCs
- [ ] Combat, items, UI parity

## M5: Data + scripting
- [ ] Data tables editor (items, skills, monsters)
- [ ] Scripting host; gameplay without recompiling
- [ ] Prove generality: a tiny second sample game

## M6: Ship
- [ ] Asset packaging, client build, launcher/patcher
- [ ] glTF import/export (Blender pipeline) for models + animations
