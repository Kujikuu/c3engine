//! The Hierarchy panel: everything on the open map in groups (portals, NPCs, spawn areas,
//! effects, objects, sounds, layers). Clicking a row selects it and shows it in the Scene;
//! the "+" menu creates new things where the Scene view looks, like Unity's Create menu.

use crate::app::{Studio, Workspace};
use crate::icons;
use crate::selection::Selection;
use crate::theme;
use c3_edit::SceneCommand;
use c3_scene::{CellPos, Entity, EntityKind, Npc, Placement, Spawn};
use eframe::egui::{self, RichText};

/// Most effect names the create menu lists at once (narrow down with its search).
const MAX_LISTED_EFFECTS: usize = 200;
/// Half the side of a new spawn area, in cells.
const NEW_SPAWN_RADIUS: i32 = 4;

/// A row of the tree: its text and what clicking it selects.
type Row = (String, Selection);

impl Studio {
    pub(crate) fn hierarchy_panel(&mut self, ui: &mut egui::Ui) {
        if self.workspace == Workspace::Models {
            self.model_hierarchy(ui);
            return;
        }
        let Some(doc) = &self.doc else {
            theme::empty_state(
                ui,
                icons::TREE_STRUCTURE,
                "No map open",
                "Open a map from the Project panel.",
            );
            return;
        };
        let name = if doc.is_dirty() {
            format!("{}*", doc.name())
        } else {
            doc.name().to_owned()
        };
        let mut picked = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new(theme::icon_text(icons::MAP_TRIFOLD, &name)).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button(theme::icon_text(icons::PLUS, "Create"), |ui| {
                    self.create_menu(ui);
                })
                .response
                .on_hover_text("Create a portal, NPC, spawn area, effect or object");
            });
        });
        ui.separator();
        let Some(doc) = &self.doc else { return };
        let scene = doc.scene();
        let selected = self.editor.selected;
        // A new selection (picked in the Scene) opens its group and scrolls to its row once.
        let reveal = selected.filter(|s| self.revealed != Some(*s));
        self.revealed = selected;
        let tree = Tree { selected, reveal };
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let portals: Vec<Row> = scene
                    .portals
                    .iter()
                    .map(|p| {
                        let label = format!("Portal {}   {}, {}", p.id, p.cell.0, p.cell.1);
                        (label, Selection::Portal(p.id))
                    })
                    .collect();
                tree.group(ui, icons::DOOR_OPEN, "Portals", &portals, &mut picked);
                let npcs: Vec<Row> = scene
                    .npcs
                    .iter()
                    .map(|n| (format!("{}   #{}", n.name, n.id), Selection::Npc(n.id)))
                    .collect();
                tree.group(ui, icons::USER, "NPCs", &npcs, &mut picked);
                let spawns: Vec<Row> = scene
                    .spawns
                    .iter()
                    .map(|s| {
                        let label = format!("Monster {} × {}   #{}", s.monster, s.max_alive, s.id);
                        (label, Selection::Spawn(s.id))
                    })
                    .collect();
                tree.group(ui, icons::USERS_THREE, "Spawn areas", &spawns, &mut picked);
                for (icon, group, keep) in [
                    (icons::SPARKLE, "Effects", 0),
                    (icons::TREE, "Objects", 1),
                    (icons::SPEAKER_HIGH, "Sounds", 2),
                ] {
                    let rows: Vec<Row> = (scene.entities.iter().enumerate())
                        .filter_map(|(i, e)| {
                            let (kind, name) = match &e.kind {
                                EntityKind::Effect { name, .. } => (0, name.as_str()),
                                EntityKind::Sprite { animation, .. } => (1, animation.as_str()),
                                EntityKind::Prefab { asset } => (1, file_stem(asset)),
                                EntityKind::Sound { asset, .. } => (2, file_stem(asset)),
                            };
                            (kind == keep).then(|| (name.to_owned(), Selection::Entity(i)))
                        })
                        .collect();
                    tree.group(ui, icon, group, &rows, &mut picked);
                }
                ui.add_enabled_ui(false, |ui| {
                    let header =
                        theme::icon_text(icons::STACK, &format!("Layers ({})", scene.layers.len()));
                    egui::CollapsingHeader::new(header)
                        .id_salt("Layers")
                        .show(ui, |ui| {
                            for l in &scene.layers {
                                ui.label(format!(
                                    "Layer {} · scroll {}%",
                                    l.order, l.scroll_rate.0
                                ));
                            }
                        });
                });
            });
        if let Some(selection) = picked {
            self.editor.selected = Some(selection);
            self.inspect_asset = false;
            self.frame_selection();
        }
    }

    /// The "+" menu: new things appear at the middle of the Scene view and get selected.
    fn create_menu(&mut self, ui: &mut egui::Ui) {
        let Some(doc) = &self.doc else { return };
        let at = doc.world_to_cell(self.cam.center);
        if ui
            .button(theme::icon_text(icons::DOOR_OPEN, "Portal"))
            .clicked()
        {
            self.create(SceneCommand::AddPortal { cell: at, id: None });
            ui.close();
        }
        if ui.button(theme::icon_text(icons::USER, "NPC")).clicked() {
            self.create(SceneCommand::AddNpc { npc: new_npc(at) });
            ui.close();
        }
        if ui
            .button(theme::icon_text(icons::USERS_THREE, "Spawn area"))
            .clicked()
        {
            self.create(SceneCommand::AddSpawn {
                spawn: new_spawn(at),
            });
            ui.close();
        }
        ui.menu_button(theme::icon_text(icons::SPARKLE, "Effect"), |ui| {
            self.effect_picker(ui, at);
        });
        let object = self.browser_object(at);
        let hint = match &object {
            Some(_) => "Place the asset selected in the Project panel",
            None => "Select a sprite set (.ani) or terrain object (.scene) in the Project panel",
        };
        let button = ui.add_enabled(
            object.is_some(),
            egui::Button::new(theme::icon_text(
                icons::TREE,
                "Object from Project selection",
            )),
        );
        if button.on_hover_text(hint).clicked()
            && let Some(entity) = object
        {
            self.create(SceneCommand::AddEntity {
                entity,
                index: None,
            });
            ui.close();
        }
    }

    /// Places the effect chosen in the effect list on `at`.
    fn effect_picker(&mut self, ui: &mut egui::Ui, at: CellPos) {
        if let Some(name) = self.effect_list(ui) {
            self.create(SceneCommand::AddEntity {
                entity: Entity {
                    placement: Placement::Cell(at),
                    kind: EntityKind::Effect {
                        name,
                        transform: None,
                    },
                },
                index: None,
            });
            ui.close();
        }
    }

    /// Searchable list of the content's effects; returns the one clicked.
    pub(crate) fn effect_list(&mut self, ui: &mut egui::Ui) -> Option<String> {
        ui.add(
            egui::TextEdit::singleline(&mut self.effect_search)
                .hint_text(theme::icon_text(icons::MAGNIFYING_GLASS, "Search effects"))
                .desired_width(220.0),
        );
        let needle = self.effect_search.to_lowercase();
        let matches: Vec<&String> = self
            .effect_names
            .iter()
            .filter(|n| n.to_lowercase().contains(&needle))
            .take(MAX_LISTED_EFFECTS + 1)
            .collect();
        if matches.is_empty() {
            ui.label(RichText::new("No effect matches").color(theme::TEXT_DIM));
            return None;
        }
        let mut chosen = None;
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                for name in matches.iter().take(MAX_LISTED_EFFECTS) {
                    if ui.button(name.as_str()).clicked() {
                        chosen = Some((*name).clone());
                    }
                }
            });
        if matches.len() > MAX_LISTED_EFFECTS {
            ui.label(
                RichText::new("Showing the first 200: search to narrow down")
                    .color(theme::TEXT_DIM)
                    .small(),
            );
        }
        chosen
    }

    /// The Project panel's selection as a new object on `at`: a sprite (first animation of a
    /// sprite set) or a terrain object.
    fn browser_object(&self, at: CellPos) -> Option<Entity> {
        let key = self.browser.selected.as_ref()?;
        let client = self.client.as_ref()?;
        let rel = key.rel.as_str();
        let ext = rel.rsplit_once('.')?.1.to_ascii_lowercase();
        let kind = match ext.as_str() {
            "ani" => {
                let set = c3_render::SceneAssets::sprite_set(client, rel)?;
                let animation = set.animations.keys().next()?.clone();
                EntityKind::Sprite {
                    asset: rel.to_owned(),
                    animation,
                    footprint: (1, 1),
                    offset: (0, 0),
                    frame_ms: 100,
                }
            }
            "scene" | "part" => EntityKind::Prefab {
                asset: rel.to_owned(),
            },
            _ => return None,
        };
        Some(Entity {
            placement: Placement::Cell(at),
            kind,
        })
    }

    /// Runs a create command and selects what it made.
    fn create(&mut self, command: SceneCommand) {
        let Some(doc) = &mut self.doc else { return };
        match doc.execute(&command) {
            Ok(outcome) => {
                let scene = doc.scene();
                self.editor.selected = match command {
                    SceneCommand::AddPortal { cell, .. } => scene
                        .portal_at(cell)
                        .map(|i| Selection::Portal(scene.portals[i].id)),
                    SceneCommand::AddNpc { .. } => scene.npcs.last().map(|n| Selection::Npc(n.id)),
                    SceneCommand::AddSpawn { .. } => {
                        scene.spawns.last().map(|s| Selection::Spawn(s.id))
                    }
                    SceneCommand::AddEntity { .. } => {
                        scene.entities.len().checked_sub(1).map(Selection::Entity)
                    }
                    _ => None,
                };
                self.editor.tool = crate::editor::Tool::Select;
                self.inspect_asset = false;
                self.notify(outcome.summary);
            }
            Err(e) => self.error = Some(format!("Can't create it here: {e}")),
        }
    }
}

fn new_npc(cell: CellPos) -> Npc {
    Npc {
        id: 0,
        name: "New NPC".into(),
        cell,
        direction: 0,
        look: 1,
        kind: 0,
        extra: Default::default(),
    }
}

fn new_spawn(at: CellPos) -> Spawn {
    let r = NEW_SPAWN_RADIUS;
    Spawn {
        id: 0,
        monster: 1,
        min: (at.0 - r, at.1 - r),
        max: (at.0 + r, at.1 + r),
        max_alive: 5,
        respawn_secs: 10,
        per_wave: 2,
        extra: Default::default(),
    }
}

/// How the tree marks the selection this frame.
struct Tree {
    selected: Option<Selection>,
    /// Selection to open the group of and scroll to (it just changed).
    reveal: Option<Selection>,
}

impl Tree {
    /// A collapsible group whose rows select (and show) what they list.
    fn group(
        &self,
        ui: &mut egui::Ui,
        icon: &str,
        title: &str,
        rows: &[Row],
        picked: &mut Option<Selection>,
    ) {
        let header = theme::icon_text(icon, &format!("{title} ({})", rows.len()));
        let reveal = rows.iter().any(|(_, s)| Some(*s) == self.reveal);
        egui::CollapsingHeader::new(header)
            .id_salt(title)
            .default_open(rows.len() <= 12)
            .open(reveal.then_some(true))
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                    for (label, selection) in rows {
                        let on = Some(*selection) == self.selected;
                        let row = ui
                            .add(egui::Button::selectable(on, label))
                            .on_hover_text("Click to select it and show it in the Scene");
                        if Some(*selection) == self.reveal {
                            row.scroll_to_me(Some(egui::Align::Center));
                        }
                        if row.clicked() {
                            *picked = Some(*selection);
                        }
                    }
                });
            });
    }
}

/// `ani/tree.ani` → `tree`.
fn file_stem(path: &str) -> &str {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    file.rsplit_once('.').map_or(file, |(stem, _)| stem)
}
