//! What scene rendering needs from a game's content: implemented by each game's loader.

use c3_assets::Image;
use c3_assets::sprite::{Prefab, SpriteSet};
use c3_scene::TileLayer;

pub trait SceneAssets {
    fn sprite_set(&self, path: &str) -> Option<SpriteSet>;
    fn prefab(&self, path: &str) -> Option<Prefab>;
    /// A tile layer (origin 0, 0) by asset path, for parallax backgrounds.
    fn tile_layer(&self, path: &str) -> Option<TileLayer>;
    fn image(&self, path: &str) -> Option<Image>;
}
