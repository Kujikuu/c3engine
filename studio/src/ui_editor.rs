//! The Game UI panel: the game's dialogs (TQ's model: a `GUI.ini`-form layout, an
//! `Ani`-form skin and the controls file) drawn as the game draws them, with sample values.
//! Click a control to select it, drag to move it, and edit its kind, image, binding and
//! action on the right. Each change is one undo step; the files are patched in place (their
//! comments stay) and written into the project's assets, which the game reads first, so a
//! client used in place is never written.

use crate::app::{Focus, Studio};
use crate::icons;
use crate::theme;
use crate::value_form::{Touch, field, grid, pick};
use c3_core::settings::UiSettings;
use c3_edit::ValueDoc;
use c3_play::ui_textures::GameTextures;
use c3_ui::gui::{self, Control, ControlKind, Gui, GuiData, GuiRect, ini};
use eframe::egui::{self, Color32, DragValue, Pos2, Rect, RichText, Vec2};

/// The UI files as edited: their bytes, patched in place.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UiFiles {
    layout: Vec<u8>,
    skin: Vec<u8>,
    controls: Vec<u8>,
}

/// Which control: `(dialog, id)`; a dialog is `(0, id)`.
type Key = (u32, u32);

/// The game UI open for editing.
pub(crate) struct UiEditor {
    paths: UiSettings,
    doc: ValueDoc<UiFiles>,
    /// The UI read from the files (read again after each change).
    gui: Gui,
    /// The dialog in view.
    dialog: Option<u32>,
    selected: Option<Key>,
    textures: GameTextures,
    /// A selected control being dragged: where it started and the pointer then.
    drag: Option<(GuiRect, Pos2)>,
}

/// Bindings the game reads, for the pickers.
const NUMBERS: [&str; 11] = [
    "hero.level",
    "hero.life",
    "hero.life_max",
    "hero.mana",
    "hero.mana_max",
    "hero.exp",
    "hero.exp_next",
    "hero.money",
    "hero.running",
    "hero.bag_count",
    "hero.bag_size",
];
const LISTS: [&str; 2] = ["hero.bag", "hero.equipment"];
/// Actions the game carries out (besides `toggle:<dialog>`).
const ACTIONS: [&str; 6] = ["bag", "quests", "menu", "chat", "sound", "run"];

const KINDS: [(ControlKind, &str); 7] = [
    (ControlKind::Dialog, "dialog"),
    (ControlKind::Image, "image"),
    (ControlKind::Button, "button"),
    (ControlKind::Check, "check"),
    (ControlKind::Progress, "progress"),
    (ControlKind::Text, "text"),
    (ControlKind::Grid, "grid"),
];

fn kind_name(kind: ControlKind) -> &'static str {
    KINDS
        .iter()
        .find(|(k, _)| *k == kind)
        .map_or("image", |(_, n)| n)
}

/// Sample values the editor draws bindings with.
struct Sample;

impl GuiData for Sample {
    fn number(&self, key: &str) -> Option<f64> {
        Some(match key {
            "hero.level" => 12.0,
            "hero.life" => 750.0,
            "hero.life_max" | "hero.mana_max" | "hero.exp_next" => 1000.0,
            "hero.mana" => 400.0,
            "hero.exp" => 620.0,
            "hero.money" => 12345.0,
            "hero.running" => 1.0,
            "hero.bag_count" => 3.0,
            "hero.bag_size" => 40.0,
            _ => return None,
        })
    }

    fn text(&self, key: &str) -> Option<String> {
        match key {
            "hero.name" => Some("Hero".into()),
            _ => self.number(key).map(|n| format!("{n}")),
        }
    }
}

fn section(key: Key) -> String {
    format!("{}-{}", key.0, key.1)
}

impl UiFiles {
    fn gui(&self) -> Gui {
        Gui::new(&self.layout, &self.skin, &self.controls)
    }

    /// Moves or sizes `key`: in the layout when it places it, else in the controls file.
    fn set_rect(&mut self, key: Key, r: GuiRect) {
        let name = section(key);
        let placed = ini::parse(&self.layout).iter().any(|s| s.name == name);
        let file = if placed {
            &mut self.layout
        } else {
            &mut self.controls
        };
        for (k, v) in [("x", r.x), ("y", r.y), ("w", r.w), ("h", r.h)] {
            let value = (v >= 0 || k == "x" || k == "y").then(|| v.to_string());
            *file = ini::set(file, &name, k, value.as_deref());
        }
    }

    /// Sets a key of `key`'s controls entry (`None` or empty: removed).
    fn set_key(&mut self, key: Key, name: &str, value: &str) {
        let value = (!value.is_empty()).then_some(value);
        self.controls = ini::set(&self.controls, &section(key), name, value);
    }

    fn remove(&mut self, key: Key) {
        self.controls = ini::remove(&self.controls, &section(key));
        self.layout = ini::remove(&self.layout, &section(key));
    }
}

impl Studio {
    /// The game's UI settings: the applied ones, else the game's.
    fn ui_settings(&self) -> Option<UiSettings> {
        let game = self.game.as_ref()?;
        let settings = (self.applied_settings.clone()).unwrap_or_else(|| game.default_settings());
        Some(settings.ui)
    }

    /// Opens the game's UI files when the project has them (again after it changed).
    fn ensure_ui_editor(&mut self) {
        let Some(paths) = self.ui_settings() else {
            self.ui_editor = None;
            return;
        };
        if self.ui_editor.as_ref().is_some_and(|e| e.paths == paths) || !paths.from_data() {
            if !paths.from_data() {
                self.ui_editor = None;
            }
            return;
        }
        let Some(game) = &self.game else { return };
        let read = |path: &str| game.read_file(path).unwrap_or_default();
        let files = UiFiles {
            layout: read(&paths.layout),
            skin: read(&paths.skin),
            controls: read(&paths.controls),
        };
        let gui = files.gui();
        let dialog = gui.dialogs().next().map(|d| d.id);
        self.ui_editor = Some(UiEditor {
            paths,
            gui,
            doc: ValueDoc::new(files),
            dialog,
            selected: dialog.map(|d| (0, d)),
            textures: GameTextures::new(std::rc::Rc::clone(game)),
            drag: None,
        });
    }

    pub(crate) fn ui_panel(&mut self, ui: &mut egui::Ui) {
        self.ensure_ui_editor();
        let Some(mut editor) = self.ui_editor.take() else {
            theme::empty_state(
                ui,
                icons::APP_WINDOW,
                "No game UI from data",
                "Name a layout, skin and controls file in Project Settings > Game UI.",
            );
            return;
        };
        let mut files = editor.doc.value().clone();
        let mut touch = Touch::default();
        let mut label = "Edit the game UI";
        toolbar(ui, &mut editor, &mut files, &mut touch, &mut label);
        egui::Panel::right("ui inspector")
            .resizable(true)
            .default_size(270.0)
            .size_range(220.0..=420.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        inspector(ui, &editor, &mut files, &mut touch, &mut label)
                    });
            });
        egui::Panel::left("ui controls")
            .resizable(true)
            .default_size(200.0)
            .size_range(150.0..=360.0)
            .show(ui, |ui| control_list(ui, &mut editor));
        egui::CentralPanel::default().show(ui, |ui| {
            canvas(ui, &mut editor, &mut files, &mut touch, &mut label)
        });
        self.ui_editor = Some(editor);
        self.apply_ui_edit(files, &touch, label);
    }

    /// Records `files` (one step per drag or field visit) and writes what changed.
    fn apply_ui_edit(&mut self, files: UiFiles, touch: &Touch, label: &str) {
        let Some(editor) = &mut self.ui_editor else {
            return;
        };
        if touch.started {
            editor.doc.begin_group(label);
            self.focus = Focus::Ui;
        }
        let before = editor.doc.value().clone();
        let changed = touch.changed && editor.doc.set(files, label).changed;
        if touch.finished {
            editor.doc.end_group();
        }
        if changed {
            self.write_ui(&before);
        }
    }

    /// Writes the files that differ from `before` into the project's assets.
    fn write_ui(&mut self, before: &UiFiles) {
        let (Some(editor), Some(project)) = (&mut self.ui_editor, &self.project) else {
            return;
        };
        let now = editor.doc.value().clone();
        editor.gui = now.gui();
        let dir = project.assets_dir();
        let changed = [
            (&editor.paths.layout, &now.layout, &before.layout),
            (&editor.paths.skin, &now.skin, &before.skin),
            (&editor.paths.controls, &now.controls, &before.controls),
        ];
        for (path, bytes, old) in changed {
            if bytes == old || path.is_empty() {
                continue;
            }
            let file = dir.join(path);
            let written = (file.parent().map_or(Ok(()), std::fs::create_dir_all))
                .and_then(|()| std::fs::write(&file, bytes));
            if let Err(e) = written {
                self.error = Some(format!("Can't write {path}: {e}"));
                return;
            }
            if let Some(native) = self.game.as_ref().and_then(|g| g.native()) {
                let _ = native.reload(path);
            }
        }
    }

    /// Undoes (or redoes) the last UI edit; returns its label.
    pub(crate) fn ui_step(&mut self, redo: bool) -> Option<String> {
        let editor = self.ui_editor.as_mut()?;
        let before = editor.doc.value().clone();
        let label = if redo {
            editor.doc.redo()
        } else {
            editor.doc.undo()
        };
        if label.is_some() {
            self.write_ui(&before);
        }
        label
    }

    pub(crate) fn ui_history(&self) -> Option<(String, Vec<c3_edit::HistoryEntry>)> {
        let editor = self.ui_editor.as_ref()?;
        Some(("Game UI".to_owned(), editor.doc.history()))
    }

    pub(crate) fn ui_labels(&self) -> (Option<String>, Option<String>) {
        let doc = self.ui_editor.as_ref().map(|e| &e.doc);
        (
            doc.and_then(|d| d.undo_label()).map(str::to_owned),
            doc.and_then(|d| d.redo_label()).map(str::to_owned),
        )
    }
}

/// The dialog picker and the add / remove buttons.
fn toolbar(
    ui: &mut egui::Ui,
    editor: &mut UiEditor,
    files: &mut UiFiles,
    touch: &mut Touch,
    label: &mut &'static str,
) {
    ui.horizontal(|ui| {
        let shown = editor
            .dialog
            .map_or("none".to_owned(), |d| format!("Dialog {d}"));
        egui::ComboBox::from_id_salt("ui dialog")
            .selected_text(shown)
            .width(140.0)
            .show_ui(ui, |ui| {
                let dialogs: Vec<u32> = editor.gui.dialogs().map(|d| d.id).collect();
                for id in dialogs {
                    let count = (editor.gui.controls.iter())
                        .filter(|c| c.parent == id)
                        .count();
                    let text = format!("Dialog {id}  ({count})");
                    if ui
                        .selectable_label(editor.dialog == Some(id), text)
                        .clicked()
                    {
                        editor.dialog = Some(id);
                        editor.selected = Some((0, id));
                    }
                }
            });
        if ui
            .button(theme::icon_text(icons::PLUS, "Dialog"))
            .on_hover_text("A new dialog, open from the start")
            .clicked()
        {
            let id = editor.gui.dialogs().map(|d| d.id).max().unwrap_or(0) + 1;
            files.set_key((0, id), "type", "dialog");
            files.set_key((0, id), "open", "always");
            files.set_rect(
                (0, id),
                GuiRect {
                    x: 100,
                    y: 100,
                    w: 300,
                    h: 200,
                },
            );
            editor.dialog = Some(id);
            editor.selected = Some((0, id));
            touch.click();
            *label = "Add a dialog";
        }
        if let Some(dialog) = editor.dialog {
            ui.menu_button(theme::icon_text(icons::PLUS, "Control"), |ui| {
                for (kind, name) in &KINDS[1..] {
                    if ui.button(*name).clicked() {
                        let id = (editor.gui.controls.iter())
                            .filter(|c| c.parent == dialog)
                            .map(|c| c.id)
                            .max()
                            .unwrap_or(0)
                            + 1;
                        files.set_key((dialog, id), "type", name);
                        let size = match kind {
                            ControlKind::Grid => (160, 32),
                            ControlKind::Progress => (120, 12),
                            _ => (60, 20),
                        };
                        files.set_rect(
                            (dialog, id),
                            GuiRect {
                                x: 10,
                                y: 10,
                                w: size.0,
                                h: size.1,
                            },
                        );
                        editor.selected = Some((dialog, id));
                        touch.click();
                        *label = "Add a control";
                        ui.close();
                    }
                }
            });
        }
        if let Some(key) = editor.selected
            && ui
                .button(icons::TRASH)
                .on_hover_text("Remove the selected control (a dialog: with its controls)")
                .clicked()
        {
            if key.0 == 0 {
                let inside: Vec<Key> = (editor.gui.controls.iter())
                    .filter(|c| c.parent == key.1)
                    .map(|c| (c.parent, c.id))
                    .collect();
                for k in inside {
                    files.remove(k);
                }
                editor.dialog = None;
            }
            files.remove(key);
            editor.selected = editor.dialog.map(|d| (0, d));
            touch.click();
            *label = "Remove a control";
        }
    });
    ui.separator();
}

/// The dialog's controls, in drawing order.
fn control_list(ui: &mut egui::Ui, editor: &mut UiEditor) {
    let Some(dialog) = editor.dialog else {
        return;
    };
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                let rows = std::iter::once((0, dialog)).chain(
                    (editor.gui.controls.iter())
                        .filter(|c| c.parent == dialog)
                        .map(|c| (c.parent, c.id)),
                );
                for key in rows.collect::<Vec<_>>() {
                    let Some(control) = find(&editor.gui, key) else {
                        continue;
                    };
                    let text = format!("{}  {}  {}", key.1, kind_name(control.kind), control.ani);
                    let row = if key.0 == 0 {
                        RichText::new(text).strong()
                    } else {
                        RichText::new(text)
                    };
                    if ui
                        .selectable_label(editor.selected == Some(key), row)
                        .clicked()
                    {
                        editor.selected = Some(key);
                    }
                }
            });
        });
}

fn find(gui: &Gui, key: Key) -> Option<&Control> {
    (gui.controls.iter()).find(|c| (c.parent, c.id) == key)
}

/// The screen as the game draws it: dialogs open from the start and the one in view.
fn canvas(
    ui: &mut egui::Ui,
    editor: &mut UiEditor,
    files: &mut UiFiles,
    touch: &mut Touch,
    label: &mut &'static str,
) {
    let (full, response) =
        ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
    let [bw, bh] = editor.gui.layout.base().map(|v| v as f32);
    let scale = (full.width() / bw).min(full.height() / bh);
    let area = Rect::from_center_size(full.center(), Vec2::new(bw, bh) * scale);
    let painter = ui.painter_at(full);
    painter.rect_filled(full, 0.0, Color32::from_gray(24));
    painter.rect_filled(area, 0.0, Color32::from_rgb(46, 58, 40));
    painter.text(
        area.left_top() + Vec2::new(6.0, 4.0),
        egui::Align2::LEFT_TOP,
        format!("{bw} × {bh}"),
        egui::FontId::proportional(11.0),
        theme::TEXT_DIM,
    );
    let gui = editor.gui.clone();
    let shown: Vec<&Control> = (gui.dialogs())
        .filter(|d| d.open.eq_ignore_ascii_case("always") || Some(d.id) == editor.dialog)
        .collect();
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
    for dialog in &shown {
        let place = gui.place(dialog, area);
        gui.paint_dialog(
            &mut child,
            dialog,
            place,
            &Sample,
            &mut editor.textures,
            false,
        );
    }
    // Outlines: the dialog in view, the hovered control and the selected one.
    let Some(dialog) = editor.dialog.and_then(|d| find(&gui, (0, d))) else {
        return;
    };
    let place = gui.place(dialog, area);
    let ctx = ui.ctx().clone();
    let mut bounds = |c: &Control| gui.bounds(&ctx, c, place, &mut editor.textures);
    let dialog_rect = bounds(dialog);
    painter.rect_stroke(
        dialog_rect,
        0.0,
        egui::Stroke::new(1.0, theme::TEXT_DIM),
        egui::StrokeKind::Outside,
    );
    let pointer = response.hover_pos();
    let controls: Vec<&Control> = (gui.controls.iter())
        .filter(|c| c.parent == dialog.id)
        .collect();
    // The topmost control at a point, else the dialog.
    let mut under_at = |p: Pos2| {
        (controls.iter().rev())
            .find(|c| bounds(c).contains(p))
            .map(|c| (c.parent, c.id))
            .or_else(|| dialog_rect.contains(p).then_some((0, dialog.id)))
    };
    let under = pointer.and_then(&mut under_at);
    // Clicks and drags pick what was under the press (a drag starts after the pointer moved).
    let press = ui.input(|i| i.pointer.press_origin());
    let pressed = press.and_then(&mut under_at);
    if let Some(key) = under
        && Some(key) != editor.selected
        && let Some(c) = find(&gui, key)
    {
        let r = bounds(c);
        painter.rect_stroke(
            r,
            0.0,
            egui::Stroke::new(1.0, Color32::from_white_alpha(90)),
            egui::StrokeKind::Outside,
        );
    }
    if let Some(c) = editor.selected.and_then(|k| find(&gui, k)) {
        let r = bounds(c);
        painter.rect_stroke(
            r,
            0.0,
            egui::Stroke::new(2.0, theme::ACCENT),
            egui::StrokeKind::Outside,
        );
    }
    if response.clicked() || response.drag_started() {
        editor.selected = pressed.or(editor.selected);
    }
    // Dragging moves what was pressed (a control or the dialog) in layout pixels.
    if response.drag_started()
        && let (Some(key), Some(p)) = (pressed, press)
        && let Some(c) = find(&gui, key)
    {
        editor.drag = Some((gui.rect(c), p));
        touch.started = true;
    }
    if let (Some((from, grab)), Some(key), Some(p)) = (
        editor.drag,
        editor.selected,
        response.interact_pointer_pos(),
    ) {
        let delta = (p - grab) / place.scale;
        let moved = GuiRect {
            x: from.x + delta.x.round() as i32,
            y: from.y + delta.y.round() as i32,
            ..from
        };
        if moved != gui.rect(find(&gui, key).unwrap_or(dialog)) {
            files.set_rect(key, moved);
            touch.changed = true;
            *label = "Move a control";
        }
    }
    if response.drag_stopped() && editor.drag.take().is_some() {
        touch.finished = true;
    }
}

/// The selected control's fields.
fn inspector(
    ui: &mut egui::Ui,
    editor: &UiEditor,
    files: &mut UiFiles,
    touch: &mut Touch,
    label: &mut &'static str,
) {
    let Some(key) = editor.selected else {
        ui.label(RichText::new("Select a control on the screen.").color(theme::TEXT_DIM));
        return;
    };
    let gui = &editor.gui;
    let Some(control) = find(gui, key) else {
        return;
    };
    // A one-click change, applied after the fields.
    let mut edit: Option<(&'static str, String)> = None;
    let title = if key.0 == 0 {
        format!("Dialog {}", key.1)
    } else {
        format!("Control {} of dialog {}", key.1, key.0)
    };
    theme::section(ui, &theme::icon_text(icons::APP_WINDOW, &title), |ui| {
        grid(ui, "ui control", |ui| {
            if key.0 != 0 {
                field(ui, "Kind", |ui| {
                    let kinds: Vec<String> =
                        KINDS[1..].iter().map(|(_, n)| (*n).to_owned()).collect();
                    if let Some(kind) = pick(ui, "kind", kind_name(control.kind), &kinds) {
                        edit = Some(("type", (kind).to_string()));
                        *label = "Change a control's kind";
                    }
                });
            }
            field(ui, "Image", |ui| {
                let mut sections = editor.skin_sections();
                sections.insert(0, String::new());
                if let Some(ani) = pick(ui, "ani", &control.ani, &sections) {
                    edit = Some(("ani", (ani).to_string()));
                    *label = "Change a control's image";
                }
            });
            let frames = gui.skin.frames(&control.ani).len();
            if frames > 1
                && matches!(
                    control.kind,
                    ControlKind::Dialog | ControlKind::Image | ControlKind::Progress
                )
            {
                field(ui, "Frame", |ui| {
                    let mut frame = control.frame;
                    if ui
                        .add(DragValue::new(&mut frame).range(0..=frames - 1))
                        .changed()
                    {
                        edit = Some(("frame", frame.to_string()));
                        *label = "Change a control's frame";
                    }
                });
            }
            match control.kind {
                ControlKind::Progress => {
                    binding(ui, "Value", "value", &control.value, &NUMBERS, &mut edit);
                    binding(ui, "Full at", "max", &control.max, &NUMBERS, &mut edit);
                    field(ui, "Fills", |ui| {
                        let fills = ["up", "right", "left"].map(str::to_owned);
                        let now = match control.fill {
                            gui::Fill::Up => "up",
                            gui::Fill::Right => "right",
                            gui::Fill::Left => "left",
                        };
                        if let Some(fill) = pick(ui, "fill", now, &fills) {
                            edit = Some(("fill", (fill).to_string()));
                        }
                    });
                }
                ControlKind::Check => {
                    binding(
                        ui,
                        "Checked by",
                        "value",
                        &control.value,
                        &NUMBERS,
                        &mut edit,
                    );
                }
                ControlKind::Grid => {
                    binding(ui, "Items", "value", &control.value, &LISTS, &mut edit);
                    field(ui, "Columns", |ui| {
                        let mut cols = control.cols;
                        if ui.add(DragValue::new(&mut cols).range(1..=40)).changed() {
                            edit = Some(("cols", cols.to_string()));
                        }
                    });
                    field(ui, "Rows", |ui| {
                        let mut rows = control.rows;
                        if ui.add(DragValue::new(&mut rows).range(1..=40)).changed() {
                            edit = Some(("rows", rows.to_string()));
                        }
                    });
                }
                ControlKind::Text => {
                    field(ui, "Text", |ui| {
                        let mut text = control.text.clone();
                        let r = ui
                            .add(egui::TextEdit::singleline(&mut text).desired_width(170.0))
                            .on_hover_text("{hero.level} shows a binding");
                        if r.changed() {
                            files.set_key(key, "text", &text);
                            touch.changed = true;
                        }
                        touch.started |= r.gained_focus();
                        touch.finished |= r.lost_focus();
                    });
                    field(ui, "Colour", |ui| {
                        let [r, g, b] = control.color;
                        let mut color = Color32::from_rgb(r, g, b);
                        if ui.color_edit_button_srgba(&mut color).changed() {
                            let hex =
                                format!("{:02x}{:02x}{:02x}", color.r(), color.g(), color.b());
                            edit = Some(("color", (hex).to_string()));
                        }
                    });
                    field(ui, "Size", |ui| {
                        let mut size = control.size as u32;
                        if ui.add(DragValue::new(&mut size).range(6..=72)).changed() {
                            edit = Some(("size", size.to_string()));
                        }
                    });
                }
                _ => {}
            }
            if matches!(control.kind, ControlKind::Button | ControlKind::Check) {
                field(ui, "Click", |ui| {
                    let mut actions: Vec<String> = std::iter::once(String::new())
                        .chain(ACTIONS.iter().map(|a| (*a).to_owned()))
                        .collect();
                    actions.extend(gui.dialogs().map(|d| format!("toggle:{}", d.id)));
                    if let Some(action) = pick(ui, "action", &control.action, &actions) {
                        edit = Some(("action", (action).to_string()));
                        *label = "Change what a click does";
                    }
                });
            }
            if key.0 == 0 {
                field(ui, "Open", |ui| {
                    let mut always = control.open.eq_ignore_ascii_case("always");
                    if ui
                        .checkbox(&mut always, "From the start")
                        .on_hover_text("Else a toggle:<id> action opens it")
                        .changed()
                    {
                        edit = Some(("open", (if always { "always" } else { "" }).to_string()));
                    }
                });
            }
        });
    });
    if let Some((name, value)) = edit {
        files.set_key(key, name, &value);
        touch.click();
    }
    theme::section(ui, &theme::icon_text(icons::BOUNDING_BOX, "Place"), |ui| {
        let mut r = gui.rect(control);
        let mut changed = false;
        grid(ui, "ui place", |ui| {
            for (name, value, low) in [
                ("X", &mut r.x, -4096),
                ("Y", &mut r.y, -4096),
                ("Width", &mut r.w, -1),
                ("Height", &mut r.h, -1),
            ] {
                // A size of -1 is none (the picture's own).
                let range = low..=4096;
                field(ui, name, |ui| {
                    let resp = touch.track(ui.add(DragValue::new(value).range(range)));
                    changed |= resp.changed();
                });
            }
        });
        if changed {
            files.set_rect(key, r);
            *label = "Move a control";
        }
        let placed_by = if ini::parse(&files.layout)
            .iter()
            .any(|s| s.name == section(key))
        {
            "The layout places it."
        } else {
            "The controls file places it (the layout has no section)."
        };
        ui.label(RichText::new(placed_by).small().color(theme::TEXT_DIM));
    });
}

/// A binding field: a picked one, or typed.
fn binding(
    ui: &mut egui::Ui,
    name: &str,
    key: &'static str,
    current: &str,
    known: &[&str],
    edit: &mut Option<(&'static str, String)>,
) {
    field(ui, name, |ui| {
        let options: Vec<String> = std::iter::once(String::new())
            .chain(known.iter().map(|k| (*k).to_owned()))
            .collect();
        if let Some(picked) = pick(ui, key, current, &options) {
            *edit = Some((key, picked));
        }
    });
}

impl UiEditor {
    /// The skin's sections, sorted.
    fn skin_sections(&self) -> Vec<String> {
        let mut sections: Vec<String> = ini::parse(&self.doc.value().skin)
            .into_iter()
            .map(|s| s.name)
            .collect();
        sections.sort();
        sections.dedup();
        sections
    }
}
