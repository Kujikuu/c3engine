//! Parallax layers (sky, moving water, far scenery): adding, changing and removing them.

use super::{Change, Edit, SceneDoc};
use crate::EditError;
use c3_scene::ParallaxLayer;

impl SceneDoc {
    pub(super) fn add_layer(&mut self, layer: &ParallaxLayer, index: Option<usize>) -> Edit {
        let at = index.unwrap_or(self.scene.layers.len());
        if at > self.scene.layers.len() {
            return Err(no_layer(at));
        }
        let mut after = self.scene.layers.clone();
        after.insert(at, layer.clone());
        Ok(self.layers_change(after, format!("Add layer {}", layer.order)))
    }

    pub(super) fn set_layer(&mut self, index: usize, layer: &ParallaxLayer) -> Edit {
        let mut after = self.scene.layers.clone();
        *after.get_mut(index).ok_or_else(|| no_layer(index))? = layer.clone();
        Ok(self.layers_change(after, format!("Change layer {}", layer.order)))
    }

    pub(super) fn remove_layer(&mut self, index: usize) -> Edit {
        let mut after = self.scene.layers.clone();
        if index >= after.len() {
            return Err(no_layer(index));
        }
        let gone = after.remove(index);
        Ok(self.layers_change(after, format!("Remove layer {}", gone.order)))
    }

    fn layers_change(
        &mut self,
        after: Vec<ParallaxLayer>,
        summary: String,
    ) -> (String, Option<Change>) {
        if after == self.scene.layers {
            return ("Nothing changed".into(), None);
        }
        let before = std::mem::replace(&mut self.scene.layers, after.clone());
        (summary, Some(Change::Layers { before, after }))
    }
}

fn no_layer(index: usize) -> EditError {
    EditError::InvalidArea(format!("there is no layer {index}"))
}

#[cfg(test)]
mod tests {
    use crate::SceneCommand;
    use crate::scene::tests::doc;
    use c3_scene::{LayerItem, ParallaxLayer};

    #[test]
    fn layers_add_change_and_remove_with_undo() {
        let mut d = doc();
        let sky = ParallaxLayer {
            order: 0,
            scroll_rate: (50, 50),
            items: vec![LayerItem::Tiles {
                asset: "map/sky.pul".into(),
            }],
        };
        d.execute(&SceneCommand::AddLayer {
            layer: sky.clone(),
            index: None,
        })
        .unwrap();
        let mut slower = sky.clone();
        slower.scroll_rate = (20, 20);
        d.execute(&SceneCommand::SetLayer {
            index: 0,
            layer: slower,
        })
        .unwrap();
        assert_eq!(d.scene().layers[0].scroll_rate, (20, 20));
        assert!(d.execute(&SceneCommand::RemoveLayer { index: 3 }).is_err());
        d.execute(&SceneCommand::RemoveLayer { index: 0 }).unwrap();
        assert!(d.scene().layers.is_empty());
        d.undo();
        d.undo();
        assert_eq!(d.scene().layers, [sky]);
    }
}
