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
- [ ] DDS / TGA / MSK texture loading
- [ ] ANI index files
- [ ] ini / dat tables (3DObj, 3DTexture, 3DMotion, 3DEffect, armor, weapon, GameMap.dat)
- [x] DMap (cells, heights, portals, interactive + scene layers): 144/144 client maps byte-identical
- [ ] PUL puzzle
- [ ] C3 chunks: PHY (mesh), MOTI (skeletal motion), PTCL (particles), camera
- [ ] Round-trip tests on real client files for every format

## M2: Renderer
- [ ] wgpu window + camera
- [ ] Puzzle ground + map layers (classic iso)
- [ ] Skinned C3 meshes with armor/weapon/mount layering
- [ ] Effects / particles
- [ ] Map + character viewer app

## M3: Studio v1
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
