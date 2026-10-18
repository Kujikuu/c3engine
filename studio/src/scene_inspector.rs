//! Inspector for the Scene selection (portal, object, NPC, spawn area) and the map's
//! Population section. Every field edit is a command; one drag or one text-field visit is one
//! undo step.

use crate::app::Studio;
use crate::icons;
use crate::selection::Selection;
use crate::theme;
use c3_edit::SceneCommand;
use c3_play::map_list::MapList;
use c3_scene::{CellPos, Entity, EntityKind, Npc, Placement, PortalTarget, Spawn};
use eframe::egui::{self, DragValue, RichText};
use std::collections::BTreeMap;

/// Tracks the widgets of one edit so a drag or a text-field visit becomes one undo step.
#[derive(Default)]
pub(crate) struct Interaction {
    pub changed: bool,
    pub started: bool,
    pub finished: bool,
}

impl Interaction {
    fn track(&mut self, r: egui::Response) -> egui::Response {
        self.changed |= r.changed();
        self.started |= r.drag_started() || r.gained_focus();
        self.finished |= r.drag_stopped() || r.lost_focus();
        r
    }
}

/// A labelled row in the property grid.
pub(crate) fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.label(RichText::new(label).color(theme::TEXT_DIM));
    add(ui);
    ui.end_row();
}

pub(crate) fn grid(ui: &mut egui::Ui, id: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, body);
}

/// Where a portal on `cell` leads: nowhere, or one of the game's maps (the arrival cell
/// kept; a new destination arrives on `cell` until moved).
fn destination_field(
    ui: &mut egui::Ui,
    it: &mut Interaction,
    maps: &MapList,
    (cell, to): (CellPos, &mut Option<PortalTarget>),
) {
    let shown = to.map_or_else(|| "Nowhere".to_owned(), |t| maps.describe(t.map));
    let mut picked = None;
    egui::ComboBox::from_id_salt("portal destination")
        .selected_text(shown)
        .width(170.0)
        .show_ui(ui, |ui| {
            if ui.selectable_label(to.is_none(), "Nowhere").clicked() {
                picked = Some(None);
            }
            for m in maps.iter() {
                let here = to.is_some_and(|t| m.ids.contains(&t.map));
                let label = format!("{}   {}", m.name, m.id());
                if ui
                    .selectable_label(here, label)
                    .on_hover_text(&m.key)
                    .clicked()
                {
                    picked = Some(Some(m.id()));
                }
            }
        });
    if let Some(map) = picked {
        *to = map.map(|map| PortalTarget {
            map,
            cell: to.map_or(cell, |t| t.cell),
        });
        it.changed = true;
    }
}

/// Two drag fields for a cell.
pub(crate) fn cell_field(ui: &mut egui::Ui, it: &mut Interaction, cell: &mut CellPos) {
    ui.horizontal(|ui| {
        it.track(ui.add(DragValue::new(&mut cell.0).prefix("x ").speed(0.2)));
        it.track(ui.add(DragValue::new(&mut cell.1).prefix("y ").speed(0.2)));
    });
}

fn text_field(ui: &mut egui::Ui, it: &mut Interaction, text: &mut String) {
    it.track(ui.add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY)));
}

pub(crate) fn number<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    it: &mut Interaction,
    value: &mut T,
) {
    it.track(ui.add(DragValue::new(value).speed(0.2)));
}

/// Game values kept for export, read-only.
fn extra_section(ui: &mut egui::Ui, extra: &BTreeMap<String, String>) {
    if extra.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(RichText::new(format!("Game data ({})", extra.len())).strong())
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                RichText::new("Kept as they were imported and written back on export.")
                    .color(theme::TEXT_DIM)
                    .small(),
            );
            for (k, v) in extra {
                theme::kv(ui, k, v);
            }
        });
}

impl Studio {
    /// Shows the selection's properties; false when nothing is selected.
    pub(crate) fn selection_inspector(&mut self, ui: &mut egui::Ui) -> bool {
        let (Some(doc), Some(selected)) = (&self.doc, self.editor.selected()) else {
            return false;
        };
        let scene = doc.scene();
        if !selected.exists(scene) {
            return false;
        }
        let (icon, kind) = match selected {
            Selection::Portal(_) => (icons::DOOR_OPEN, "Portal"),
            Selection::Npc(_) => (icons::USER, "NPC"),
            Selection::Spawn(_) => (icons::USERS_THREE, "Monster spawn area"),
            Selection::Entity(i) => match scene.entities[i].kind {
                EntityKind::Sprite { .. } => (icons::TREE, "Cover object"),
                EntityKind::Prefab { .. } => (icons::MOUNTAINS, "Terrain object"),
                EntityKind::Sound { .. } => (icons::SPEAKER_HIGH, "Sound source"),
                EntityKind::Effect { .. } => (icons::SPARKLE, "Effect"),
            },
        };
        let title = selected.label(scene);
        let (mut frame, mut delete) = (false, false);
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon).size(22.0).color(theme::ACCENT));
            ui.vertical(|ui| {
                ui.label(RichText::new(&title).size(15.0).strong());
                let others = self.editor.selection().len() - 1;
                let kind = match others {
                    0 => kind.to_owned(),
                    n => format!("{kind} · +{n} selected"),
                };
                ui.label(RichText::new(kind).color(theme::TEXT_DIM).small());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                delete = theme::icon_toggle(ui, false, icons::TRASH, "Delete (Del)").clicked();
                frame = theme::icon_toggle(ui, false, icons::CORNERS_OUT, "Show in the Scene (F)")
                    .clicked();
            });
        });
        self.arrange_bar(ui);
        ui.separator();
        let mut it = Interaction::default();
        let command = match selected {
            Selection::Portal(id) => self.portal_fields(ui, &mut it, id),
            Selection::Entity(index) => self.entity_fields(ui, &mut it, index),
            Selection::Npc(id) => self.npc_fields(ui, &mut it, id),
            Selection::Spawn(id) => self.spawn_fields(ui, &mut it, id),
        };
        self.apply_edit(it, &title, command);
        if frame {
            self.frame_selection();
        }
        if delete {
            self.delete_selection();
        }
        true
    }

    /// Runs an Inspector edit, merging one drag or text-field visit into one undo step.
    pub(crate) fn apply_edit(
        &mut self,
        it: Interaction,
        title: &str,
        command: Option<SceneCommand>,
    ) {
        let Some(doc) = &mut self.doc else { return };
        if it.finished {
            doc.end_group();
        }
        if it.started {
            doc.begin_group(format!("Edit {title}"));
        }
        if let Some(command) = command.filter(|_| it.changed)
            && let Err(e) = doc.execute(&command)
        {
            self.error = Some(format!("Can't change {title}: {e}"));
        }
    }

    fn portal_fields(
        &mut self,
        ui: &mut egui::Ui,
        it: &mut Interaction,
        id: i32,
    ) -> Option<SceneCommand> {
        let doc = self.doc.as_ref()?;
        let portal = doc.scene().portals.iter().find(|p| p.id == id)?;
        let (mut cell, mut to) = (portal.cell, portal.to);
        let mut go = false;
        grid(ui, "portal", |ui| {
            row(ui, "Id", |ui| {
                ui.label(id.to_string())
                    .on_hover_text("The game's data may link this id to a destination");
            });
            row(ui, "Cell", |ui| cell_field(ui, it, &mut cell));
            row(ui, "Leads to", |ui| {
                ui.horizontal(|ui| {
                    destination_field(ui, it, &self.map_list, (cell, &mut to));
                    go = to.is_some()
                        && ui
                            .button(icons::ARROW_SQUARE_OUT)
                            .on_hover_text("Open that map at the arrival")
                            .clicked();
                });
            });
            if let Some(target) = &mut to {
                row(ui, "Arrive at", |ui| cell_field(ui, it, &mut target.cell));
            }
        });
        if go && let Some(target) = to {
            self.go_to_portal_target(target);
            return None;
        }
        if to != portal.to {
            Some(SceneCommand::SetPortalTarget { id, to })
        } else {
            Some(SceneCommand::MovePortal { id, to: cell })
        }
    }

    /// Opens the map a portal leads to, the view on where travellers arrive.
    fn go_to_portal_target(&mut self, to: PortalTarget) {
        let Some(info) = self.map_list.by_id(to.map) else {
            self.error = Some(format!("There is no map {} to open", to.map));
            return;
        };
        let map = crate::maps::MapRef::parse(&info.key);
        self.open_map(&map);
        if let Some(doc) = self.doc.as_ref().filter(|d| d.map == map) {
            self.fit_pending = false;
            self.cam.center = doc.cell_to_world(to.cell);
            let portal = (doc.scene().portals.iter())
                .find(|p| p.cell == to.cell)
                .map(|p| Selection::Portal(p.id));
            self.editor.select(portal);
        }
    }

    fn entity_fields(
        &mut self,
        ui: &mut egui::Ui,
        it: &mut Interaction,
        index: usize,
    ) -> Option<SceneCommand> {
        let doc = self.doc.as_ref()?;
        let mut entity: Entity = doc.scene().entities.get(index)?.clone();
        let mut pick_effect = false;
        grid(ui, "entity", |ui| {
            if let Placement::Cell(cell) = &mut entity.placement {
                row(ui, "Cell", |ui| cell_field(ui, it, cell));
            } else {
                row(ui, "Position", |ui| {
                    ui.label("Placed off the grid (moving puts it on a cell)");
                });
            }
            match &mut entity.kind {
                EntityKind::Sprite {
                    asset,
                    animation,
                    footprint,
                    offset,
                    frame_ms,
                } => {
                    row(ui, "Sprite set", |ui| {
                        ui.label(asset.as_str());
                    });
                    row(ui, "Animation", |ui| text_field(ui, it, animation));
                    row(ui, "Footprint", |ui| {
                        ui.horizontal(|ui| {
                            number(ui, it, &mut footprint.0);
                            ui.label("×");
                            number(ui, it, &mut footprint.1);
                            ui.label("cells");
                        });
                    });
                    row(ui, "Offset", |ui| cell_field(ui, it, offset));
                    row(ui, "Frame time", |ui| {
                        it.track(ui.add(DragValue::new(frame_ms).suffix(" ms").range(1..=10_000)));
                    });
                }
                EntityKind::Prefab { asset } => row(ui, "Terrain object", |ui| {
                    ui.label(asset.as_str());
                }),
                EntityKind::Sound {
                    asset,
                    range,
                    volume,
                } => {
                    row(ui, "Sound", |ui| {
                        ui.label(asset.as_str());
                    });
                    row(ui, "Range", |ui| number(ui, it, range));
                    row(ui, "Volume", |ui| {
                        it.track(ui.add(DragValue::new(volume).range(0..=100).suffix(" %")));
                    });
                }
                EntityKind::Effect { name, transform } => {
                    row(ui, "Effect", |ui| {
                        ui.horizontal(|ui| {
                            text_field(ui, it, name);
                            pick_effect = ui
                                .button(icons::MAGNIFYING_GLASS)
                                .on_hover_text("Choose from the content's effects")
                                .clicked();
                        });
                    });
                    if let Some(t) = transform {
                        row(ui, "Rotation", |ui| {
                            ui.horizontal(|ui| {
                                for a in &mut t.angles {
                                    it.track(ui.add(DragValue::new(a).speed(0.01)));
                                }
                            });
                        });
                        row(ui, "Scale", |ui| {
                            ui.horizontal(|ui| {
                                for s in &mut t.scale {
                                    it.track(ui.add(DragValue::new(s).speed(0.01)));
                                }
                            });
                        });
                    }
                }
            }
        });
        if pick_effect {
            self.effect_search.clear();
            self.picking_effect = true;
        }
        if self.picking_effect {
            let mut open = true;
            let mut chosen = None;
            egui::Window::new("Choose an effect")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| chosen = self.effect_list(ui));
            if let (Some(name), EntityKind::Effect { name: current, .. }) =
                (chosen, &mut entity.kind)
            {
                *current = name;
                it.changed = true;
                open = false;
            }
            self.picking_effect = open;
        }
        Some(SceneCommand::SetEntity { index, entity })
    }

    fn npc_fields(
        &mut self,
        ui: &mut egui::Ui,
        it: &mut Interaction,
        id: u32,
    ) -> Option<SceneCommand> {
        let doc = self.doc.as_ref()?;
        let mut npc: Npc = doc.scene().npcs.iter().find(|n| n.id == id)?.clone();
        let looks = &self.npc_looks;
        grid(ui, "npc", |ui| {
            row(ui, "Id", |ui| {
                ui.label(id.to_string())
                    .on_hover_text("Scripts and the server refer to the NPC by this id");
            });
            row(ui, "Name", |ui| text_field(ui, it, &mut npc.name));
            row(ui, "Cell", |ui| cell_field(ui, it, &mut npc.cell));
            row(ui, "Facing", |ui| {
                it.track(
                    ui.add(DragValue::new(&mut npc.direction).range(0..=7))
                        .on_hover_text("0–7, the eight directions"),
                );
            });
            row(ui, "Look", |ui| {
                ui.horizontal(|ui| {
                    number(ui, it, &mut npc.look);
                    if !looks.is_empty() {
                        match looks.iter().find(|(l, _)| *l == npc.look) {
                            Some((_, name)) => ui.label(RichText::new(name).color(theme::TEXT_DIM)),
                            None => ui.label(RichText::new("no such look").color(theme::WARNING)),
                        };
                    }
                });
            });
            row(ui, "Kind", |ui| {
                number(ui, it, &mut npc.kind);
            });
        });
        let request = self.npc_script_field(ui, it, &mut npc.script);
        extra_section(ui, &npc.extra);
        match request {
            ScriptRequest::None => {}
            ScriptRequest::NewScript => {
                let name = format!("npc/{}", npc_file_name(&npc));
                if self.create_script(&name, &crate::script_panel::npc_template(&npc.name)) {
                    npc.script = name;
                    it.changed = true;
                }
            }
            ScriptRequest::NewDialogue => {
                let name = npc_file_name(&npc);
                if self.create_dialogue(&name, c3_script::DialogueGraph::default()) {
                    npc.script = c3_script::generated_script(&name);
                    it.changed = true;
                }
            }
            ScriptRequest::Edit => match dialogue_of(&npc.script) {
                Some(dialogue) => self.open_dialogue(dialogue),
                None => self.open_script(&npc.script),
            },
        }
        Some(SceneCommand::SetNpc { npc })
    }

    /// The NPC's script: pick one (a dialogue's or a hand-written one), open it, or make one.
    fn npc_script_field(
        &self,
        ui: &mut egui::Ui,
        it: &mut Interaction,
        script: &mut String,
    ) -> ScriptRequest {
        let mut request = ScriptRequest::None;
        let names = &self.scripts.names;
        let runnable = |n: &&String| n.starts_with("npc/") || dialogue_of(n).is_some();
        ui.add_space(4.0);
        grid(ui, "npc script", |ui| {
            row(ui, "Script", |ui| {
                let shown = if script.is_empty() {
                    "None"
                } else {
                    script.as_str()
                };
                egui::ComboBox::from_id_salt("npc script")
                    .selected_text(shown)
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        it.changed |= ui.selectable_value(script, String::new(), "None").changed();
                        for name in names.iter().filter(runnable) {
                            it.changed |= ui.selectable_value(script, name.clone(), name).changed();
                        }
                    })
                    .response
                    .on_hover_text("Runs when a player clicks the NPC: a dialogue's script (dialogue/…) or a hand-written one (npc/…)");
            });
            row(ui, "", |ui| {
                ui.horizontal(|ui| {
                    if script.is_empty() {
                        if ui
                            .button(theme::icon_text(icons::CHATS, "New dialogue"))
                            .on_hover_text("Draw this NPC's dialogue as a graph")
                            .clicked()
                        {
                            request = ScriptRequest::NewDialogue;
                        }
                        if ui
                            .button(theme::icon_text(icons::PLUS, "New script"))
                            .on_hover_text("Write this NPC's dialogue as a script")
                            .clicked()
                        {
                            request = ScriptRequest::NewScript;
                        }
                    } else if names.contains(script) {
                        let (icon, label) = if dialogue_of(script).is_some() {
                            (icons::CHATS, "Edit dialogue")
                        } else {
                            (icons::FILE_CODE, "Edit script")
                        };
                        if ui.button(theme::icon_text(icon, label)).clicked() {
                            request = ScriptRequest::Edit;
                        }
                    } else {
                        ui.label(
                            RichText::new(theme::icon_text(icons::WARNING, "Script file missing"))
                                .color(theme::WARNING),
                        );
                    }
                });
            });
        });
        request
    }

    fn spawn_fields(
        &mut self,
        ui: &mut egui::Ui,
        it: &mut Interaction,
        id: u32,
    ) -> Option<SceneCommand> {
        let doc = self.doc.as_ref()?;
        let mut spawn: Spawn = doc.scene().spawns.iter().find(|s| s.id == id)?.clone();
        grid(ui, "spawn", |ui| {
            row(ui, "Id", |ui| {
                ui.label(id.to_string());
            });
            row(ui, "Monster type", |ui| number(ui, it, &mut spawn.monster));
            row(ui, "From cell", |ui| cell_field(ui, it, &mut spawn.min));
            row(ui, "To cell", |ui| cell_field(ui, it, &mut spawn.max));
            row(ui, "Most alive", |ui| number(ui, it, &mut spawn.max_alive));
            row(ui, "Per wave", |ui| number(ui, it, &mut spawn.per_wave));
            row(ui, "Respawn", |ui| {
                it.track(ui.add(DragValue::new(&mut spawn.respawn_secs).suffix(" s")));
            });
        });
        let (w, h) = (spawn.max.0 - spawn.min.0 + 1, spawn.max.1 - spawn.min.1 + 1);
        ui.label(
            RichText::new(format!("{w} × {h} cells"))
                .color(theme::TEXT_DIM)
                .small(),
        );
        extra_section(ui, &spawn.extra);
        Some(SceneCommand::SetSpawn { spawn })
    }

    /// The map's NPCs and spawn areas: counts, import from server data, export as SQL.
    pub(crate) fn population_section(&mut self, ui: &mut egui::Ui) {
        let Some(doc) = &self.doc else { return };
        let (npcs, spawns) = (doc.scene().npcs.len(), doc.scene().spawns.len());
        let mut import = false;
        let mut export = false;
        theme::section(ui, "Population", |ui| {
            theme::kv(ui, "NPCs", &npcs.to_string());
            theme::kv(ui, "Spawn areas", &spawns.to_string());
            ui.horizontal(|ui| {
                import = ui
                    .button(theme::icon_text(icons::DATABASE, "Import…"))
                    .on_hover_text(
                        "Add the NPCs and spawn areas of this map from a server SQL dump",
                    )
                    .clicked();
                export = ui
                    .add_enabled(
                        npcs + spawns > 0,
                        egui::Button::new(theme::icon_text(icons::UPLOAD_SIMPLE, "Export SQL…")),
                    )
                    .on_hover_text("Write them as SQL for the server's database")
                    .clicked();
            });
            ui.label(
                RichText::new("Create more with + Create in the Hierarchy.")
                    .color(theme::TEXT_DIM)
                    .small(),
            );
        });
        if import {
            self.import_population();
        }
        if export {
            self.export_population();
        }
    }

    fn import_population(&mut self) {
        let start = self.project.as_ref().map(|p| p.root.clone());
        let mut dialog = rfd::FileDialog::new()
            .set_title("Import NPCs and spawn areas from a server SQL dump")
            .add_filter("SQL dump", &["sql"]);
        if let Some(dir) = start {
            dialog = dialog.set_directory(dir);
        }
        if let Some(file) = dialog.pick_file() {
            self.import_population_file(&file);
        }
    }

    /// Adds the open map's NPCs and spawn areas from server SQL dump `file` as one undo step.
    pub(crate) fn import_population_file(&mut self, file: &std::path::Path) {
        let (Some(client), Some(doc)) = (&self.client, &mut self.doc) else {
            return;
        };
        let population = match client.server_population(file, &doc.map) {
            Ok(p) => p,
            Err(e) => {
                self.error = Some(format!("Can't import {}: {e}", file.display()));
                return;
            }
        };
        let mut commands: Vec<SceneCommand> = population
            .npcs
            .into_iter()
            .map(|npc| SceneCommand::AddNpc { npc })
            .collect();
        commands.extend(
            population
                .spawns
                .into_iter()
                .map(|spawn| SceneCommand::AddSpawn { spawn }),
        );
        // Where the map's portals lead, for those it has.
        let portals = &doc.scene().portals;
        commands.extend(
            population
                .links
                .into_iter()
                .filter(|link| portals.iter().any(|p| p.id == link.portal))
                .map(|link| SceneCommand::SetPortalTarget {
                    id: link.portal,
                    to: Some(link.to),
                }),
        );
        if commands.is_empty() {
            self.notify(format!(
                "{} has no NPCs, spawn areas or portal destinations for this map",
                file.display()
            ));
            return;
        }
        let results = doc.execute_batch("Import population", &commands);
        let refused: Vec<String> = results
            .iter()
            .filter_map(|r| r.as_ref().err().map(|e| e.to_string()))
            .collect();
        let added = results.len() - refused.len();
        for why in &refused {
            self.log_error(format!("Import skipped a row: {why}"));
        }
        self.notify(match refused.len() {
            0 => format!("Imported {added} NPCs, spawn areas and portal destinations"),
            n => format!("Imported {added}; skipped {n} (see the Console)"),
        });
    }

    fn export_population(&mut self) {
        let (Some(client), Some(doc)) = (&self.client, &self.doc) else {
            return;
        };
        let name = format!("{}_population.sql", doc.name());
        let Some(file) = rfd::FileDialog::new()
            .set_title("Export NPCs and spawn areas as SQL")
            .set_file_name(&name)
            .add_filter("SQL", &["sql"])
            .save_file()
        else {
            return;
        };
        let sql = client.population_sql(&doc.scene().population(), &doc.map);
        match std::fs::write(&file, sql) {
            Ok(()) => self.notify(format!("Exported to {}", file.display())),
            Err(e) => self.error = Some(format!("Can't write {}: {e}", file.display())),
        }
    }
}

/// Name of a new script for `npc`: `npc/<name>_<id>` (name in lower case, letters and digits).
/// What the NPC script field asked for.
enum ScriptRequest {
    None,
    Edit,
    NewScript,
    NewDialogue,
}

/// The dialogue a script was made from (`dialogue/guard` → `guard`).
fn dialogue_of(script: &str) -> Option<&str> {
    script
        .strip_prefix(c3_script::GENERATED_DIR)
        .and_then(|s| s.strip_prefix('/'))
}

/// A file name for the NPC's script or dialogue (`shopkeeper_3`).
fn npc_file_name(npc: &Npc) -> String {
    let mut name: String = npc
        .name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    name = name.trim_matches('_').to_owned();
    if name.is_empty() {
        format!("npc_{}", npc.id)
    } else {
        format!("{name}_{}", npc.id)
    }
}
