//! 2D sprite assets: named frame animations and multi-part map objects.

use std::collections::BTreeMap;

/// Named animations, each an ordered list of image asset paths.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpriteSet {
    pub animations: BTreeMap<String, Vec<String>>,
}

impl SpriteSet {
    pub fn frames(&self, animation: &str) -> Option<&[String]> {
        self.animations.get(animation).map(Vec::as_slice)
    }
}

/// One animated part of a [`Prefab`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefabPart {
    /// Sprite set asset and the animation within it.
    pub sprite_set: String,
    pub animation: String,
    /// Sprite top-left relative to the part's cell centre, in pixels.
    pub offset: (i32, i32),
    pub frame_ms: u32,
    /// Footprint in cells (extends toward −x/−y from the part's cell).
    pub footprint: (u32, u32),
    /// Part cell relative to the prefab's anchor cell.
    pub cell_offset: (i32, i32),
}

/// Multi-part map object placed at one anchor cell (e.g. a building).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prefab {
    pub parts: Vec<PrefabPart>,
}
