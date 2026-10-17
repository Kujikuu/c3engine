//! DMap: binary map file (cells, portals, interactive objects, scene layers).
//! Port of `CGameMap::LoadDataMap` / `SaveDataMap` (3dgamemap/3DGameMap.cpp).
//!
//! ```text
//! version: u32 (1004) · data: u32
//! puzzle_file: char[260]
//! width: u32 · height: u32
//! height × ( width × Cell{mask:u16, terrain:u16, altitude:i16} , row_check:u32 )
//! passages: i32 n, n × {x:i32, y:i32, index:i32}
//! interactive: i32 n, n × (type:i32, object)
//! scene layers: i32 n, n × (layer_index:i32, layer_type:i32, SceneLayer)
//! ```

use crate::io::{Error, FixedStr, Reader, Result, Writer};

pub const MAX_PATH: usize = 260;
pub const MAX_TITLE: usize = 128;

pub const OBJ_TERRAIN: i32 = 1;
pub const OBJ_SCENE: i32 = 3;
pub const OBJ_COVER: i32 = 4;
pub const OBJ_PUZZLE: i32 = 8;
pub const OBJ_3DEFFECT: i32 = 10;
pub const OBJ_SOUND: i32 = 15;
pub const OBJ_3DEFFECT_NEW: i32 = 19;
pub const LAYER_SCENE: i32 = 4;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cell {
    /// Non-zero = blocked.
    pub mask: u16,
    pub terrain: u16,
    pub altitude: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Passage {
    pub x: i32,
    pub y: i32,
    pub index: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InteractiveObj {
    /// 2D animated cover object (trees, buildings) occupying a cell footprint.
    Cover {
        ani_file: FixedStr<MAX_PATH>,
        ani_title: FixedStr<MAX_TITLE>,
        cell: (i32, i32),
        base: (u32, u32),
        offset: (i32, i32),
        frame_interval: u32,
    },
    /// Reference to a `.Scene` terrain object file placed at a cell.
    Terrain { file: FixedStr<MAX_PATH>, cell: (i32, i32) },
    Sound { file: FixedStr<MAX_PATH>, world: (i32, i32), range: i32, volume: i32 },
    Effect { index: FixedStr<64>, world: (i32, i32) },
    EffectNew {
        index: FixedStr<64>,
        world: (i32, i32),
        /// The client reads the vertical angle twice; both values are kept for round-trip.
        angles: [f32; 3],
        scale: [f32; 3],
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SceneObj {
    Scene {
        ani_file: FixedStr<MAX_PATH>,
        ani_title: FixedStr<MAX_TITLE>,
        cell: (i32, i32),
        offset: (i32, i32),
        frame_interval: u32,
        show_way: u32,
    },
    Puzzle { file: FixedStr<MAX_PATH> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneLayer {
    pub index: i32,
    pub layer_type: i32,
    pub move_rate: (i32, i32),
    pub objects: Vec<SceneObj>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DMap {
    pub version: u32,
    pub data: u32,
    pub puzzle_file: FixedStr<MAX_PATH>,
    pub width: u32,
    pub height: u32,
    /// Row-major, `cells[y * width + x]`.
    pub cells: Vec<Cell>,
    pub passages: Vec<Passage>,
    pub interactive: Vec<InteractiveObj>,
    pub scene_layers: Vec<SceneLayer>,
}

fn row_check(row: &[Cell], y: u32) -> u32 {
    let mut sum: u32 = 0;
    for (x, c) in row.iter().enumerate() {
        let (m, t, a) = (c.mask as i32, c.terrain as i32, c.altitude as i32);
        let v = m
            .wrapping_mul(t + y as i32 + 1)
            .wrapping_add((a + 2).wrapping_mul(x as i32 + 1 + t));
        sum = sum.wrapping_add(v as u32);
    }
    sum
}

fn pair_i(r: &mut Reader) -> Result<(i32, i32)> {
    Ok((r.i32()?, r.i32()?))
}

impl DMap {
    pub fn new(width: u32, height: u32, puzzle_file: &str) -> Self {
        Self {
            version: 1004,
            data: 0,
            puzzle_file: FixedStr::new(puzzle_file),
            width,
            height,
            cells: vec![Cell::default(); (width * height) as usize],
            passages: Vec::new(),
            interactive: Vec::new(),
            scene_layers: Vec::new(),
        }
    }

    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        Self::parse(&std::fs::read(path)?)
    }

    pub fn cell(&self, x: u32, y: u32) -> Option<&Cell> {
        (x < self.width && y < self.height).then(|| &self.cells[(y * self.width + x) as usize])
    }

    pub fn cell_mut(&mut self, x: u32, y: u32) -> Option<&mut Cell> {
        (x < self.width && y < self.height).then(|| &mut self.cells[(y * self.width + x) as usize])
    }

    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let version = r.u32()?;
        let dw_data = r.u32()?;
        let puzzle_file = r.fixed()?;
        let width = r.u32()?;
        let height = r.u32()?;
        let total = (width as u64) * (height as u64);
        if total * 6 > r.remaining() as u64 {
            return Err(Error::Invalid(format!("map size {width}x{height} exceeds file")));
        }

        let mut cells = Vec::with_capacity(total as usize);
        for y in 0..height {
            let start = cells.len();
            for _ in 0..width {
                cells.push(Cell { mask: r.u16()?, terrain: r.u16()?, altitude: r.i16()? });
            }
            let check = r.u32()?;
            if check != row_check(&cells[start..], y) {
                return Err(Error::Invalid(format!("row {y} checksum mismatch")));
            }
        }

        let n = r.i32()?;
        let mut passages = Vec::new();
        for _ in 0..n.max(0) {
            passages.push(Passage { x: r.i32()?, y: r.i32()?, index: r.i32()? });
        }

        let n = r.i32()?;
        let mut interactive = Vec::new();
        for _ in 0..n.max(0) {
            let ty = r.i32()?;
            let obj = match ty {
                OBJ_COVER => InteractiveObj::Cover {
                    ani_file: r.fixed()?,
                    ani_title: r.fixed()?,
                    cell: pair_i(&mut r)?,
                    base: (r.u32()?, r.u32()?),
                    offset: pair_i(&mut r)?,
                    frame_interval: r.u32()?,
                },
                OBJ_TERRAIN => InteractiveObj::Terrain { file: r.fixed()?, cell: pair_i(&mut r)? },
                OBJ_SOUND => InteractiveObj::Sound {
                    file: r.fixed()?,
                    world: pair_i(&mut r)?,
                    range: r.i32()?,
                    volume: r.i32()?,
                },
                OBJ_3DEFFECT => InteractiveObj::Effect { index: r.fixed()?, world: pair_i(&mut r)? },
                OBJ_3DEFFECT_NEW => InteractiveObj::EffectNew {
                    index: r.fixed()?,
                    world: pair_i(&mut r)?,
                    angles: [r.f32()?, r.f32()?, r.f32()?],
                    scale: [r.f32()?, r.f32()?, r.f32()?],
                },
                // The client silently skips unknown types, which desyncs the stream.
                _ => return Err(Error::Invalid(format!("unknown interactive obj type {ty} at {}", r.pos()))),
            };
            interactive.push(obj);
        }

        let mut scene_layers = Vec::new();
        // Some maps end right after the interactive layer.
        if r.remaining() >= 4 {
            let n = r.i32()?;
            for _ in 0..n.max(0) {
                let index = r.i32()?;
                let layer_type = r.i32()?;
                if layer_type != LAYER_SCENE {
                    return Err(Error::Invalid(format!("unsupported layer type {layer_type}")));
                }
                let move_rate = pair_i(&mut r)?;
                let count = r.i32()?;
                let mut objects = Vec::new();
                for _ in 0..count.max(0) {
                    let ty = r.i32()?;
                    objects.push(match ty {
                        OBJ_SCENE => SceneObj::Scene {
                            ani_file: r.fixed()?,
                            ani_title: r.fixed()?,
                            cell: pair_i(&mut r)?,
                            offset: pair_i(&mut r)?,
                            frame_interval: r.u32()?,
                            show_way: r.u32()?,
                        },
                        OBJ_PUZZLE => SceneObj::Puzzle { file: r.fixed()? },
                        _ => return Err(Error::Invalid(format!("unknown scene obj type {ty}"))),
                    });
                }
                scene_layers.push(SceneLayer { index, layer_type, move_rate, objects });
            }
        }

        if r.remaining() != 0 {
            return Err(Error::Invalid(format!("{} trailing bytes", r.remaining())));
        }

        Ok(Self { version, data: dw_data, puzzle_file, width, height, cells, passages, interactive, scene_layers })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u32(self.version);
        w.u32(self.data);
        w.fixed(&self.puzzle_file);
        w.u32(self.width);
        w.u32(self.height);
        for (y, row) in self.cells.chunks(self.width.max(1) as usize).enumerate() {
            for c in row {
                w.u16(c.mask);
                w.u16(c.terrain);
                w.i16(c.altitude);
            }
            w.u32(row_check(row, y as u32));
        }

        w.i32(self.passages.len() as i32);
        for p in &self.passages {
            w.i32(p.x);
            w.i32(p.y);
            w.i32(p.index);
        }

        w.i32(self.interactive.len() as i32);
        for o in &self.interactive {
            match o {
                InteractiveObj::Cover { ani_file, ani_title, cell, base, offset, frame_interval } => {
                    w.i32(OBJ_COVER);
                    w.fixed(ani_file);
                    w.fixed(ani_title);
                    w.i32(cell.0);
                    w.i32(cell.1);
                    w.u32(base.0);
                    w.u32(base.1);
                    w.i32(offset.0);
                    w.i32(offset.1);
                    w.u32(*frame_interval);
                }
                InteractiveObj::Terrain { file, cell } => {
                    w.i32(OBJ_TERRAIN);
                    w.fixed(file);
                    w.i32(cell.0);
                    w.i32(cell.1);
                }
                InteractiveObj::Sound { file, world, range, volume } => {
                    w.i32(OBJ_SOUND);
                    w.fixed(file);
                    w.i32(world.0);
                    w.i32(world.1);
                    w.i32(*range);
                    w.i32(*volume);
                }
                InteractiveObj::Effect { index, world } => {
                    w.i32(OBJ_3DEFFECT);
                    w.fixed(index);
                    w.i32(world.0);
                    w.i32(world.1);
                }
                InteractiveObj::EffectNew { index, world, angles, scale } => {
                    w.i32(OBJ_3DEFFECT_NEW);
                    w.fixed(index);
                    w.i32(world.0);
                    w.i32(world.1);
                    angles.iter().chain(scale).for_each(|&f| w.f32(f));
                }
            }
        }

        w.i32(self.scene_layers.len() as i32);
        for l in &self.scene_layers {
            w.i32(l.index);
            w.i32(l.layer_type);
            w.i32(l.move_rate.0);
            w.i32(l.move_rate.1);
            w.i32(l.objects.len() as i32);
            for o in &l.objects {
                match o {
                    SceneObj::Scene { ani_file, ani_title, cell, offset, frame_interval, show_way } => {
                        w.i32(OBJ_SCENE);
                        w.fixed(ani_file);
                        w.fixed(ani_title);
                        w.i32(cell.0);
                        w.i32(cell.1);
                        w.i32(offset.0);
                        w.i32(offset.1);
                        w.u32(*frame_interval);
                        w.u32(*show_way);
                    }
                    SceneObj::Puzzle { file } => {
                        w.i32(OBJ_PUZZLE);
                        w.fixed(file);
                    }
                }
            }
        }
        w.buf
    }
}
