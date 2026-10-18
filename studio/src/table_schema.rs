//! Editing a table's shape in the Data panel: the column header menu (rename, type, what the
//! column is for, the table it refers to, insert, delete), and references between tables —
//! cells that name another table's row show its name, and the Inspector picks it.

use crate::app::Studio;
use crate::icons;
use crate::theme;
use c3_data::{Column, ColumnKind, Table};
use c3_edit::TableCommand;
use eframe::egui::{self, RichText};
use std::collections::HashMap;

/// What a column's header menu asked for.
pub(crate) enum ColumnAction {
    Rename(String),
    Describe(String),
    Retype(ColumnKind),
    /// Refer to this table's rows (empty: to none).
    Refer(String),
    InsertAfter,
    Delete,
}

/// A column being renamed or described: which, and the text typed so far.
pub(crate) struct ColumnText {
    pub column: String,
    pub text: String,
    /// Renaming (else describing).
    pub rename: bool,
}

/// Tables other columns refer to, read once (and again after a table is saved): each row's
/// key and label, in table order.
#[derive(Default)]
pub(crate) struct ReferredTables {
    tables: HashMap<String, Option<Vec<(i64, String)>>>,
}

impl ReferredTables {
    pub fn forget(&mut self, name: Option<&str>) {
        match name {
            Some(name) => {
                self.tables.remove(name);
            }
            None => self.tables.clear(),
        }
    }

    /// What a cell naming row `key` of `table` shows.
    pub fn label(&self, table: &str, key: i64) -> String {
        match self.tables.get(table) {
            Some(Some(rows)) => match rows.iter().find(|(k, _)| *k == key) {
                Some((_, label)) => label.clone(),
                None if key == 0 => "none".to_owned(),
                None => format!("{key} (not in {table})"),
            },
            _ => key.to_string(),
        }
    }
}

const KINDS: [(ColumnKind, &str); 4] = [
    (ColumnKind::Int, "Whole number"),
    (ColumnKind::Float, "Decimal number"),
    (ColumnKind::Text, "Text"),
    (ColumnKind::Bool, "Yes / no"),
];

/// The menu of column `c`; `tables` are the project's tables it may refer to.
pub(crate) fn column_menu(
    ui: &mut egui::Ui,
    c: &Column,
    key: bool,
    tables: &[String],
) -> Option<ColumnAction> {
    let mut action = None;
    if ui
        .button(theme::icon_text(icons::PENCIL_SIMPLE, "Rename…"))
        .clicked()
    {
        action = Some(ColumnAction::Rename(c.name.clone()));
    }
    if ui
        .button(theme::icon_text(icons::INFO, "Describe…"))
        .on_hover_text("What the column means, shown on hover")
        .clicked()
    {
        action = Some(ColumnAction::Describe(c.doc.clone()));
    }
    if !key {
        ui.menu_button(theme::icon_text(icons::SLIDERS, "Type"), |ui| {
            for (kind, label) in KINDS {
                if ui.selectable_label(c.kind == kind, label).clicked() {
                    action = Some(ColumnAction::Retype(kind));
                    ui.close();
                }
            }
        });
    }
    if c.kind == ColumnKind::Int && !key {
        ui.menu_button(theme::icon_text(icons::LINK_SIMPLE, "Refers to"), |ui| {
            ui.label(
                RichText::new("Rows of a table, by key (an item, a monster…)")
                    .small()
                    .color(theme::TEXT_DIM),
            );
            if ui
                .selectable_label(c.references.is_empty(), "No table")
                .clicked()
            {
                action = Some(ColumnAction::Refer(String::new()));
                ui.close();
            }
            for t in tables {
                if ui.selectable_label(&c.references == t, t).clicked() {
                    action = Some(ColumnAction::Refer(t.clone()));
                    ui.close();
                }
            }
        });
    }
    ui.separator();
    if ui
        .button(theme::icon_text(icons::PLUS, "Insert column after"))
        .clicked()
    {
        action = Some(ColumnAction::InsertAfter);
    }
    if !key
        && ui
            .button(theme::icon_text(icons::TRASH, "Delete column"))
            .clicked()
    {
        action = Some(ColumnAction::Delete);
    }
    if action.is_some() {
        ui.close();
    }
    action
}

impl Studio {
    /// Does what column `name`'s header menu asked for.
    pub(crate) fn column_action(&mut self, name: &str, action: ColumnAction) {
        let Some(open) = &self.data.open else { return };
        let table = open.doc.table();
        let Some(c) = table.column(name) else { return };
        let column = table.columns[c].clone();
        let set = |column: Column| TableCommand::SetColumn {
            name: name.to_owned(),
            column,
        };
        let command = match action {
            ColumnAction::Rename(text) => {
                self.edit_column_text(name, text, true);
                return;
            }
            ColumnAction::Describe(text) => {
                self.edit_column_text(name, text, false);
                return;
            }
            ColumnAction::Retype(kind) => set(Column { kind, ..column }),
            ColumnAction::Refer(references) => set(Column {
                references,
                ..column
            }),
            ColumnAction::InsertAfter => {
                let free = (1..)
                    .map(|n| format!("column_{n}"))
                    .find(|n| table.column(n).is_none())
                    .unwrap_or_default();
                let new = free.clone();
                self.run_table(TableCommand::AddColumn {
                    column: Column::new(free, ColumnKind::Int),
                    index: Some(c + 1),
                });
                self.edit_column_text(&new.clone(), new, true);
                return;
            }
            ColumnAction::Delete => TableCommand::RemoveColumn {
                name: name.to_owned(),
            },
        };
        self.run_table(command);
    }

    fn edit_column_text(&mut self, column: &str, text: String, rename: bool) {
        self.data.column_text = Some(ColumnText {
            column: column.to_owned(),
            text,
            rename,
        });
    }

    /// The rename / describe dialog, while one is open.
    pub(crate) fn column_text_window(&mut self, ctx: &egui::Context) {
        let Some(edit) = &mut self.data.column_text else {
            return;
        };
        let (mut done, mut cancel) = (false, false);
        let title = match edit.rename {
            true => format!("Rename column {}", edit.column),
            false => format!("Describe column {}", edit.column),
        };
        egui::Window::new(theme::icon_text(icons::PENCIL_SIMPLE, &title))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                let field = ui.add(egui::TextEdit::singleline(&mut edit.text).desired_width(320.0));
                field.request_focus();
                done = ui.input(|i| i.key_pressed(egui::Key::Enter));
                cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
                ui.horizontal(|ui| {
                    done |= ui.button("OK").clicked();
                    cancel |= ui.button("Cancel").clicked();
                });
            });
        if cancel {
            self.data.column_text = None;
        } else if done && let Some(edit) = self.data.column_text.take() {
            let command = if edit.rename {
                TableCommand::RenameColumn {
                    from: edit.column,
                    to: edit.text.trim().to_owned(),
                }
            } else {
                let Some(open) = &self.data.open else { return };
                let table = open.doc.table();
                let Some(c) = table.column(&edit.column) else {
                    return;
                };
                TableCommand::SetColumn {
                    name: edit.column,
                    column: Column {
                        doc: edit.text.trim().to_owned(),
                        ..table.columns[c].clone()
                    },
                }
            };
            self.run_table(command);
        }
    }

    /// The rows of table `name` (key and label) as another table's column refers to it,
    /// read once.
    pub(crate) fn referred(&mut self, name: &str) -> Option<&[(i64, String)]> {
        let project = self.project.as_ref()?;
        let path = project.table_file(name);
        let entry = self.data.referred.tables.entry(name.to_owned());
        entry
            .or_insert_with(|| c3_data::load_table(&path).ok().map(|t| labels(&t)))
            .as_deref()
    }

    /// The row Inspector's field for a column referring to `table`: the row it names, a
    /// searchable picker, and a jump to that row. Returns a newly picked key.
    pub(crate) fn reference_field(
        &mut self,
        ui: &mut egui::Ui,
        table: &str,
        key: i64,
    ) -> Option<i64> {
        let rows: Vec<(i64, String)> = self.referred(table).map(<[_]>::to_vec).unwrap_or_default();
        let label = self.data.referred.label(table, key);
        let mut picked = None;
        let mut jump = false;
        ui.horizontal(|ui| {
            ui.menu_button(label, |ui| {
                let labels: Vec<&str> = rows.iter().map(|(_, l)| l.as_str()).collect();
                if let Some(i) = crate::hierarchy::search_list(
                    ui,
                    &mut self.data.reference_search,
                    "Search rows",
                    &labels,
                ) {
                    picked = Some(rows[i].0);
                    ui.close();
                }
            });
            jump = ui
                .button(icons::ARROW_SQUARE_OUT)
                .on_hover_text(format!("Open {table} at this row"))
                .clicked();
        });
        if jump {
            self.open_table(table);
            if self.data.open.as_ref().is_some_and(|t| t.name == table) {
                self.data.selected = Some(key);
            }
        }
        picked
    }
}

/// Every row of `table` as its key and label.
fn labels(table: &Table) -> Vec<(i64, String)> {
    (0..table.rows.len())
        .filter_map(|r| table.key(r))
        .map(|k| (k, table.row_label(k)))
        .collect()
}
