//! The Hierarchy panel: everything on the open map in groups (portals, NPCs, spawn areas,
//! effects, objects, sounds, layers). Clicking a row selects it and shows it in the Scene;
//! the "+" menu creates new things where the Scene view looks, like Unity's Create menu.

use crate::app::{Studio, Workspace};
use crate::icons;
use crate::map_view::Dropped;
use crate::selection::Selection;
use crate::theme;
use c3_edit::SceneCommand;
use c3_scene::{CellPos, Entity, EntityKind, Npc, Placement, Spawn};
use eframe::egui::{self, RichText};

/// Most effect names the create menu lists at once (narrow down with its search).
const MAX_LISTED: usize = 200;
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
        // What is dropped on the Hierarchy lands in the middle of the Scene view.
        let at = doc.world_to_cell(self.cam.center);
        match Dropped::released(ui, ui.max_rect()) {
            Some(Dropped::Asset(key)) => return self.place_asset(&key, at),
            Some(Dropped::Row(row)) => return self.place_row(&row, at),
            None => {}
        }
        let name = if doc.is_dirty() {
            format!("{}*", doc.name())
        } else {
            doc.name().to_owned()
        };
        let mut picked = None;
        let mut picked_layer = None;
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
        let selected = self.editor.selected();
        // A new selection (picked in the Scene) opens its group and scrolls to its row once.
        let reveal = selected.filter(|s| self.revealed != Some(*s));
        self.revealed = selected;
        let tree = Tree {
            selected: self.editor.selection(),
            hidden: self.editor.hidden(),
            locked: self.editor.locked(),
            reveal,
        };
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let portals: Vec<Row> = scene
                    .portals
                    .iter()
                    .map(|p| {
                        let mut label = format!("Portal {}   {}, {}", p.id, p.cell.0, p.cell.1);
                        if let Some(to) = p.to {
                            let map = self.map_list.describe(to.map);
                            label.push_str(&format!("   {}  {map}", icons::ARROW_RIGHT));
                        }
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
                                EntityKind::Effect { name, .. } => (0, name.clone()),
                                EntityKind::Sprite { .. } => (1, c3_edit::entity_label(e)),
                                EntityKind::Prefab { asset } => (1, file_stem(asset).to_owned()),
                                EntityKind::Sound { asset, .. } => (2, file_stem(asset).to_owned()),
                            };
                            (kind == keep).then_some((name, Selection::Entity(i)))
                        })
                        .collect();
                    tree.group(ui, icon, group, &rows, &mut picked);
                }
                let header =
                    theme::icon_text(icons::STACK, &format!("Layers ({})", scene.layers.len()));
                egui::CollapsingHeader::new(header)
                    .id_salt("Layers")
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                            for (i, l) in scene.layers.iter().enumerate() {
                                let label = format!(
                                    "Layer {} · scroll {}% · {} items",
                                    l.order,
                                    l.scroll_rate.0,
                                    l.items.len()
                                );
                                let on = self.editor.layer == Some(i);
                                if ui.add(egui::Button::selectable(on, label)).clicked() {
                                    picked_layer = Some(i);
                                }
                            }
                        });
                    });
            });
        if let Some(i) = picked_layer {
            self.editor.select(None);
            self.editor.layer = Some(i);
            self.inspect_asset = false;
        }
        if let Some(selection) = picked {
            self.editor.select(Some(selection));
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
        if self.npc_looks.is_empty() {
            if ui.button(theme::icon_text(icons::USER, "NPC")).clicked() {
                self.create(SceneCommand::AddNpc { npc: new_npc(at) });
                ui.close();
            }
        } else {
            ui.menu_button(theme::icon_text(icons::USER, "NPC"), |ui| {
                if let Some(look) = self.look_list(ui) {
                    let mut npc = new_npc(at);
                    npc.look = look;
                    self.create(SceneCommand::AddNpc { npc });
                    ui.close();
                }
            });
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
        if ui
            .button(theme::icon_text(icons::STACK, "Layer"))
            .on_hover_text("A parallax layer: sky, water or far scenery behind the map")
            .clicked()
        {
            self.add_layer();
            ui.close();
        }
        ui.menu_button(theme::icon_text(icons::SPARKLE, "Effect"), |ui| {
            self.effect_picker(ui, at);
        });
        let object = (self.browser.selected.as_ref()).and_then(|key| self.asset_entity(key, at));
        let hint = match &object {
            Some(_) => "Place the asset selected in the Project panel",
            None => {
                "Select an image, sprite set, terrain object, sound or effect in the Project panel (or drag one onto the map)"
            }
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
        let names: Vec<&str> = self.effect_names.iter().map(String::as_str).collect();
        let picked = search_list(ui, &mut self.effect_search, "Search effects", &names)?;
        Some(names[picked].to_owned())
    }

    /// Searchable list of the game's NPC looks; returns the look clicked.
    pub(crate) fn look_list(&mut self, ui: &mut egui::Ui) -> Option<u32> {
        let labels: Vec<String> = (self.npc_looks.iter())
            .map(|(look, name)| format!("{look}  {name}"))
            .collect();
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        let picked = search_list(ui, &mut self.look_search, "Search looks", &labels)?;
        Some(self.npc_looks[picked].0)
    }

    /// Runs a create command and selects what it made.
    pub(crate) fn create(&mut self, command: SceneCommand) {
        let Some(doc) = &mut self.doc else { return };
        let census = crate::selection::Census::of(doc.scene());
        match doc.execute(&command) {
            Ok(outcome) => {
                self.editor.select_all(census.added(doc.scene()));
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
        script: String::new(),
        extra: Default::default(),
    }
}

pub(crate) fn new_spawn(at: CellPos) -> Spawn {
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
struct Tree<'a> {
    selected: &'a [Selection],
    hidden: &'a std::collections::HashSet<Selection>,
    locked: &'a std::collections::HashSet<Selection>,
    /// Selection to open the group of and scroll to (it just changed).
    reveal: Option<Selection>,
}

impl Tree<'_> {
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
                        let on = self.selected.contains(selection);
                        let hidden = self.hidden.contains(selection);
                        let mut text = String::new();
                        if hidden {
                            text.push_str(icons::EYE_SLASH);
                            text.push(' ');
                        }
                        if self.locked.contains(selection) {
                            text.push_str(icons::LOCK);
                            text.push(' ');
                        }
                        text.push_str(label);
                        let mut text = RichText::new(text);
                        if hidden {
                            text = text.color(theme::TEXT_DIM);
                        }
                        let row = ui
                            .add(
                                egui::Button::selectable(on, text)
                                    .wrap_mode(egui::TextWrapMode::Truncate),
                            )
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

/// A search box over `labels` and the matches as buttons (the first few hundred); returns
/// the index of the one clicked.
fn search_list(
    ui: &mut egui::Ui,
    search: &mut String,
    placeholder: &str,
    labels: &[&str],
) -> Option<usize> {
    ui.add(
        egui::TextEdit::singleline(search)
            .hint_text(theme::icon_text(icons::MAGNIFYING_GLASS, placeholder))
            .desired_width(220.0),
    );
    let needle = search.to_lowercase();
    let matches: Vec<usize> = (0..labels.len())
        .filter(|&i| labels[i].to_lowercase().contains(&needle))
        .take(MAX_LISTED + 1)
        .collect();
    if matches.is_empty() {
        ui.label(RichText::new("Nothing matches").color(theme::TEXT_DIM));
        return None;
    }
    let mut chosen = None;
    egui::ScrollArea::vertical()
        .max_height(320.0)
        .show(ui, |ui| {
            for &i in matches.iter().take(MAX_LISTED) {
                if ui.button(labels[i]).clicked() {
                    chosen = Some(i);
                }
            }
        });
    if matches.len() > MAX_LISTED {
        ui.label(
            RichText::new(format!(
                "Showing the first {MAX_LISTED}: search to narrow down"
            ))
            .color(theme::TEXT_DIM)
            .small(),
        );
    }
    chosen
}
