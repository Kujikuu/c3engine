//! The Data panel: the project's tables (items, monsters, skills…) as spreadsheets. Search
//! rows, double-click a cell to edit it, add and delete rows; the Inspector shows the selected
//! row. Every edit is a `c3_edit` table command with undo. Tables come from SQL dumps through
//! the import dialog.

use crate::app::{Focus, Studio};
use crate::icons;
use crate::theme;
use c3_data::sql::{self, DumpTable};
use c3_data::{ColumnKind, Table, Value};
use c3_edit::{TableCommand, TableDoc};
use eframe::egui::{self, RichText};
use egui_extras::{Column as GridColumn, TableBuilder};
use std::path::PathBuf;

/// Height of a grid row.
const ROW_HEIGHT: f32 = 22.0;

/// A table open in the Data panel.
pub struct OpenTable {
    pub name: String,
    pub doc: TableDoc,
}

/// A cell being typed into.
struct CellEdit {
    key: i64,
    column: usize,
    text: String,
    /// Focus the text field on its first frame.
    fresh: bool,
}

/// The import dialog: a dump's tables, with which to import.
struct ImportDialog {
    file: PathBuf,
    sql: String,
    tables: Vec<(DumpTable, bool)>,
}

#[derive(Default)]
pub struct DataPanel {
    pub open: Option<OpenTable>,
    /// The project's table names (refreshed when tables change on disk).
    pub names: Vec<String>,
    pub search: String,
    /// Key of the selected row.
    pub selected: Option<i64>,
    editing: Option<CellEdit>,
    import: Option<ImportDialog>,
    /// Rows matching the search, by index (`None` = recompute).
    filtered: Option<Vec<usize>>,
}

impl DataPanel {
    pub fn is_dirty(&self) -> bool {
        self.open.as_ref().is_some_and(|t| t.doc.is_dirty())
    }

    /// The table changed: refilter on the next frame.
    fn changed(&mut self) {
        self.filtered = None;
    }

    pub fn undo(&mut self) -> Option<String> {
        let label = self.open.as_mut()?.doc.undo();
        self.changed();
        label
    }

    pub fn redo(&mut self) -> Option<String> {
        let label = self.open.as_mut()?.doc.redo();
        self.changed();
        label
    }

    fn rows(&mut self) -> &[usize] {
        let Some(open) = &self.open else {
            return &[];
        };
        self.filtered.get_or_insert_with(|| {
            let table = open.doc.table();
            let needle = self.search.trim().to_lowercase();
            (0..table.rows.len())
                .filter(|&r| {
                    needle.is_empty()
                        || table.rows[r]
                            .iter()
                            .any(|v| v.to_string().to_lowercase().contains(&needle))
                })
                .collect()
        })
    }
}

impl Studio {
    /// Re-reads the project's table names.
    pub(crate) fn refresh_tables(&mut self) {
        self.data.names = self
            .project
            .as_ref()
            .map(|p| p.table_names())
            .unwrap_or_default();
    }

    pub(crate) fn data_panel(&mut self, ui: &mut egui::Ui) {
        if self.project.is_none() {
            theme::empty_state(
                ui,
                icons::DATABASE,
                "No project open",
                "Open a project to edit its game data.",
            );
            return;
        }
        if ui.rect_contains_pointer(ui.max_rect()) && ui.input(|i| i.pointer.any_pressed()) {
            self.focus = Focus::Data;
        }
        let mut open = None;
        egui::Panel::left("data tables")
            .resizable(true)
            .default_size(190.0)
            .size_range(140.0..=360.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Tables").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(theme::icon_text(icons::DOWNLOAD_SIMPLE, "Import"))
                            .on_hover_text("Import tables from an SQL dump")
                            .clicked()
                        {
                            self.pick_dump();
                        }
                    });
                });
                ui.separator();
                if self.data.names.is_empty() {
                    ui.label(
                        RichText::new("No tables yet. Import them from an SQL dump.")
                            .color(theme::TEXT_DIM),
                    );
                }
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                            for name in &self.data.names {
                                let current =
                                    self.data.open.as_ref().is_some_and(|t| &t.name == name);
                                let dirty = current && self.data.is_dirty();
                                let label = theme::icon_text(
                                    icons::DATABASE,
                                    &format!("{name}{}", if dirty { "*" } else { "" }),
                                );
                                if ui.add(egui::Button::selectable(current, label)).clicked() {
                                    open = Some(name.clone());
                                }
                            }
                        });
                    });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            if self.data.open.is_some() {
                self.table_bar(ui);
                ui.separator();
                self.table_grid(ui);
            } else {
                theme::empty_state(
                    ui,
                    icons::DATABASE,
                    "No table open",
                    "Pick a table on the left, or import some from an SQL dump.",
                );
            }
        });
        if let Some(name) = open {
            self.open_table(&name);
        }
        self.import_dialog(ui.ctx());
    }

    /// Opens table `name`, asking first if the open one has unsaved changes.
    pub(crate) fn open_table(&mut self, name: &str) {
        if self.data.open.as_ref().is_some_and(|t| t.name == name) {
            return;
        }
        if self.data.is_dirty() {
            self.pending = Some(crate::app::Pending::OpenTable(name.to_owned()));
            return;
        }
        self.load_table(name);
    }

    pub(crate) fn load_table(&mut self, name: &str) {
        let Some(project) = &self.project else { return };
        match c3_data::load_table(&project.table_file(name)) {
            Ok(table) => {
                let problems = table.problems();
                if let Some(p) = problems.first() {
                    self.log_error(format!(
                        "Table {name}: {p} (and {} more)",
                        problems.len() - 1
                    ));
                }
                self.data.open = Some(OpenTable {
                    name: name.to_owned(),
                    doc: TableDoc::new(table),
                });
                self.data.selected = None;
                self.data.editing = None;
                self.data.changed();
                self.focus = Focus::Data;
            }
            Err(e) => self.error = Some(format!("Can't open table {name}: {e}")),
        }
    }

    /// Writes the open table; false if that failed.
    pub(crate) fn save_table(&mut self) -> bool {
        let (Some(project), Some(open)) = (&self.project, &mut self.data.open) else {
            return true;
        };
        open.doc.end_group();
        match c3_data::save_table(open.doc.table(), &project.table_file(&open.name)) {
            Ok(()) => {
                open.doc.mark_saved();
                let name = open.name.clone();
                self.notify(format!("Saved table {name}"));
                true
            }
            Err(e) => {
                self.error = Some(format!("Can't save table {}: {e}", open.name));
                false
            }
        }
    }

    fn table_bar(&mut self, ui: &mut egui::Ui) {
        let rows_shown = self.data.rows().len();
        let Some(open) = &self.data.open else { return };
        let table = open.doc.table();
        let total = table.rows.len();
        let mut add = false;
        let mut delete = false;
        let mut save = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(theme::icon_text(icons::DATABASE, &open.name)).strong());
            let count = if rows_shown == total {
                format!("{total} rows · {} columns", table.columns.len())
            } else {
                format!("{rows_shown} of {total} rows")
            };
            ui.label(RichText::new(count).color(theme::TEXT_DIM));
            ui.separator();
            let search = ui.add(
                egui::TextEdit::singleline(&mut self.data.search)
                    .hint_text(theme::icon_text(icons::MAGNIFYING_GLASS, "Search rows"))
                    .desired_width(180.0),
            );
            if search.changed() {
                self.data.filtered = None;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let dirty = open.doc.is_dirty();
                save = ui
                    .add_enabled(
                        dirty,
                        egui::Button::new(theme::icon_text(icons::FLOPPY_DISK, "Save")),
                    )
                    .on_hover_text("Ctrl+S")
                    .clicked();
                delete = ui
                    .add_enabled(
                        self.data.selected.is_some(),
                        egui::Button::new(theme::icon_text(icons::TRASH, "Delete row")),
                    )
                    .clicked();
                add = ui
                    .button(theme::icon_text(icons::PLUS, "Add row"))
                    .clicked();
            });
        });
        if add {
            self.run_table(TableCommand::AddRow {
                values: None,
                key: None,
            });
            self.data.selected = self.data.open.as_ref().and_then(|t| {
                let table = t.doc.table();
                table.key(table.rows.len().checked_sub(1)?)
            });
        }
        if delete && let Some(key) = self.data.selected.take() {
            self.run_table(TableCommand::RemoveRow { key });
        }
        if save {
            self.save_table();
        }
    }

    /// Runs a table command, reporting refusals.
    pub(crate) fn run_table(&mut self, command: TableCommand) {
        let Some(open) = &mut self.data.open else {
            return;
        };
        match open.doc.execute(&command) {
            Ok(outcome) if outcome.changed => {
                self.data.changed();
                self.notify(outcome.summary);
            }
            Ok(_) => {}
            Err(e) => self.error = Some(format!("Can't change the table: {e}")),
        }
    }

    fn table_grid(&mut self, ui: &mut egui::Ui) {
        let rows: Vec<usize> = self.data.rows().to_vec();
        let Some(open) = &self.data.open else { return };
        let table: &Table = open.doc.table();
        let selected = self.data.selected;
        let mut select = None;
        let mut start_edit = None;
        let mut commit: Option<TableCommand> = None;
        let mut cancel = false;
        let editing = &mut self.data.editing;
        // Wide tables scroll sideways; rows scroll inside the grid.
        egui::ScrollArea::horizontal()
            .auto_shrink(false)
            .show(ui, |ui| {
                let mut builder = TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .sense(egui::Sense::click())
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
                for c in &table.columns {
                    let width = match c.kind {
                        ColumnKind::Text => 150.0,
                        ColumnKind::Bool => 60.0,
                        ColumnKind::Int | ColumnKind::Float => 86.0,
                    };
                    builder = builder.column(GridColumn::initial(width).at_least(40.0).clip(true));
                }
                builder
                    .header(ROW_HEIGHT, |mut header| {
                        for (i, c) in table.columns.iter().enumerate() {
                            header.col(|ui| {
                                let name = if i == 0 {
                                    theme::icon_text(icons::KEY, &c.name)
                                } else {
                                    c.name.clone()
                                };
                                ui.label(RichText::new(name).strong())
                                    .on_hover_text(column_hint(c));
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(ROW_HEIGHT, rows.len(), |mut row| {
                            let r = rows[row.index()];
                            let values = &table.rows[r];
                            let key = values.first().and_then(Value::as_int).unwrap_or_default();
                            row.set_selected(selected == Some(key));
                            for (c, value) in values.iter().enumerate() {
                                let (_, resp) = row.col(|ui| {
                                    match editing.as_mut().filter(|e| e.key == key && e.column == c)
                                    {
                                        Some(edit) => {
                                            let field = ui.add(
                                                egui::TextEdit::singleline(&mut edit.text)
                                                    .desired_width(f32::INFINITY),
                                            );
                                            if std::mem::take(&mut edit.fresh) {
                                                field.request_focus();
                                            }
                                            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                                cancel = true;
                                            } else if field.lost_focus() {
                                                let kind = table.columns[c].kind;
                                                commit = Some(match kind.parse(&edit.text) {
                                                    Some(value) => TableCommand::SetCell {
                                                        key,
                                                        column: table.columns[c].name.clone(),
                                                        value,
                                                    },
                                                    None => {
                                                        cancel = true;
                                                        return;
                                                    }
                                                });
                                            }
                                        }
                                        None => match value {
                                            Value::Bool(b) => {
                                                let mut on = *b;
                                                if ui.checkbox(&mut on, "").changed() {
                                                    commit = Some(TableCommand::SetCell {
                                                        key,
                                                        column: table.columns[c].name.clone(),
                                                        value: Value::Bool(on),
                                                    });
                                                }
                                            }
                                            _ => {
                                                let text = value.to_string();
                                                let color = match value {
                                                    Value::Text(_) => theme::TEXT,
                                                    _ => egui::Color32::from_rgb(170, 200, 235),
                                                };
                                                ui.label(RichText::new(text).color(color));
                                            }
                                        },
                                    }
                                });
                                if resp.clicked() {
                                    select = Some(key);
                                }
                                if resp.double_clicked() && !matches!(value, Value::Bool(_)) {
                                    start_edit = Some(CellEdit {
                                        key,
                                        column: c,
                                        text: value.to_string(),
                                        fresh: true,
                                    });
                                }
                            }
                        });
                    });
            });
        if let Some(key) = select {
            self.data.selected = Some(key);
            self.inspect_asset = false;
        }
        if let Some(edit) = start_edit {
            self.data.editing = Some(edit);
        }
        if cancel {
            self.data.editing = None;
        }
        if let Some(command) = commit {
            self.data.editing = None;
            self.run_table(command);
        }
    }

    /// The selected row in the Inspector, every column editable; false if none is selected.
    pub(crate) fn row_inspector(&mut self, ui: &mut egui::Ui) -> bool {
        if self.focus != Focus::Data {
            return false;
        }
        let (Some(open), Some(key)) = (&self.data.open, self.data.selected) else {
            return false;
        };
        let table = open.doc.table();
        let Some(r) = table.find(key) else {
            return false;
        };
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(icons::DATABASE)
                    .size(22.0)
                    .color(theme::ACCENT),
            );
            ui.vertical(|ui| {
                ui.label(RichText::new(format!("Row {key}")).size(15.0).strong());
                ui.label(RichText::new(&open.name).color(theme::TEXT_DIM).small());
            });
        });
        ui.separator();
        let mut commit = None;
        egui::Grid::new("row inspector")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                for (c, column) in table.columns.iter().enumerate() {
                    ui.label(RichText::new(&column.name).color(theme::TEXT_DIM))
                        .on_hover_text(column_hint(column));
                    let set = |value| TableCommand::SetCell {
                        key,
                        column: column.name.clone(),
                        value,
                    };
                    match table.rows[r][c].clone() {
                        Value::Int(mut v) => {
                            if ui.add(egui::DragValue::new(&mut v)).changed() {
                                commit = Some(set(Value::Int(v)));
                            }
                        }
                        Value::Float(mut v) => {
                            if ui.add(egui::DragValue::new(&mut v).speed(0.01)).changed() {
                                commit = Some(set(Value::Float(v)));
                            }
                        }
                        Value::Bool(mut v) => {
                            if ui.checkbox(&mut v, "").changed() {
                                commit = Some(set(Value::Bool(v)));
                            }
                        }
                        Value::Text(mut t) => {
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut t).desired_width(f32::INFINITY),
                                )
                                .changed()
                            {
                                commit = Some(set(Value::Text(t)));
                            }
                        }
                    }
                    ui.end_row();
                }
            });
        if let Some(command) = commit {
            if let TableCommand::SetCell { column, .. } = &command
                && column == &table.columns[0].name
            {
                self.data.selected = None;
            }
            self.run_table(command);
        }
        true
    }

    fn pick_dump(&mut self) {
        let Some(file) = rfd::FileDialog::new()
            .set_title("Import tables from an SQL dump")
            .add_filter("SQL dump", &["sql"])
            .pick_file()
        else {
            return;
        };
        self.open_dump(file);
    }

    /// Reads a dump and opens the import dialog for it.
    pub(crate) fn open_dump(&mut self, file: PathBuf) {
        let sql = match std::fs::read(&file) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(e) => {
                self.error = Some(format!("Can't read {}: {e}", file.display()));
                return;
            }
        };
        let suggested = self
            .client
            .as_ref()
            .map_or(&[][..], |c| c.suggested_tables());
        let tables = sql::list_tables(&sql)
            .into_iter()
            .map(|t| {
                let pick = suggested.contains(&t.name.as_str());
                (t, pick)
            })
            .collect();
        self.data.import = Some(ImportDialog { file, sql, tables });
    }

    fn import_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.data.import else {
            return;
        };
        let existing = &self.data.names;
        let mut open = true;
        let mut import = false;
        egui::Window::new(theme::icon_text(icons::DOWNLOAD_SIMPLE, "Import tables"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([420.0, 460.0])
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(dialog.file.display().to_string())
                        .color(theme::TEXT_DIM)
                        .small(),
                );
                ui.horizontal(|ui| {
                    if ui.small_button("All").clicked() {
                        dialog.tables.iter_mut().for_each(|t| t.1 = true);
                    }
                    if ui.small_button("None").clicked() {
                        dialog.tables.iter_mut().for_each(|t| t.1 = false);
                    }
                });
                egui::ScrollArea::vertical()
                    .max_height(340.0)
                    .show(ui, |ui| {
                        for (table, pick) in &mut dialog.tables {
                            ui.horizontal(|ui| {
                                ui.checkbox(pick, &table.name);
                                ui.label(
                                    RichText::new(format!(
                                        "{} rows · {} columns",
                                        table.rows, table.columns
                                    ))
                                    .color(theme::TEXT_DIM)
                                    .small(),
                                );
                                if existing.contains(&table.name) {
                                    ui.label(
                                        RichText::new("replaces").color(theme::WARNING).small(),
                                    )
                                    .on_hover_text("The project already has this table");
                                }
                            });
                        }
                    });
                ui.separator();
                let count = dialog.tables.iter().filter(|t| t.1).count();
                import = ui
                    .add_enabled(
                        count > 0,
                        egui::Button::new(format!("Import {count} tables")).fill(theme::ACCENT),
                    )
                    .clicked();
            });
        if import {
            self.import_picked();
        } else if !open {
            self.data.import = None;
        }
    }

    pub(crate) fn import_picked(&mut self) {
        let (Some(dialog), Some(project)) = (self.data.import.take(), &self.project) else {
            return;
        };
        let names: Vec<&str> = dialog
            .tables
            .iter()
            .filter(|t| t.1)
            .map(|t| t.0.name.as_str())
            .collect();
        let mut saved = 0;
        let mut failures = Vec::new();
        for table in sql::import_tables(&dialog.sql, &names) {
            match c3_data::save_table(&table, &project.table_file(&table.name)) {
                Ok(()) => saved += 1,
                Err(e) => failures.push(format!("Can't save table {}: {e}", table.name)),
            }
        }
        for failure in failures {
            self.log_error(failure);
        }
        let reopen = self.data.open.as_ref().map(|t| t.name.clone());
        self.refresh_tables();
        if let Some(name) = reopen.filter(|n| names.contains(&n.as_str())) {
            self.data.open = None;
            self.load_table(&name);
        }
        self.notify(format!("Imported {saved} tables"));
    }
}

/// Tooltip for a column header: its kind and description.
fn column_hint(c: &c3_data::Column) -> String {
    let kind = match c.kind {
        ColumnKind::Int => "Whole number",
        ColumnKind::Float => "Decimal number",
        ColumnKind::Text => "Text",
        ColumnKind::Bool => "Yes / no",
    };
    if c.doc.is_empty() {
        kind.to_owned()
    } else {
        format!("{kind}: {}", c.doc)
    }
}
