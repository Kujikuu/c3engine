//! Painting the ground layer: tiles set to an image (added to the tileset when it's new) or
//! cleared, one by one or by flood fill. A map without a ground layer can't be painted.

use super::{Change, Edit, SceneDoc};
use crate::EditError;
use c3_scene::TileLayer;
use std::collections::VecDeque;

/// Most tiles a fill may change at once.
pub const MAX_FILL_TILES: usize = 1 << 18;

impl SceneDoc {
    pub(super) fn paint_ground(&mut self, tiles: &[(u32, u32)], image: Option<&str>) -> Edit {
        let mut after = self.ground()?.clone();
        let tile = image.map(|i| frame_for(&mut after, i));
        let mut painted = 0;
        for &(x, y) in tiles {
            if let Some(i) = index(&after, x, y)
                && after.tiles[i] != tile
            {
                after.tiles[i] = tile;
                painted += 1;
            }
        }
        let verb = if image.is_some() { "Paint" } else { "Erase" };
        self.ground_change(after, format!("{verb} {painted} ground tiles"))
    }

    pub(super) fn fill_ground(&mut self, at: (u32, u32), image: Option<&str>) -> Edit {
        let mut after = self.ground()?.clone();
        let Some(start) = index(&after, at.0, at.1) else {
            return Err(EditError::InvalidArea(format!(
                "tile {}, {} is off the ground",
                at.0, at.1
            )));
        };
        let tile = image.map(|i| frame_for(&mut after, i));
        let from = after.tiles[start];
        if from == tile {
            return Ok(("Nothing changed".into(), None));
        }
        // Every tile joined to `at` (side by side) that shows what it shows.
        let mut queue = VecDeque::from([at]);
        let mut filled = 0;
        while let Some((x, y)) = queue.pop_front() {
            let Some(i) = index(&after, x, y).filter(|&i| after.tiles[i] == from) else {
                continue;
            };
            after.tiles[i] = tile;
            filled += 1;
            if filled > MAX_FILL_TILES {
                return Err(EditError::AreaTooLarge(filled));
            }
            queue.extend([
                (x + 1, y),
                (x.wrapping_sub(1), y),
                (x, y + 1),
                (x, y.wrapping_sub(1)),
            ]);
        }
        self.ground_change(after, format!("Fill {filled} ground tiles"))
    }

    fn ground(&self) -> Result<&TileLayer, EditError> {
        (self.scene.ground.as_ref())
            .ok_or_else(|| EditError::InvalidArea("the map has no ground layer".into()))
    }

    fn ground_change(&mut self, after: TileLayer, summary: String) -> Edit {
        let before = self.ground()?.clone();
        if before == after {
            return Ok(("Nothing changed".into(), None));
        }
        self.scene.ground = Some(after.clone());
        let change = Change::Ground {
            before: Box::new(before),
            after: Box::new(after),
        };
        Ok((summary, Some(change)))
    }
}

/// The tile index of `(x, y)`, if it is on the layer.
fn index(layer: &TileLayer, x: u32, y: u32) -> Option<usize> {
    (x < layer.width && y < layer.height).then(|| (y * layer.width + x) as usize)
}

/// The tileset's id of `image`, added as a new frame when the tileset doesn't have it.
fn frame_for(layer: &mut TileLayer, image: &str) -> u16 {
    let frames = &mut layer.tileset.frames;
    if let Some((&id, _)) = frames.iter().find(|(_, path)| path.as_str() == image) {
        return id;
    }
    let id = frames.keys().next_back().map_or(0, |id| id + 1);
    frames.insert(id, image.to_owned());
    id
}

#[cfg(test)]
mod tests {
    use crate::SceneCommand;
    use crate::scene::tests::doc;
    use c3_scene::TileLayer;

    fn with_ground() -> crate::SceneDoc {
        let mut scene = doc().scene().clone();
        scene.ground = Some(TileLayer::covering(
            "grass.png",
            64,
            &scene.projection,
            &scene.grid,
        ));
        crate::SceneDoc::new(scene)
    }

    #[test]
    fn painting_adds_frames_and_fill_stops_at_other_tiles() {
        let mut d = with_ground();
        let paint = |tiles: Vec<(u32, u32)>, image: Option<&str>| SceneCommand::PaintGround {
            tiles,
            image: image.map(str::to_owned),
        };
        // A wall of dirt across row 1.
        let wall: Vec<(u32, u32)> = (0..20).map(|x| (x, 1)).collect();
        d.execute(&paint(wall, Some("dirt.png"))).unwrap();
        let ground = d.scene().ground.as_ref().unwrap();
        assert_eq!(ground.tileset.frames.len(), 2);
        let dirt = ground.tiles[(ground.width + 3) as usize];
        assert_eq!(dirt, Some(1));
        // Filling above the wall stays above it.
        d.execute(&SceneCommand::FillGround {
            at: (0, 0),
            image: None,
        })
        .unwrap();
        let ground = d.scene().ground.as_ref().unwrap();
        assert_eq!(ground.tiles[0], None);
        assert_eq!(ground.tiles[(ground.width * 2) as usize], Some(0));
        d.undo();
        assert_eq!(d.scene().ground.as_ref().unwrap().tiles[0], Some(0));
    }
}
