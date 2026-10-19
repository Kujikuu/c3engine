//! Edit > Project Settings: every setting of the game, by section, as in Unity. Each edit is
//! a `c3_edit::SettingsCommand` (one undo step per drag or typed value), saved at once to
//! `settings.toml` and applied to the game playing in the Game view.

use crate::app::{Focus, Studio};
use crate::icons;
use crate::theme;
use c3_core::ProjectSettings;
use c3_core::settings::{
    AttributeDef, Cursor, EquipmentSlot, FrontSettings, ITEM_ATTRIBUTES, MapEntry,
};
use c3_edit::{SettingsCommand, SettingsDoc};
use c3_play::GameModule;
use c3_play::map_list::{self, MapInfo, MapList};
use eframe::egui::{self, DragValue, RichText, TextureHandle};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

mod input;

/// The window's sections, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Player,
    Cursors,
    Camera,
    Input,
    Gameplay,
    Items,
    Maps,
    Graphics,
    Audio,
    Network,
    GameUi,
    Localization,
}

impl Section {
    const ALL: [Self; 12] = [
        Self::Player,
        Self::Cursors,
        Self::Camera,
        Self::Input,
        Self::Gameplay,
        Self::Items,
        Self::Maps,
        Self::Graphics,
        Self::Audio,
        Self::Network,
        Self::GameUi,
        Self::Localization,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Player => "Player",
            Self::Cursors => "Cursors",
            Self::Camera => "Camera",
            Self::Input => "Input",
            Self::Gameplay => "Gameplay",
            Self::Items => "Items",
            Self::Maps => "Maps",
            Self::Graphics => "Graphics",
            Self::Audio => "Audio",
            Self::Network => "Network",
            Self::GameUi => "Game UI",
            Self::Localization => "Localization",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Player => icons::APP_WINDOW,
            Self::Cursors => icons::CURSOR,
            Self::Camera => icons::VIDEO_CAMERA,
            Self::Input => icons::GAME_CONTROLLER,
            Self::Gameplay => icons::SWORD,
            Self::Items => icons::PACKAGE,
            Self::Maps => icons::MAP_TRIFOLD,
            Self::Graphics => icons::PAINT_BRUSH,
            Self::Audio => icons::SPEAKER_HIGH,
            Self::Network => icons::GLOBE_SIMPLE,
            Self::GameUi => icons::APP_WINDOW,
            Self::Localization => icons::TRANSLATE,
        }
    }

    fn about(self) -> &'static str {
        match self {
            Self::Player => "The game as a program: its name, icon and window.",
            Self::Cursors => {
                "The mouse cursor for what it is over: images of the project (PNG, or Windows \
                 .cur/.ani cursors). Empty: the system pointer."
            }
            Self::Camera => "How the game's camera sees the map and follows the hero.",
            Self::Input => {
                "What players press for each action, on keyboard and mouse, gamepads (Xbox, \
                 PlayStation, Switch and others) and touch screens."
            }
            Self::Gameplay => {
                "The simulation's numbers: timing, ranges, where players start and what they \
                 start with, and who may use game-master commands."
            }
            Self::Items => {
                "The bag, the money and other currencies, item qualities and wear, and the \
                 slots items are worn in."
            }
            Self::Maps => {
                "The game's maps: the ids portals, scripts and saves name them by, the names \
                 players see, their music and light, and where players may fight each other. Maps not \
                 listed play too, with ids the engine gives them."
            }
            Self::Graphics => "How the client draws.",
            Self::Audio => "Volumes and how far sounds carry.",
            Self::Network => "The game server: its port, saving, and fronts for other clients.",
            Self::GameUi => {
                "The game's dialogs from data, in TQ's model: a layout placing them (GUI.ini form), a skin of their images (Ani form) and the controls file saying what each control is, shows and does. Empty: the engine's own UI. Edit them in the Game UI panel."
            }
            Self::Localization => {
                "The fonts the game's text is drawn with, in order: each next font draws the \
                 characters the ones before lack (e.g. Chinese)."
            }
        }
    }
}

/// What an edit in the window asks for, carried out after it is drawn.
enum Edit {
    /// Starts one undo step for a drag or a typed value.
    Begin(String),
    Command(SettingsCommand),
    End,
}

/// The Project Settings window and the settings it edits.
pub struct SettingsEditor {
    pub doc: SettingsDoc,
    pub open: bool,
    section: Section,
    /// Text of list fields being typed, by setting path.
    buffers: HashMap<String, String>,
    /// Cursor previews by image path: the first frame, or why it can't be shown.
    previews: HashMap<String, Result<TextureHandle, String>>,
    /// The action waiting for a pressed key (Input page).
    listening: Option<c3_core::input::Action>,
}

impl SettingsEditor {
    pub fn new(settings: ProjectSettings, defaults: ProjectSettings) -> Self {
        Self {
            doc: SettingsDoc::new(settings, defaults),
            open: false,
            section: Section::Player,
            buffers: HashMap::new(),
            listening: None,
            previews: HashMap::new(),
        }
    }

    /// Shows what `old` showed (the window open, on the same page).
    pub fn keep_view(&mut self, old: &Self) {
        self.open = old.open;
        self.section = old.section;
    }

    pub fn settings(&self) -> &ProjectSettings {
        self.doc.settings()
    }
}

/// What the fields read and collect while one section is drawn.
struct Fields<'a> {
    doc: &'a SettingsDoc,
    edits: Vec<Edit>,
    buffers: &'a mut HashMap<String, String>,
    previews: &'a mut HashMap<String, Result<TextureHandle, String>>,
    game: Option<&'a dyn GameModule>,
    /// Where the files the settings name are (paths are relative to it).
    files_root: Option<PathBuf>,
    maps: &'a [String],
    /// What went wrong picking files, for the Studio to show.
    problems: Vec<String>,
}

impl Fields<'_> {
    fn set(&mut self, path: &str, value: Value) {
        self.edits.push(Edit::Command(SettingsCommand::Set {
            path: path.into(),
            value,
        }));
    }

    /// Opens or closes the undo step of a drag or a typed field.
    fn track(&mut self, resp: &egui::Response, label: &str) {
        if resp.drag_started() || resp.gained_focus() {
            self.edits.push(Edit::Begin(format!("Change {label}")));
        }
        if resp.drag_stopped() || resp.lost_focus() {
            self.edits.push(Edit::End);
        }
    }

    /// The row's label, with its hint on hover.
    fn label(ui: &mut egui::Ui, label: &str, hint: &str) {
        ui.label(RichText::new(label).color(theme::TEXT_DIM))
            .on_hover_text(hint);
    }

    /// A button putting `path` back to its default, shown while it differs.
    fn reset(&mut self, ui: &mut egui::Ui, path: &str) {
        if !self.doc.is_default(path)
            && ui
                .add(
                    egui::Button::new(RichText::new(icons::ARROW_COUNTER_CLOCKWISE).small())
                        .frame(false),
                )
                .on_hover_text("Back to the default")
                .clicked()
        {
            self.edits
                .push(Edit::Command(SettingsCommand::Reset { path: path.into() }));
        }
    }

    fn number(
        &mut self,
        ui: &mut egui::Ui,
        (label, hint): (&str, &str),
        path: &str,
        value: f64,
        range: RangeInclusive<f64>,
        suffix: &str,
    ) {
        Self::label(ui, label, hint);
        ui.horizontal(|ui| {
            let mut v = value;
            let speed = ((range.end() - range.start()) / 500.0).max(0.001);
            let resp = ui.add(
                DragValue::new(&mut v)
                    .range(range)
                    .speed(speed)
                    .max_decimals(3)
                    .suffix(suffix),
            );
            self.track(&resp, label);
            if resp.changed() {
                self.set(path, json!(v));
            }
            self.reset(ui, path);
        });
        ui.end_row();
    }

    fn whole(
        &mut self,
        ui: &mut egui::Ui,
        (label, hint): (&str, &str),
        path: &str,
        value: i64,
        range: RangeInclusive<i64>,
        suffix: &str,
    ) {
        Self::label(ui, label, hint);
        ui.horizontal(|ui| {
            let mut v = value;
            let resp = ui.add(
                DragValue::new(&mut v)
                    .range(range)
                    .speed(0.2)
                    .suffix(suffix),
            );
            self.track(&resp, label);
            if resp.changed() {
                self.set(path, json!(v));
            }
            self.reset(ui, path);
        });
        ui.end_row();
    }

    fn check(&mut self, ui: &mut egui::Ui, (label, hint): (&str, &str), path: &str, value: bool) {
        Self::label(ui, label, hint);
        ui.horizontal(|ui| {
            let mut v = value;
            if ui.checkbox(&mut v, "").changed() {
                self.set(path, json!(v));
            }
            self.reset(ui, path);
        });
        ui.end_row();
    }

    fn text(
        &mut self,
        ui: &mut egui::Ui,
        (label, hint): (&str, &str),
        path: &str,
        value: &str,
        placeholder: &str,
    ) {
        Self::label(ui, label, hint);
        ui.horizontal(|ui| {
            let mut v = value.to_owned();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut v)
                    .hint_text(placeholder)
                    .desired_width(240.0),
            );
            self.track(&resp, label);
            if resp.changed() {
                self.set(path, json!(v));
            }
            self.reset(ui, path);
        });
        ui.end_row();
    }

    fn color(
        &mut self,
        ui: &mut egui::Ui,
        (label, hint): (&str, &str),
        path: &str,
        value: [u8; 3],
    ) {
        Self::label(ui, label, hint);
        ui.horizontal(|ui| {
            let mut v = value;
            let resp = ui.color_edit_button_srgb(&mut v);
            if resp.changed() {
                self.set(path, json!(v));
            }
            self.reset(ui, path);
        });
        ui.end_row();
    }

    /// A list of whole numbers typed as `1, 2, 3` (applied while it reads as one).
    fn numbers(
        &mut self,
        ui: &mut egui::Ui,
        (label, hint): (&str, &str),
        path: &str,
        value: &[u32],
    ) {
        let shown = value
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        self.list(
            ui,
            (label, hint),
            path,
            shown,
            "e.g. 410003, 130003",
            |text| {
                text.split(',')
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(|t| t.parse::<u32>().ok().map(Value::from))
                    .collect::<Option<Vec<_>>>()
                    .map(Value::Array)
            },
        );
    }

    /// A list of names typed as `a, b`.
    /// A list of names typed as `a, b, c`; `example` shows while it is empty.
    fn names(
        &mut self,
        ui: &mut egui::Ui,
        (label, hint, example): (&str, &str, &str),
        path: &str,
        value: &[String],
    ) {
        self.list(ui, (label, hint), path, value.join(", "), example, |text| {
            Some(Value::Array(
                text.split(',')
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(Value::from)
                    .collect(),
            ))
        });
    }

    fn list(
        &mut self,
        ui: &mut egui::Ui,
        (label, hint): (&str, &str),
        path: &str,
        shown: String,
        placeholder: &str,
        parse: impl Fn(&str) -> Option<Value>,
    ) {
        Self::label(ui, label, hint);
        ui.horizontal(|ui| {
            let mut text = self.buffers.get(path).cloned().unwrap_or(shown);
            let resp = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .hint_text(placeholder)
                    .desired_width(240.0),
            );
            self.track(&resp, label);
            if resp.changed()
                && let Some(value) = parse(&text)
            {
                self.set(path, value);
            }
            if resp.has_focus() {
                self.buffers.insert(path.to_owned(), text);
            } else {
                self.buffers.remove(path);
            }
            self.reset(ui, path);
        });
        ui.end_row();
    }

    /// A file of the game's content, typed or picked.
    fn file(
        &mut self,
        ui: &mut egui::Ui,
        (label, hint): (&str, &str),
        path: &str,
        value: &str,
        kinds: &[&str],
    ) {
        Self::label(ui, label, hint);
        ui.horizontal(|ui| {
            if let Some(picked) = self.file_field(ui, label, value, kinds) {
                self.set(path, json!(picked));
            }
            self.reset(ui, path);
        });
        ui.end_row();
    }

    /// A path field with a Browse button; returns the new path when it changes.
    fn file_field(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        value: &str,
        kinds: &[&str],
    ) -> Option<String> {
        let mut v = value.to_owned();
        let resp = ui.add(
            egui::TextEdit::singleline(&mut v)
                .hint_text("none")
                .desired_width(200.0),
        );
        self.track(&resp, label);
        let mut changed = resp.changed().then_some(v);
        if ui
            .button(icons::FOLDER_OPEN)
            .on_hover_text("Pick a file of the project")
            .clicked()
            && let Some(root) = &self.files_root
            && let Some(file) = rfd::FileDialog::new()
                .add_filter(label, kinds)
                .set_directory(root)
                .pick_file()
        {
            match relative(root, &file) {
                Some(rel) => changed = Some(rel),
                None => self
                    .problems
                    .push(format!("Pick a file inside {}", root.display())),
            }
        }
        changed
    }

    fn cursor(
        &mut self,
        ui: &mut egui::Ui,
        (label, hint): (&str, &str),
        path: &str,
        value: &Cursor,
    ) {
        Self::label(ui, label, hint);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let image_path = format!("{path}.image");
                let kinds = ["png", "cur", "ani", "ico", "bmp", "tga", "dds"];
                if let Some(picked) = self.file_field(ui, label, &value.image, &kinds) {
                    self.set(&image_path, json!(picked));
                }
                self.reset(ui, path);
            });
            if !value.image.is_empty() {
                ui.horizontal(|ui| {
                    self.preview(ui, value);
                    let mut own = value.hotspot.is_some();
                    let hotspot_path = format!("{path}.hotspot");
                    if ui
                        .checkbox(&mut own, "Hotspot")
                        .on_hover_text("The pixel that points; off: the one the file names")
                        .changed()
                    {
                        self.set(&hotspot_path, if own { json!([0, 0]) } else { Value::Null });
                    }
                    if let Some([x, y]) = value.hotspot {
                        let (mut x, mut y) = (x, y);
                        let rx = ui.add(DragValue::new(&mut x).range(0..=255).prefix("x "));
                        let ry = ui.add(DragValue::new(&mut y).range(0..=255).prefix("y "));
                        self.track(&rx, label);
                        self.track(&ry, label);
                        if rx.changed() || ry.changed() {
                            self.set(&hotspot_path, json!([x, y]));
                        }
                    }
                });
            }
        });
        ui.end_row();
    }

    /// The cursor's first frame with its hotspot marked, or why it can't be read.
    fn preview(&mut self, ui: &mut egui::Ui, value: &Cursor) {
        let Some(game) = self.game else { return };
        let key = format!("{}|{:?}", value.image, value.hotspot);
        let preview = self.previews.entry(key).or_insert_with(|| {
            c3_play::cursor::read(game, value).map(|cursor| {
                let frame = &cursor.frames[0];
                let image = &frame.image;
                let mut rgba = image.rgba.clone();
                let [hx, hy] = frame.hotspot;
                if hx < image.width && hy < image.height {
                    let at = ((hy * image.width + hx) * 4) as usize;
                    rgba[at..at + 4].copy_from_slice(&[255, 60, 60, 255]);
                }
                ui.ctx().load_texture(
                    format!("cursor preview {}", value.image),
                    egui::ColorImage::from_rgba_unmultiplied(
                        [image.width as usize, image.height as usize],
                        &rgba,
                    ),
                    egui::TextureOptions::NEAREST,
                )
            })
        });
        match preview {
            Ok(texture) => {
                let size = texture.size_vec2();
                let (rect, _) =
                    ui.allocate_exact_size(size + egui::vec2(4.0, 4.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, 3.0, egui::Color32::from_gray(60));
                ui.painter().image(
                    texture.id(),
                    rect.shrink(2.0),
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            Err(e) => {
                ui.label(RichText::new(format!("{} {e}", icons::WARNING)).color(theme::WARNING));
            }
        }
    }
}

/// `file` as a path under `root` with forward slashes, if it is under it.
fn relative(root: &Path, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(root).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}

fn grid(ui: &mut egui::Ui, id: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([18.0, 8.0])
        .min_col_width(150.0)
        .show(ui, body);
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).strong());
    ui.add_space(2.0);
}

impl Studio {
    /// Opens Edit > Project Settings, at `section` when given.
    pub(crate) fn show_project_settings(&mut self, section: Option<&str>) {
        if let Some(editor) = &mut self.settings {
            editor.open = true;
            if let Some(s) = section.and_then(|n| {
                Section::ALL
                    .into_iter()
                    .find(|s| s.name().eq_ignore_ascii_case(n))
            }) {
                editor.section = s;
            }
        }
    }

    /// Draws the Project Settings window while it is open and carries out its edits.
    pub(crate) fn project_settings_window(&mut self, ctx: &egui::Context) {
        let files_root = self.files_root();
        let maps: Vec<String> = self.maps.iter().map(|m| m.key()).collect();
        let Some(editor) = &mut self.settings else {
            return;
        };
        if !editor.open {
            return;
        }
        let mut open = true;
        let mut edits = Vec::new();
        let mut problems = Vec::new();
        let resp = egui::Window::new(theme::icon_text(icons::GEAR_SIX, "Project Settings"))
            .id(egui::Id::unique("project settings"))
            .open(&mut open)
            .default_size([760.0, 560.0])
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .min_size([560.0, 360.0])
            .collapsible(false)
            .show(ctx, |ui| {
                egui::Panel::left("settings sections")
                    .resizable(false)
                    .exact_size(170.0)
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                            for section in Section::ALL {
                                let text = theme::icon_text(section.icon(), section.name());
                                if ui
                                    .add(egui::Button::selectable(editor.section == section, text))
                                    .clicked()
                                {
                                    editor.section = section;
                                }
                            }
                        });
                    });
                egui::Panel::bottom("settings footer").show(ui, |ui| {
                    ui.label(
                        RichText::new(
                            "Saved to settings.toml as you go · Play picks changes up at once \
                                 (tilt and the start: next Play) · Ctrl+Z undoes",
                        )
                        .small()
                        .color(theme::TEXT_DIM),
                    );
                });
                egui::CentralPanel::default().show(ui, |ui| {
                    let section = editor.section;
                    ui.label(
                        RichText::new(theme::icon_text(section.icon(), section.name())).size(18.0),
                    );
                    ui.label(RichText::new(section.about()).color(theme::TEXT_DIM));
                    ui.separator();
                    let settings = editor.doc.settings().clone();
                    let pressed = editor.listening.and_then(|action| {
                        let key = ui.input(|i| {
                            i.events.iter().find_map(|e| match e {
                                egui::Event::Key {
                                    key, pressed: true, ..
                                } => Some(*key),
                                _ => None,
                            })
                        })?;
                        Some((action, key))
                    });
                    let mut fields = Fields {
                        doc: &editor.doc,
                        edits: Vec::new(),
                        buffers: &mut editor.buffers,
                        previews: &mut editor.previews,
                        game: self.game.as_deref(),
                        files_root: files_root.clone(),
                        maps: &maps,
                        problems: Vec::new(),
                    };
                    if let Some((action, key)) = pressed {
                        editor.listening = None;
                        if key != egui::Key::Escape {
                            fields.bind_key(&settings.input, action, key);
                        }
                    }
                    egui::ScrollArea::vertical()
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            if section == Section::Input {
                                fields.input(ui, &settings.input, &mut editor.listening);
                            } else {
                                section_fields(ui, section, &settings, &mut fields);
                            }
                        });
                    edits = fields.edits;
                    problems = fields.problems;
                });
            });
        if let Some(resp) = &resp
            && resp.response.contains_pointer()
            && ctx.input(|i| i.pointer.any_pressed())
        {
            self.focus = Focus::Settings;
        }
        if let Some(problem) = problems.pop() {
            self.error = Some(problem);
        }
        let Some(editor) = &mut self.settings else {
            return;
        };
        editor.open = open;
        let mut changed = false;
        for edit in edits {
            match edit {
                Edit::Begin(label) => editor.doc.begin_group(label),
                Edit::End => editor.doc.end_group(),
                Edit::Command(command) => match editor.doc.execute(&command) {
                    Ok(outcome) => changed |= outcome.changed,
                    Err(e) => self.error = Some(format!("Can't change that setting: {e}")),
                },
            }
        }
        if changed {
            self.settings_changed();
        }
    }

    /// Where the files the settings name live: the game's content (a Conquer client), else
    /// the project's assets.
    fn files_root(&self) -> Option<PathBuf> {
        let project = self.project.as_ref()?;
        Some(
            project
                .tq_client()
                .map_or_else(|| project.assets_dir(), |(_, dir)| dir),
        )
    }

    /// Saves the settings and applies them where they show: the game playing, the game's
    /// fonts, the Scene view's tilt.
    pub(crate) fn settings_changed(&mut self) {
        let (Some(editor), Some(project)) = (&self.settings, &self.project) else {
            return;
        };
        let settings = editor.settings().clone();
        if let Err(e) = project.save_settings(&settings) {
            self.error = Some(format!("Can't save the settings: {e}"));
        }
        if let Some(play) = &mut self.playing {
            play.set_settings(settings.clone());
        }
        if self.applied_settings.as_ref().map(|s| &s.localization) != Some(&settings.localization) {
            self.fonts_dirty = true;
        }
        let tilt_changed = self.applied_settings.as_ref().map(|s| s.camera.tilt_deg)
            != Some(settings.camera.tilt_deg);
        let maps_changed = self.applied_settings.as_ref().map(|s| &s.maps) != Some(&settings.maps);
        self.applied_settings = Some(settings);
        if maps_changed {
            self.refresh_maps();
        }
        if tilt_changed {
            self.stage = None;
            self.rebuild_live_scene();
        }
    }
}

/// The fields of `section`.
fn section_fields(ui: &mut egui::Ui, section: Section, s: &ProjectSettings, f: &mut Fields) {
    match section {
        Section::Player => grid(ui, "player", |ui| {
            let p = &s.player;
            f.text(
                ui,
                ("Game name", "Shown as the window title"),
                "player.game_name",
                &p.game_name,
                "the project's name",
            );
            f.file(
                ui,
                ("Icon", "The window icon: an .ico or an image"),
                "player.icon",
                &p.icon,
                &["ico", "png", "bmp", "tga", "dds"],
            );
            f.whole(
                ui,
                ("Window width", "Pixels"),
                "player.window_width",
                i64::from(p.window_width),
                320..=7680,
                " px",
            );
            f.whole(
                ui,
                ("Window height", "Pixels"),
                "player.window_height",
                i64::from(p.window_height),
                240..=4320,
                " px",
            );
            f.check(
                ui,
                ("Full screen", "Start full screen"),
                "player.fullscreen",
                p.fullscreen,
            );
            f.check(
                ui,
                ("Resizable", "Players may resize the window"),
                "player.resizable",
                p.resizable,
            );
        }),
        Section::Cursors => grid(ui, "cursors", |ui| {
            let c = &s.cursors;
            f.cursor(
                ui,
                ("Normal", "Over nothing in particular"),
                "cursors.normal",
                &c.normal,
            );
            f.cursor(
                ui,
                ("Attack", "Over a monster"),
                "cursors.attack",
                &c.attack,
            );
            f.cursor(ui, ("Talk", "Over an NPC"), "cursors.talk", &c.talk);
            f.cursor(
                ui,
                ("Pick up", "Over something on the ground"),
                "cursors.pick_up",
                &c.pick_up,
            );
            f.cursor(ui, ("Portal", "Over a portal"), "cursors.portal", &c.portal);
        }),
        Section::Camera => {
            let c = &s.camera;
            heading(ui, "Zoom");
            grid(ui, "camera zoom", |ui| {
                f.number(
                    ui,
                    ("Start zoom", "1 = the art's own size"),
                    "camera.zoom",
                    c.zoom,
                    0.05..=16.0,
                    "×",
                );
                f.number(
                    ui,
                    ("Least zoom", "How far players can zoom out"),
                    "camera.min_zoom",
                    c.min_zoom,
                    0.05..=16.0,
                    "×",
                );
                f.number(
                    ui,
                    ("Most zoom", "How far players can zoom in"),
                    "camera.max_zoom",
                    c.max_zoom,
                    0.05..=16.0,
                    "×",
                );
                f.number(
                    ui,
                    ("Wheel speed", "How fast the wheel zooms; 0: it doesn't"),
                    "camera.wheel_speed",
                    c.wheel_speed,
                    0.0..=5.0,
                    "×",
                );
            });
            heading(ui, "Following and panning");
            grid(ui, "camera follow", |ui| {
                f.check(
                    ui,
                    (
                        "Free pan",
                        "Players may move the camera off the hero (drag, keys)",
                    ),
                    "camera.free_pan",
                    c.free_pan,
                );
                f.number(
                    ui,
                    ("Pan speed", "Keyboard panning at zoom 1"),
                    "camera.pan_speed",
                    c.pan_speed,
                    0.0..=5000.0,
                    " px/s",
                );
                f.number(
                    ui,
                    (
                        "Follow smoothing",
                        "Seconds to catch up with the hero; 0: locked on",
                    ),
                    "camera.follow_smoothing",
                    c.follow_smoothing,
                    0.0..=3.0,
                    " s",
                );
                f.number(
                    ui,
                    (
                        "Dead zone",
                        "How far the hero may move from the centre before the camera follows",
                    ),
                    "camera.dead_zone",
                    c.dead_zone,
                    0.0..=600.0,
                    " px",
                );
                f.check(
                    ui,
                    ("Keep in map", "The view never shows past the map's edge"),
                    "camera.clamp_to_map",
                    c.clamp_to_map,
                );
                f.number(
                    ui,
                    (
                        "Shake on hit",
                        "How hard the view shakes when the hero is hit; 0: no shake",
                    ),
                    "camera.shake",
                    c.shake,
                    0.0..=40.0,
                    " px",
                );
            });
            heading(ui, "Projection");
            grid(ui, "camera projection", |ui| {
                f.tilt(ui, c.tilt_deg);
                f.number(
                    ui,
                    ("New map cell width", "Pixels across a cell of new maps"),
                    "camera.new_map_cell.0",
                    c.new_map_cell[0],
                    2.0..=512.0,
                    " px",
                );
                f.number(
                    ui,
                    ("New map cell height", "Pixels down a cell of new maps"),
                    "camera.new_map_cell.1",
                    c.new_map_cell[1],
                    1.0..=512.0,
                    " px",
                );
            });
        }
        // Drawn with the key listener it needs (see the window).
        Section::Input => {}
        Section::Gameplay => {
            let g = &s.gameplay;
            heading(ui, "Time");
            grid(ui, "gameplay time", |ui| {
                f.whole(
                    ui,
                    ("Tick", "Game time per simulation step"),
                    "gameplay.tick_ms",
                    g.tick_ms as i64,
                    10..=1000,
                    " ms",
                );
                f.number(
                    ui,
                    ("Corpse time", "How long a dead monster lies"),
                    "gameplay.corpse_seconds",
                    g.corpse_seconds,
                    0.0..=600.0,
                    " s",
                );
                f.number(
                    ui,
                    ("Revive time", "How long a dead player waits to come back"),
                    "gameplay.revive_seconds",
                    g.revive_seconds,
                    0.0..=600.0,
                    " s",
                );
                f.number(
                    ui,
                    ("Drop lifetime", "How long a drop stays on the ground"),
                    "gameplay.item_life_seconds",
                    g.item_life_seconds,
                    0.0..=3600.0,
                    " s",
                );
            });
            heading(ui, "Ranges (cells)");
            grid(ui, "gameplay ranges", |ui| {
                f.whole(
                    ui,
                    ("View", "How far players see others"),
                    "gameplay.view_range",
                    i64::from(g.view_range),
                    1..=100,
                    "",
                );
                f.whole(
                    ui,
                    ("Talk", "How near an NPC a player must be to talk"),
                    "gameplay.talk_range",
                    i64::from(g.talk_range),
                    0..=100,
                    "",
                );
                f.whole(
                    ui,
                    ("Trade", "How near an NPC a player must be to trade"),
                    "gameplay.trade_range",
                    i64::from(g.trade_range),
                    0..=100,
                    "",
                );
                f.whole(
                    ui,
                    ("Jump", "How far a player jumps at once"),
                    "gameplay.jump_range",
                    i64::from(g.jump_range),
                    0..=100,
                    "",
                );
                f.whole(
                    ui,
                    ("Attack nearest", "How far Tab looks for a monster"),
                    "gameplay.attack_search",
                    i64::from(g.attack_search),
                    0..=100,
                    "",
                );
                f.whole(
                    ui,
                    ("Monster leash", "How far monsters chase from home"),
                    "gameplay.monster_leash",
                    i64::from(g.monster_leash),
                    0..=200,
                    "",
                );
                f.whole(
                    ui,
                    ("Monster wander", "How far monsters roam from home"),
                    "gameplay.monster_wander",
                    i64::from(g.monster_wander),
                    0..=100,
                    "",
                );
            });
            heading(ui, "New characters");
            grid(ui, "gameplay start", |ui| {
                f.start_map(ui, &g.start_map);
                f.start_cell(ui, g.start_cell);
                f.whole(
                    ui,
                    ("Level", "The level they start at"),
                    "gameplay.new_character.level",
                    i64::from(g.new_character.level),
                    1..=1000,
                    "",
                );
                f.whole(
                    ui,
                    ("Money", "The money they start with"),
                    "gameplay.new_character.money",
                    g.new_character.money as i64,
                    0..=i64::MAX,
                    "",
                );
                f.numbers(
                    ui,
                    ("Bag", "Item types they carry"),
                    "gameplay.new_character.bag",
                    &g.new_character.bag,
                );
                f.numbers(
                    ui,
                    ("Worn", "Item types they wear"),
                    "gameplay.new_character.equipment",
                    &g.new_character.equipment,
                );
                f.numbers(
                    ui,
                    ("Skills", "Skills they know, in skill bar order"),
                    "gameplay.new_character.skills",
                    &g.new_character.skills,
                );
            });
            heading(ui, "Game masters");
            grid(ui, "gameplay gm", |ui| {
                f.check(
                    ui,
                    (
                        "Everyone",
                        "Everyone may use / commands (handy while making the game)",
                    ),
                    "gameplay.gm_everyone",
                    g.gm_everyone,
                );
                f.names(
                    ui,
                    (
                        "Accounts",
                        "Accounts that may use / commands",
                        "e.g. admin, alice",
                    ),
                    "gameplay.gm_accounts",
                    &g.gm_accounts,
                );
            });
        }
        Section::Items => {
            let g = &s.gameplay;
            heading(ui, "Bag and money");
            grid(ui, "gameplay items", |ui| {
                f.whole(
                    ui,
                    ("Bag size", "Items a bag holds"),
                    "gameplay.bag_size",
                    i64::from(g.bag_size),
                    1..=500,
                    "",
                );
                f.text(
                    ui,
                    ("Currency", "The money's name, as players see it"),
                    "gameplay.currency",
                    &g.currency,
                    "Gold",
                );
                f.names(
                    ui,
                    (
                        "Other currencies",
                        "Currencies beside the money (gems, honour); items may be priced in them",
                        "e.g. Gems, Honour",
                    ),
                    "gameplay.currencies",
                    &g.currencies,
                );
                f.names(
                    ui,
                    (
                        "Qualities",
                        "Item qualities, from the lowest",
                        "e.g. Normal, Refined, Unique",
                    ),
                    "gameplay.qualities",
                    &g.qualities,
                );
                f.whole(
                    ui,
                    (
                        "Quality bonus",
                        "What each quality step adds to what an item gives",
                    ),
                    "gameplay.quality_bonus_percent",
                    i64::from(g.quality_bonus_percent),
                    0..=1000,
                    " %",
                );
                f.whole(
                    ui,
                    (
                        "Wear",
                        "Chance a worn item loses a point of durability when its wearer hits or is hit",
                    ),
                    "gameplay.wear_percent",
                    i64::from(g.wear_percent),
                    0..=100,
                    " %",
                );
            });
            heading(ui, "Equipment slots");
            let game_slots = f.game.map(|g| g.slot_names()).unwrap_or_default();
            f.equipment_slots(ui, &g.equipment_slots, &game_slots);
            heading(ui, "Crime and PK points");
            grid(ui, "pk", |ui| {
                let pk = &g.pk;
                f.number(
                    ui,
                    (
                        "Crime",
                        "How long attacking a player of good name makes a criminal (blue name)",
                    ),
                    "gameplay.pk.crime_seconds",
                    pk.crime_seconds,
                    0.0..=3600.0,
                    " s",
                );
                f.number(
                    ui,
                    (
                        "Crime after a kill",
                        "How long a criminal who kills stays one",
                    ),
                    "gameplay.pk.crime_kill_seconds",
                    pk.crime_kill_seconds,
                    0.0..=3600.0,
                    " s",
                );
                f.whole(
                    ui,
                    ("PK points a kill", "For killing a player of good name"),
                    "gameplay.pk.kill_points",
                    i64::from(pk.kill_points),
                    0..=1000,
                    "",
                );
                f.whole(
                    ui,
                    ("Red name from", "PK points that make a name red"),
                    "gameplay.pk.red_at",
                    i64::from(pk.red_at),
                    1..=30000,
                    " points",
                );
                f.whole(
                    ui,
                    ("Black name from", "PK points that make a name black"),
                    "gameplay.pk.black_at",
                    i64::from(pk.black_at),
                    1..=30000,
                    " points",
                );
                f.number(
                    ui,
                    ("A point wears off every", "0: PK points stay"),
                    "gameplay.pk.decay_seconds",
                    pk.decay_seconds,
                    0.0..=36000.0,
                    " s",
                );
            });
            heading(ui, "Player attributes");
            f.attributes(ui, &g.attributes);
            heading(ui, "Item attributes");
            grid(ui, "item attributes", |ui| {
                f.names(
                    ui,
                    (
                        "Names",
                        "The numbers every item keeps beside its type, count, durability and \
                         quality (up to 8), in order",
                        "e.g. plus, gem1, gem2",
                    ),
                    "gameplay.item_attributes",
                    &g.item_attributes,
                );
            });
            if g.item_attributes.len() > ITEM_ATTRIBUTES {
                ui.colored_label(
                    theme::DANGER,
                    format!("Items keep {ITEM_ATTRIBUTES} numbers: the names past that go unused."),
                );
            }
        }
        Section::Maps => f.map_entries(ui, &s.maps),
        Section::Graphics => grid(ui, "graphics", |ui| {
            let g = &s.graphics;
            f.check(
                ui,
                ("VSync", "Wait for the display between frames (no tearing)"),
                "graphics.vsync",
                g.vsync,
            );
            f.whole(
                ui,
                ("Frame cap", "Most frames per second; 0: no limit"),
                "graphics.frame_cap",
                i64::from(g.frame_cap),
                0..=1000,
                " fps",
            );
            f.color(
                ui,
                ("Background", "Colour around and behind the map"),
                "graphics.background",
                g.background,
            );
            f.color(
                ui,
                (
                    "Light",
                    "Colour of the light over the scene: white draws it as it is; darker or tinted for night, dusk or caves",
                ),
                "graphics.light",
                g.light,
            );
            f.check(
                ui,
                (
                    "Shade models",
                    "Shade characters and monsters as if lit by the sun from one side",
                ),
                "graphics.shade_models",
                g.shade_models,
            );
        }),
        Section::Audio => grid(ui, "audio", |ui| {
            let a = &s.audio;
            f.number(
                ui,
                ("Master volume", "Everything"),
                "audio.master_volume",
                a.master_volume,
                0.0..=1.0,
                "",
            );
            f.number(
                ui,
                ("Music volume", "Music"),
                "audio.music_volume",
                a.music_volume,
                0.0..=1.0,
                "",
            );
            f.number(
                ui,
                ("Sound volume", "Actions, effects and the UI"),
                "audio.sound_volume",
                a.sound_volume,
                0.0..=1.0,
                "",
            );
            f.number(
                ui,
                ("Hearing range", "How far from the hero a sound fades out"),
                "audio.hearing_range",
                a.hearing_range,
                1.0..=100.0,
                " cells",
            );
        }),
        Section::Network => {
            let n = &s.network;
            grid(ui, "network", |ui| {
                f.whole(
                    ui,
                    ("Port", "The engine protocol's port"),
                    "network.port",
                    i64::from(n.port),
                    1..=65535,
                    "",
                );
                f.number(
                    ui,
                    ("Save interval", "How often logged-in characters are saved"),
                    "network.save_interval_seconds",
                    n.save_interval_seconds,
                    1.0..=3600.0,
                    " s",
                );
                f.number(
                    ui,
                    ("Join timeout", "How long a client waits for the server"),
                    "network.join_timeout_seconds",
                    n.join_timeout_seconds,
                    1.0..=120.0,
                    " s",
                );
            });
            heading(ui, "Fronts for other clients");
            f.fronts(ui, &n.fronts);
        }
        Section::GameUi => grid(ui, "game ui", |ui| {
            let u = &s.ui;
            f.text(
                ui,
                (
                    "Layout",
                    "Where dialogs and controls sit: [dialog-control] x, y, w, h",
                ),
                "ui.layout",
                &u.layout,
                "ui/gui.ini",
            );
            f.text(
                ui,
                (
                    "Skin",
                    "The images controls show: [Section] FrameAmount, Frame0...",
                ),
                "ui.skin",
                &u.skin,
                "ui/skin.ani",
            );
            f.text(
                ui,
                ("Controls", "Each control's kind, image, binding and action"),
                "ui.controls",
                &u.controls,
                "ui/controls.ini",
            );
        }),
        Section::Localization => {
            heading(ui, "Fonts");
            f.fonts(ui, &s.localization.fonts);
        }
    }
}

/// Fields particular to one setting.
impl Fields<'_> {
    fn tilt(&mut self, ui: &mut egui::Ui, tilt: Option<f64>) {
        Self::label(
            ui,
            "Tilt",
            "Tilt of 3D roles and effects; off: the game's own",
        );
        ui.horizontal(|ui| {
            let mut own = tilt.is_some();
            let game_tilt = self.game.map_or(0.0, |g| g.tilt_deg());
            if ui.checkbox(&mut own, "").changed() {
                self.set(
                    "camera.tilt_deg",
                    if own { json!(game_tilt) } else { Value::Null },
                );
            }
            match tilt {
                Some(t) => {
                    let mut t = t;
                    let resp = ui.add(
                        DragValue::new(&mut t)
                            .range(-90.0..=90.0)
                            .speed(0.1)
                            .suffix("°"),
                    );
                    self.track(&resp, "Tilt");
                    if resp.changed() {
                        self.set("camera.tilt_deg", json!(t));
                    }
                }
                None => {
                    ui.label(
                        RichText::new(format!("the game's ({game_tilt:.1}°)"))
                            .color(theme::TEXT_DIM),
                    );
                }
            }
        });
        ui.end_row();
    }

    fn start_map(&mut self, ui: &mut egui::Ui, map: &str) {
        Self::label(ui, "Start map", "The map new characters start on");
        ui.horizontal(|ui| {
            let shown = if map.is_empty() {
                "the game's first map"
            } else {
                map
            };
            let mut chosen = None;
            egui::ComboBox::from_id_salt("start map")
                .selected_text(shown)
                .width(240.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(map.is_empty(), "the game's first map")
                        .clicked()
                    {
                        chosen = Some(String::new());
                    }
                    for m in self.maps {
                        if ui.selectable_label(m == map, m).clicked() {
                            chosen = Some(m.clone());
                        }
                    }
                });
            if let Some(m) = chosen {
                self.set("gameplay.start_map", json!(m));
            }
            self.reset(ui, "gameplay.start_map");
        });
        ui.end_row();
    }

    fn start_cell(&mut self, ui: &mut egui::Ui, cell: Option<[i32; 2]>) {
        Self::label(
            ui,
            "Start cell",
            "The cell they start on; off: the map's centre",
        );
        ui.horizontal(|ui| {
            let mut own = cell.is_some();
            if ui.checkbox(&mut own, "").changed() {
                self.set(
                    "gameplay.start_cell",
                    if own { json!([0, 0]) } else { Value::Null },
                );
            }
            match cell {
                Some([x, y]) => {
                    let (mut x, mut y) = (x, y);
                    let rx = ui.add(DragValue::new(&mut x).range(0..=4096).prefix("x "));
                    let ry = ui.add(DragValue::new(&mut y).range(0..=4096).prefix("y "));
                    self.track(&rx, "Start cell");
                    self.track(&ry, "Start cell");
                    if rx.changed() || ry.changed() {
                        self.set("gameplay.start_cell", json!([x, y]));
                    }
                }
                None => {
                    ui.label(RichText::new("the map's centre").color(theme::TEXT_DIM));
                }
            }
        });
        ui.end_row();
    }

    fn fronts(&mut self, ui: &mut egui::Ui, fronts: &[FrontSettings]) {
        let mut list = fronts.to_vec();
        let mut changed = false;
        let mut remove = None;
        if list.is_empty() {
            ui.label(
                RichText::new("Only the engine's own clients connect.").color(theme::TEXT_DIM),
            );
        }
        egui::Grid::new("fronts")
            .num_columns(3)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                for (i, front) in list.iter_mut().enumerate() {
                    let kind = ui
                        .add_sized(
                            [160.0, 22.0],
                            egui::TextEdit::singleline(&mut front.kind).hint_text("kind"),
                        )
                        .on_hover_text("Which client it serves, e.g. conquer-5065");
                    let ip = ui
                        .add_sized(
                            [160.0, 22.0],
                            egui::TextEdit::singleline(&mut front.public_ip).hint_text("public IP"),
                        )
                        .on_hover_text("Where its clients are sent after logging in");
                    self.track(&kind, "fronts");
                    self.track(&ip, "fronts");
                    changed |= kind.changed() || ip.changed();
                    if ui
                        .button(icons::TRASH)
                        .on_hover_text("Remove this front")
                        .clicked()
                    {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        if let Some(i) = remove {
            list.remove(i);
            changed = true;
        }
        if ui
            .button(theme::icon_text(icons::PLUS, "Add a front"))
            .clicked()
        {
            list.push(FrontSettings {
                kind: "conquer-5065".into(),
                public_ip: "127.0.0.1".into(),
            });
            changed = true;
        }
        if changed {
            self.set(
                "network.front",
                serde_json::to_value(&list).unwrap_or_default(),
            );
        }
    }

    fn map_entries(&mut self, ui: &mut egui::Ui, entries: &[MapEntry]) {
        let mut list = entries.to_vec();
        let mut changed = false;
        let mut remove = None;
        // Every map with the ids it plays with now (unlisted maps keep theirs when listed).
        let game_maps = self
            .maps
            .iter()
            .map(|k| (k.clone(), self.game.map_or_else(Vec::new, |g| g.map_ids(k))))
            .collect();
        let all = MapList::from_parts(entries, game_maps);
        if list.is_empty() {
            ui.label(
                RichText::new("No map is listed yet: add the maps portals lead to.")
                    .color(theme::TEXT_DIM),
            );
        }
        egui::Grid::new("maps")
            .num_columns(8)
            .spacing([10.0, 6.0])
            .striped(true)
            .show(ui, |ui| {
                if !list.is_empty() {
                    for title in ["Map", "Id", "Name", "Music", "PvP", "Free PK", "Light", ""] {
                        ui.label(RichText::new(title).color(theme::TEXT_DIM).small());
                    }
                    ui.end_row();
                }
                for (i, entry) in list.iter_mut().enumerate() {
                    let mut key = None;
                    egui::ComboBox::from_id_salt(("map key", i))
                        .selected_text(map_list::short_name(&entry.key))
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            for m in self.maps {
                                let label = map_list::short_name(m);
                                if ui.selectable_label(*m == entry.key, label).clicked() {
                                    key = Some(m.clone());
                                }
                            }
                        })
                        .response
                        .on_hover_text(&entry.key);
                    if let Some(key) = key {
                        entry.key = key;
                        changed = true;
                    }
                    let id = ui
                        .add(DragValue::new(&mut entry.id).range(1..=u32::MAX).speed(0.2))
                        .on_hover_text("What portals, scripts and saves call the map");
                    self.track(&id, "Map id");
                    let name = ui.add_sized(
                        [140.0, 22.0],
                        egui::TextEdit::singleline(&mut entry.name)
                            .hint_text(map_list::short_name(&entry.key)),
                    );
                    self.track(&name, "Map name");
                    changed |= id.changed() || name.changed();
                    ui.horizontal(|ui| {
                        let kinds = ["ogg", "wav", "mp3"];
                        if let Some(music) = self.file_field(ui, "Music", &entry.music, &kinds) {
                            entry.music = music;
                            changed = true;
                        }
                    });
                    changed |= ui
                        .checkbox(&mut entry.pk, "")
                        .on_hover_text("Players may attack each other here")
                        .changed();
                    changed |= ui
                        .checkbox(&mut entry.pk_free, "")
                        .on_hover_text(
                            "Fights between players here bring no crime and no PK points \
                             (arenas, guild wars)",
                        )
                        .changed();
                    changed |= map_light_field(ui, &mut entry.light);
                    if ui
                        .button(icons::TRASH)
                        .on_hover_text("Take the map off the list")
                        .clicked()
                    {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        if let Some(i) = remove {
            list.remove(i);
            changed = true;
        }
        let unlisted: Vec<&MapInfo> = all
            .iter()
            .filter(|m| !entries.iter().any(|e| e.key == m.key))
            .collect();
        ui.add_space(4.0);
        let add = ui
            .add_enabled(
                !unlisted.is_empty(),
                egui::Button::new(theme::icon_text(icons::PLUS, "Add a map")),
            )
            .on_hover_text("Lists the next map, with the id it plays with now");
        if add.clicked()
            && let Some(m) = unlisted.first()
        {
            list.push(MapEntry {
                key: m.key.clone(),
                id: m.id(),
                ..Default::default()
            });
            changed = true;
        }
        if !unlisted.is_empty() {
            ui.add_space(6.0);
            egui::CollapsingHeader::new(format!("Maps not listed ({})", unlisted.len()))
                .id_salt("unlisted maps")
                .show(ui, |ui| {
                    egui::Grid::new("unlisted maps")
                        .num_columns(2)
                        .spacing([16.0, 2.0])
                        .show(ui, |ui| {
                            for m in &unlisted {
                                ui.label(&m.name).on_hover_text(&m.key);
                                let ids: Vec<String> = m.ids.iter().map(u32::to_string).collect();
                                ui.label(RichText::new(ids.join(", ")).color(theme::TEXT_DIM));
                                ui.end_row();
                            }
                        });
                });
        }
        if changed {
            self.set("maps", serde_json::to_value(&list).unwrap_or_default());
        }
    }

    /// The slots items are worn in: the settings' list, or the game's own until one is made.
    fn equipment_slots(
        &mut self,
        ui: &mut egui::Ui,
        slots: &[EquipmentSlot],
        game_slots: &[(u8, String)],
    ) {
        let mut list = slots.to_vec();
        let mut changed = false;
        let mut remove = None;
        if list.is_empty() {
            let names: Vec<&str> = game_slots.iter().map(|(_, n)| n.as_str()).collect();
            ui.label(
                RichText::new(format!("The game's own: {}.", names.join(", ")))
                    .color(theme::TEXT_DIM),
            );
            if ui
                .button(theme::icon_text(
                    icons::PENCIL_SIMPLE,
                    "Make them the project's",
                ))
                .on_hover_text("Copies the game's slots here to rename, add and remove")
                .clicked()
            {
                list = game_slots
                    .iter()
                    .map(|(id, name)| EquipmentSlot {
                        id: *id,
                        name: name.clone(),
                    })
                    .collect();
                changed = true;
            }
        }
        egui::Grid::new("equipment slots")
            .num_columns(3)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                for (i, slot) in list.iter_mut().enumerate() {
                    let id = ui
                        .add(DragValue::new(&mut slot.id).range(1..=255).prefix("slot "))
                        .on_hover_text("The number item types name the slot by (items' slot)");
                    let name = ui.add_sized(
                        [160.0, 22.0],
                        egui::TextEdit::singleline(&mut slot.name).hint_text("name"),
                    );
                    self.track(&id, "Equipment slots");
                    self.track(&name, "Equipment slots");
                    changed |= id.changed() || name.changed();
                    if ui
                        .button(icons::TRASH)
                        .on_hover_text("Remove this slot")
                        .clicked()
                    {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        if let Some(i) = remove {
            list.remove(i);
            changed = true;
        }
        if !list.is_empty()
            && ui
                .button(theme::icon_text(icons::PLUS, "Add a slot"))
                .clicked()
        {
            let id = list
                .iter()
                .map(|s| s.id)
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            list.push(EquipmentSlot {
                id,
                name: format!("Slot {id}"),
            });
            changed = true;
        }
        if changed {
            self.set(
                "gameplay.equipment_slots",
                serde_json::to_value(&list).unwrap_or_default(),
            );
        }
    }

    /// The numbers every player keeps: name, new players' value, range and what it is for.
    fn attributes(&mut self, ui: &mut egui::Ui, attributes: &[AttributeDef]) {
        let mut list = attributes.to_vec();
        let mut changed = false;
        let mut remove = None;
        if list.is_empty() {
            ui.label(
                RichText::new(
                    "None yet: add the numbers your players keep (strength, honour...). Scripts \
                     and formulas read them with attr(name).",
                )
                .color(theme::TEXT_DIM),
            );
        }
        egui::Grid::new("player attributes")
            .num_columns(6)
            .spacing([10.0, 6.0])
            .striped(true)
            .show(ui, |ui| {
                if !list.is_empty() {
                    for title in ["Name", "New players", "Lowest", "Highest", "What it is", ""] {
                        ui.label(RichText::new(title).color(theme::TEXT_DIM).small());
                    }
                    ui.end_row();
                }
                for (i, a) in list.iter_mut().enumerate() {
                    let mut responses = vec![
                        ui.add_sized(
                            [130.0, 22.0],
                            egui::TextEdit::singleline(&mut a.name).hint_text("name"),
                        ),
                        ui.add(DragValue::new(&mut a.default)),
                    ];
                    for limit in [&mut a.min, &mut a.max] {
                        let mut on = limit.is_some();
                        let mut value = limit.unwrap_or(0);
                        let r = ui
                            .horizontal(|ui| {
                                let check = ui.checkbox(&mut on, "");
                                let drag = ui.add_enabled(on, DragValue::new(&mut value));
                                check | drag
                            })
                            .inner
                            .on_hover_text("Unticked: no limit");
                        *limit = on.then_some(value);
                        responses.push(r);
                    }
                    responses.push(ui.add_sized(
                        [220.0, 22.0],
                        egui::TextEdit::singleline(&mut a.doc).hint_text("what it is for"),
                    ));
                    for r in &responses {
                        self.track(r, "Player attributes");
                        changed |= r.changed();
                    }
                    if ui
                        .button(icons::TRASH)
                        .on_hover_text("Remove this attribute")
                        .clicked()
                    {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        if let Some(i) = remove {
            list.remove(i);
            changed = true;
        }
        if ui
            .button(theme::icon_text(icons::PLUS, "Add an attribute"))
            .clicked()
        {
            list.push(AttributeDef {
                name: format!("attribute{}", list.len() + 1),
                min: Some(0),
                ..AttributeDef::default()
            });
            changed = true;
        }
        if changed {
            self.set(
                "gameplay.attribute",
                serde_json::to_value(&list).unwrap_or_default(),
            );
        }
    }

    fn fonts(&mut self, ui: &mut egui::Ui, fonts: &[String]) {
        let mut list = fonts.to_vec();
        let mut changed = false;
        let mut remove = None;
        if list.is_empty() {
            ui.label(
                RichText::new("The engine's font draws the game's text.").color(theme::TEXT_DIM),
            );
        }
        for (i, font) in fonts.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}.", i + 1)).color(theme::TEXT_DIM));
                if let Some(picked) = self.file_field(ui, "Font", font, &["ttf", "otf"]) {
                    list[i] = picked;
                    changed = true;
                }
                if ui
                    .button(icons::TRASH)
                    .on_hover_text("Remove this font")
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            list.remove(i);
            changed = true;
        }
        if ui
            .button(theme::icon_text(icons::PLUS, "Add a font"))
            .clicked()
        {
            list.push(String::new());
            changed = true;
        }
        if changed {
            self.set("localization.fonts", json!(list));
        }
    }
}

/// A map's own light: off uses the project's (Graphics > Light). Returns whether it changed.
fn map_light_field(ui: &mut egui::Ui, light: &mut Option<[u8; 3]>) -> bool {
    ui.horizontal(|ui| {
        let mut own = light.is_some();
        let mut changed = ui
            .checkbox(&mut own, "")
            .on_hover_text("Its own light (night, a cave); off: the project's")
            .changed();
        if changed {
            *light = own.then_some([255, 255, 255]);
        }
        if let Some(color) = light {
            changed |= ui.color_edit_button_srgb(color).changed();
        }
        changed
    })
    .inner
}
