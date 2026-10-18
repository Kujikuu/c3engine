//! A parallax layer in the Inspector: its draw order and scroll rate, and its items (tiled
//! skies and water, animated sprites) to add from the Project panel, move or delete.

use crate::app::Studio;
use crate::browser::AssetKind;
use crate::icons;
use crate::scene_inspector::{Interaction, cell_field, grid, number, row};
use crate::theme;
use c3_edit::SceneCommand;
use c3_scene::{EntityKind, LayerItem, ParallaxLayer};
use eframe::egui::{self, RichText};

impl Studio {
    /// Shows the layer the Hierarchy has selected; false when none is.
    pub(crate) fn layer_inspector(&mut self, ui: &mut egui::Ui) -> bool {
        let (Some(doc), Some(index)) = (&self.doc, self.editor.layer) else {
            return false;
        };
        let Some(mut layer) = doc.scene().layers.get(index).cloned() else {
            self.editor.layer = None;
            return false;
        };
        let mut delete = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(icons::STACK).size(22.0).color(theme::ACCENT));
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(format!("Layer {}", layer.order))
                        .size(15.0)
                        .strong(),
                );
                ui.label(
                    RichText::new("Parallax layer: moves slower than the map, like far scenery")
                        .color(theme::TEXT_DIM)
                        .small(),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                delete = theme::icon_toggle(ui, false, icons::TRASH, "Delete the layer").clicked();
            });
        });
        ui.separator();
        let mut it = Interaction::default();
        grid(ui, "layer", |ui| {
            row(ui, "Order", |ui| {
                number(ui, &mut it, &mut layer.order);
                ui.label(
                    RichText::new("lower draws behind")
                        .small()
                        .color(theme::TEXT_DIM),
                );
            });
            row(ui, "Scroll", |ui| {
                ui.horizontal(|ui| {
                    number(ui, &mut it, &mut layer.scroll_rate.0);
                    number(ui, &mut it, &mut layer.scroll_rate.1);
                    ui.label(
                        RichText::new("% of the camera")
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                });
            });
        });
        ui.add_space(6.0);
        let mut remove_item = None;
        theme::section(ui, &format!("Items ({})", layer.items.len()), |ui| {
            for (i, item) in layer.items.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let (icon, asset) = match item {
                        LayerItem::Tiles { asset } => (icons::SQUARES_FOUR, asset.clone()),
                        LayerItem::Sprite { asset, .. } => (icons::IMAGE, asset.clone()),
                    };
                    ui.label(icon);
                    let name = asset
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or(&asset)
                        .to_owned();
                    ui.label(name).on_hover_text(&asset);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(icons::X)
                            .on_hover_text("Remove from the layer")
                            .clicked()
                        {
                            remove_item = Some(i);
                        }
                    });
                });
                if let LayerItem::Sprite { cell, offset, .. } = item {
                    grid(ui, &format!("layer item {i}"), |ui| {
                        row(ui, "Cell", |ui| cell_field(ui, &mut it, cell));
                        row(ui, "Offset", |ui| {
                            ui.horizontal(|ui| {
                                number(ui, &mut it, &mut offset.0);
                                number(ui, &mut it, &mut offset.1);
                            });
                        });
                    });
                }
            }
            if layer.items.is_empty() {
                ui.label(RichText::new("Nothing on this layer yet.").color(theme::TEXT_DIM));
            }
        });
        let addition = self.layer_item_from_selection();
        let hint = match &addition {
            Some(_) => "Adds the asset selected in the Project panel",
            None => "Select a ground file (.pul), sprite set (.ani) or image in the Project panel",
        };
        let add = ui
            .add_enabled(
                addition.is_some(),
                egui::Button::new(theme::icon_text(icons::PLUS, "Add from Project selection")),
            )
            .on_hover_text(hint)
            .on_disabled_hover_text(hint)
            .clicked();
        let mut command = None;
        if let Some(i) = remove_item {
            layer.items.remove(i);
            it.changed = true;
            it.started = true;
            it.finished = true;
        }
        if add && let Some(item) = addition {
            layer.items.push(item);
            it.changed = true;
            it.started = true;
            it.finished = true;
        }
        if it.changed {
            command = Some(SceneCommand::SetLayer { index, layer });
        }
        self.apply_edit(it, "the layer", command);
        if delete {
            self.remove_layer(index);
        }
        true
    }

    /// The Project panel's selection as a layer item: a ground file is tiled, a sprite set
    /// or image stands on the cell in the middle of the view.
    fn layer_item_from_selection(&self) -> Option<LayerItem> {
        let key = self.browser.selected.as_ref()?;
        if AssetKind::of(&key.rel) == AssetKind::Puzzle {
            return Some(LayerItem::Tiles {
                asset: key.rel.clone(),
            });
        }
        let doc = self.doc.as_ref()?;
        let at = doc.world_to_cell(self.cam.center);
        let EntityKind::Sprite {
            asset,
            animation,
            offset,
            frame_ms,
            ..
        } = self.asset_entity(key, at)?.kind
        else {
            return None;
        };
        Some(LayerItem::Sprite {
            asset,
            animation,
            cell: at,
            offset,
            frame_ms,
            blend: 0,
        })
    }

    /// Adds an empty layer drawn in front of the others, and selects it.
    pub(crate) fn add_layer(&mut self) {
        let Some(doc) = &mut self.doc else { return };
        let order = doc
            .scene()
            .layers
            .iter()
            .map(|l| l.order + 1)
            .max()
            .unwrap_or(0);
        let layer = ParallaxLayer {
            order,
            scroll_rate: (50, 50),
            items: Vec::new(),
        };
        match doc.execute(&SceneCommand::AddLayer { layer, index: None }) {
            Ok(outcome) => {
                self.editor.select(None);
                self.editor.layer = Some(doc.scene().layers.len() - 1);
                self.inspect_asset = false;
                self.notify(outcome.summary);
            }
            Err(e) => self.error = Some(format!("Can't add a layer: {e}")),
        }
    }

    fn remove_layer(&mut self, index: usize) {
        let Some(doc) = &mut self.doc else { return };
        match doc.execute(&SceneCommand::RemoveLayer { index }) {
            Ok(outcome) => {
                self.editor.layer = None;
                self.notify(outcome.summary);
            }
            Err(e) => self.error = Some(format!("Can't remove the layer: {e}")),
        }
    }
}
