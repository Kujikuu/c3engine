//! TQ 2D assets → engine sprite types: ANI → [`SpriteSet`], `.scene` → [`Prefab`].

use crate::ani::Ani;
use crate::scene::TerrainObject;
use c3_assets::sprite::{Prefab, PrefabPart, SpriteSet};

fn gbk(bytes: &[u8]) -> String {
    encoding_rs::GBK
        .decode_without_bom_handling(bytes)
        .0
        .into_owned()
}

/// Every ANI section becomes an animation (first section wins on duplicate titles,
/// as in the client).
pub fn sprite_set_from_ani(ani: &Ani) -> SpriteSet {
    let mut set = SpriteSet::default();
    for s in &ani.sections {
        set.animations
            .entry(gbk(&s.title))
            .or_insert_with(|| s.frames.iter().map(|f| gbk(f)).collect());
    }
    set
}

/// Terrain object parts draw at `cell + scene_offset`, with the sprite's top-left at
/// `offset` from that cell's centre (`C2DMapTerrainObjPart::Show`).
pub fn prefab_from_scene(obj: &TerrainObject) -> Prefab {
    Prefab {
        parts: obj
            .parts
            .iter()
            .map(|p| PrefabPart {
                sprite_set: p.ani_file.to_gbk_string(),
                animation: p.ani_title.to_gbk_string(),
                offset: p.offset,
                frame_ms: p.frame_interval,
                footprint: (p.width, p.height),
                cell_offset: p.scene_offset,
            })
            .collect(),
    }
}
