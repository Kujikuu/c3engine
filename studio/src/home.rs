//! Projects in the Studio: the start page (new, open, import, recent), the project dialogs
//! and the File menu.

use crate::app::{Pending, Studio};
use crate::theme;
use crate::tq_plugin;
use c3_project::{
    CopyProgress, Manifest, Project, Source, copy_entries, default_projects_dir, folder_name,
};
use eframe::egui::{self, Color32, RichText};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// The client a Conquer import reads (`Source::TqClient::game`).
const CONQUER_5065: &str = "conquer-5065";

/// A project dialog being filled in.
pub(crate) enum ProjectDialog {
    New(NewProject),
    ImportConquer(NewProject),
}

/// Form for creating a project.
pub(crate) struct NewProject {
    name: String,
    /// Folder the project folder is created in.
    location: PathBuf,
    /// TQ client folder to import from (import only).
    client: String,
    /// Copy the client's content into the project (else use the folder where it is).
    copy: bool,
    error: Option<String>,
}

/// A client being copied into a new project, on a background thread.
pub(crate) struct ImportJob {
    project: Project,
    progress: Arc<Mutex<CopyProgress>>,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<bool, String>>>,
}

impl NewProject {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            location: default_projects_dir(),
            client: String::new(),
            copy: true,
            error: None,
        }
    }

    /// Folder the project will be created in.
    fn target(&self) -> PathBuf {
        self.location.join(folder_name(&self.name))
    }
}

/// Picks a folder with the system dialog.
fn pick_folder(title: &str, start: &Path) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .set_directory(start)
        .pick_folder()
}

impl Studio {
    /// Opens the New Project dialog.
    pub(crate) fn show_new_project(&mut self) {
        self.dialog = Some(ProjectDialog::New(NewProject::new("New Game")));
    }

    /// Opens the Import dialog, with `client` as the client folder if given.
    pub(crate) fn show_import_conquer(&mut self, client: Option<&str>) {
        let mut form = NewProject::new("Conquer");
        form.client = client.unwrap_or_default().to_owned();
        self.dialog = Some(ProjectDialog::ImportConquer(form));
    }

    // ---- project lifecycle ----

    /// Opens a project, asking first if the current map has unsaved changes.
    pub(crate) fn request_open_project(&mut self, path: PathBuf) {
        if self.is_dirty() {
            self.pending = Some(Pending::OpenProject(path));
        } else {
            self.open_project(&path);
        }
    }

    pub(crate) fn open_project(&mut self, path: &Path) {
        match Project::open(path) {
            Ok(project) => self.set_project(project),
            Err(e) => {
                self.error = Some(format!("Can't open project: {e}"));
                if !Project::is_project(path) {
                    self.recent.remove(path);
                    self.save_recent();
                }
            }
        }
    }

    fn set_project(&mut self, project: Project) {
        self.close_documents();
        self.recent.touch(&project);
        self.save_recent();
        self.project = Some(project);
        self.open_client();
        self.notify(format!("Opened {}", self.project_name()));
    }

    pub(crate) fn request_close_project(&mut self) {
        if self.is_dirty() {
            self.pending = Some(Pending::CloseProject);
        } else {
            self.close_project();
        }
    }

    pub(crate) fn close_project(&mut self) {
        self.close_documents();
        self.project = None;
        self.client = None;
        self.maps.clear();
        self.models.clear();
    }

    pub(crate) fn project_name(&self) -> &str {
        self.project.as_ref().map_or("", Project::name)
    }

    fn save_recent(&mut self) {
        if let Err(e) = self.recent.save() {
            self.error = Some(format!("Can't save the recent projects list: {e}"));
        }
    }

    fn browse_open_project(&mut self) {
        if let Some(dir) = pick_folder("Open project folder", &default_projects_dir()) {
            if Project::is_project(&dir) {
                self.request_open_project(dir);
            } else {
                self.error = Some(format!(
                    "{} is not a project folder (it has no {}).",
                    dir.display(),
                    c3_project::MANIFEST
                ));
            }
        }
    }

    /// Inspector section: the open project and its content source.
    pub(crate) fn project_section(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            return;
        };
        let root = project.root.clone();
        let source = project
            .tq_client()
            .map(|(game, dir)| (game.to_owned(), dir));
        let in_project = project.content_in_project();
        let mut change_client = false;
        theme::section(ui, "Project", |ui| {
            theme::kv(ui, "Name", project.name());
            theme::kv(ui, "Folder", &root.display().to_string());
            if ui
                .small_button("Show in Explorer")
                .on_hover_text("Open the project folder")
                .clicked()
            {
                // Opening a file browser is a convenience; failing to is harmless.
                let _ = std::process::Command::new("explorer").arg(&root).spawn();
            }
            ui.add_space(4.0);
            match &source {
                Some((game, dir)) => {
                    theme::kv(ui, "Content", source_label(game));
                    if in_project {
                        theme::kv(ui, "Stored in", "assets/ (copied into the project)");
                    } else {
                        theme::kv(ui, "Used in place", &dir.display().to_string());
                    }
                    if self.client.is_some() {
                        let found =
                            format!("{} maps, {} models", self.maps.len(), self.models.len());
                        ui.label(RichText::new(found).weak());
                    } else {
                        ui.colored_label(theme::DANGER, "The content folder can't be read.");
                    }
                    if !in_project {
                        change_client = ui
                            .small_button("Change client folder…")
                            .on_hover_text("Point the project at another copy of the client")
                            .clicked();
                    }
                }
                None => {
                    ui.label(RichText::new("Content lives in the project folder.").weak());
                }
            }
        });
        if change_client {
            let start = source.map_or_else(default_projects_dir, |(_, dir)| dir);
            if let Some(dir) = pick_folder("Conquer 5065 client folder", &start) {
                self.set_client_folder(dir);
            }
        }
    }

    /// Points the project's TQ client source at `dir` and reloads it.
    fn set_client_folder(&mut self, dir: PathBuf) {
        if let Err(e) = tq_plugin::check_client(&dir) {
            self.error = Some(e);
            return;
        }
        let Some(project) = &mut self.project else {
            return;
        };
        for source in &mut project.manifest.sources {
            let Source::TqClient { path, .. } = source;
            *path = dir.clone();
        }
        if let Err(e) = project.save() {
            self.error = Some(format!("Can't save the project: {e}"));
            return;
        }
        self.close_documents();
        self.open_client();
    }

    // ---- File menu ----

    pub(crate) fn file_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("File", |ui| {
            if ui.button("New Project…").clicked() {
                self.show_new_project();
            }
            if ui.button("Open Project…").clicked() {
                self.browse_open_project();
            }
            if ui.button("Import Conquer 5065 Client…").clicked() {
                self.show_import_conquer(None);
            }
            ui.menu_button("Open Recent", |ui| {
                let entries = self.recent.entries.clone();
                if entries.is_empty() {
                    ui.label(RichText::new("No recent projects").weak());
                }
                for e in entries {
                    let label = format!("{}  —  {}", e.name, e.path.display());
                    if ui
                        .add_enabled(e.exists(), egui::Button::new(label))
                        .clicked()
                    {
                        self.request_open_project(e.path);
                    }
                }
            });
            ui.separator();
            if ui
                .add_enabled(self.project.is_some(), egui::Button::new("Close Project"))
                .clicked()
            {
                self.request_close_project();
            }
        });
    }

    // ---- start page ----

    /// Shown when no project is open.
    pub(crate) fn home(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.12).min(90.0));
            ui.label(
                RichText::new("C3 Studio")
                    .size(34.0)
                    .strong()
                    .color(theme::ACCENT),
            );
            ui.label(RichText::new("Make games like TQ's C3 engine did, all in one place.").weak());
            ui.add_space(28.0);
        });
        let width = ui.available_width().min(860.0);
        let margin = (ui.available_width() - width) / 2.0;
        ui.horizontal_top(|ui| {
            ui.add_space(margin);
            ui.vertical(|ui| {
                ui.set_width(260.0);
                ui.label(RichText::new("Start").strong().size(16.0));
                ui.add_space(6.0);
                if start_button(ui, "+   New Project", "An empty game project").clicked() {
                    self.show_new_project();
                }
                if start_button(ui, "📂  Open Project", "A project folder on disk").clicked() {
                    self.browse_open_project();
                }
                if start_button(
                    ui,
                    "⤓  Import Conquer 5065",
                    "A project reading an original client folder",
                )
                .clicked()
                {
                    self.show_import_conquer(None);
                }
            });
            ui.add_space(40.0);
            ui.vertical(|ui| {
                ui.set_width(width - 300.0);
                ui.label(RichText::new("Recent projects").strong().size(16.0));
                ui.add_space(6.0);
                self.recent_list(ui);
            });
        });
    }

    fn recent_list(&mut self, ui: &mut egui::Ui) {
        if self.recent.entries.is_empty() {
            ui.label(RichText::new("Projects you open will appear here.").weak());
            return;
        }
        let mut open = None;
        let mut forget = None;
        for e in &self.recent.entries {
            let exists = e.exists();
            egui::Frame::group(ui.style())
                .fill(ui.visuals().faint_bg_color)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            let name = RichText::new(&e.name).strong().size(15.0);
                            let r = ui.add_enabled(exists, egui::Button::new(name).frame(false));
                            if r.clicked() {
                                open = Some(e.path.clone());
                            }
                            let path = e.path.display().to_string();
                            if exists {
                                ui.label(RichText::new(path).weak().small());
                            } else {
                                ui.label(
                                    RichText::new(format!("Missing: {path}"))
                                        .small()
                                        .color(theme::WARNING),
                                );
                            }
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("✕")
                                .on_hover_text("Remove from this list (the folder is kept)")
                                .clicked()
                            {
                                forget = Some(e.path.clone());
                            }
                        });
                    });
                });
        }
        if let Some(path) = forget {
            self.recent.remove(&path);
            self.save_recent();
        }
        if let Some(path) = open {
            self.request_open_project(path);
        }
    }

    // ---- dialogs ----

    pub(crate) fn project_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let (title, form, import) = match dialog {
            ProjectDialog::New(f) => ("New Project", f, false),
            ProjectDialog::ImportConquer(f) => ("Import Conquer 5065 Client", f, true),
        };
        let mut create = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::unique("project dialog")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.heading(title);
            if import {
                ui.label(
                    RichText::new(
                        "Creates a project from an original 5065 client. Its maps, models, \
                         effects and tables are used as they are, like files in a Unity \
                         project; nothing is converted.",
                    )
                    .weak(),
                );
            }
            ui.add_space(8.0);
            egui::Grid::new("project form")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    if import {
                        ui.label("Client folder");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut form.client)
                                    .hint_text("e.g. D:\\Conquer\\client5065")
                                    .desired_width(330.0),
                            );
                            if ui.button("Browse…").clicked()
                                && let Some(dir) = pick_folder(
                                    "Conquer 5065 client folder",
                                    Path::new(&form.client),
                                )
                            {
                                form.client = dir.display().to_string();
                            }
                        });
                        ui.end_row();
                        ui.label("Content");
                        ui.vertical(|ui| {
                            ui.radio_value(
                                &mut form.copy,
                                true,
                                "Copy into the project (recommended)",
                            )
                            .on_hover_text(
                                "About 1.3 GB of content files go into the project's assets \
                                 folder. The original client is never touched.",
                            );
                            ui.radio_value(
                                &mut form.copy,
                                false,
                                "Use the client folder where it is",
                            )
                            .on_hover_text(
                                "Nothing is copied. Saving a map writes into the client \
                                     folder and keeps the original as .bak.",
                            );
                        });
                        ui.end_row();
                    }
                    ui.label("Name");
                    let r = ui.add(egui::TextEdit::singleline(&mut form.name).desired_width(330.0));
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        create = !form.name.trim().is_empty();
                    }
                    ui.end_row();
                    ui.label("Location");
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(form.location.display().to_string()).truncate());
                        if ui.button("Browse…").clicked()
                            && let Some(dir) =
                                pick_folder("Where to create the project", &form.location)
                        {
                            form.location = dir;
                        }
                    });
                    ui.end_row();
                });
            ui.add_space(4.0);
            let target = form.target();
            let taken = std::fs::read_dir(&target).is_ok_and(|mut d| d.next().is_some());
            if taken {
                ui.colored_label(
                    theme::WARNING,
                    format!(
                        "{} already exists and isn't empty. Choose another name or location.",
                        target.display()
                    ),
                );
            } else {
                ui.label(
                    RichText::new(format!("Creates {}", target.display()))
                        .small()
                        .weak(),
                );
            }
            if let Some(e) = &form.error {
                ui.add_space(4.0);
                ui.colored_label(theme::DANGER, e);
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let ready = !taken
                    && !form.name.trim().is_empty()
                    && (!import || !form.client.trim().is_empty());
                let label = if import { "Import" } else { "Create" };
                let button = egui::Button::new(RichText::new(label).color(Color32::BLACK))
                    .fill(theme::ACCENT);
                if ui.add_enabled(ready, button).clicked() {
                    create = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if cancel || modal.should_close() {
            self.dialog = None;
        } else if create {
            self.create_project();
        }
    }

    /// Creates the project described by the open dialog, then opens it.
    fn create_project(&mut self) {
        let (form, import) = match &mut self.dialog {
            Some(ProjectDialog::New(form)) => (form, false),
            Some(ProjectDialog::ImportConquer(form)) => (form, true),
            None => return,
        };
        let mut manifest = Manifest::new(form.name.trim());
        let client = PathBuf::from(form.client.trim());
        let copy = import && form.copy;
        if import {
            if let Err(e) = tq_plugin::check_client(&client) {
                form.error = Some(e);
                return;
            }
            manifest.sources.push(Source::TqClient {
                game: CONQUER_5065.to_owned(),
                // Copied content lives in the project, referred to relatively.
                path: if copy {
                    PathBuf::from("assets")
                } else {
                    client.clone()
                },
            });
        }
        match Project::create(form.target(), manifest) {
            Ok(project) => {
                self.dialog = None;
                if copy {
                    self.import_job = Some(ImportJob::start(project, client));
                } else {
                    self.set_project(project);
                }
            }
            Err(e) => form.error = Some(e.to_string()),
        }
    }

    /// Progress window for a client being copied in; opens the project when done.
    pub(crate) fn import_progress(&mut self, ctx: &egui::Context) {
        let Some(job) = &mut self.import_job else {
            return;
        };
        if job.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            let result = job.worker.take().map_or(Ok(false), |w| {
                w.join().unwrap_or(Err("the copy crashed".into()))
            });
            let Some(job) = self.import_job.take() else {
                return;
            };
            let root = job.project.root.clone();
            match result {
                Ok(true) => {
                    self.set_project(job.project);
                    return;
                }
                Ok(false) => self.notify("Import cancelled; the new project was removed".into()),
                Err(e) => self.error = Some(format!("Import failed: {e}")),
            }
            // A half-copied project is no use: remove the folder this import created.
            let _ = std::fs::remove_dir_all(root);
            return;
        }
        let p = job.progress.lock().map(|p| *p).unwrap_or_default();
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("import progress")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.heading(format!("Importing {}", job.project.name()));
            ui.label(RichText::new("Copying the client's content into the project…").weak());
            ui.add_space(8.0);
            ui.add(egui::ProgressBar::new(p.fraction()).show_percentage());
            ui.label(
                RichText::new(format!(
                    "{} / {} files  ·  {} / {}",
                    p.files_done,
                    p.files_total,
                    size_text(p.bytes_done),
                    size_text(p.bytes_total)
                ))
                .small()
                .weak(),
            );
            ui.add_space(8.0);
            cancel = ui.button("Cancel").clicked();
        });
        if cancel {
            job.cancel.store(true, Ordering::Relaxed);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

impl ImportJob {
    /// Copies the client's content into `project`'s assets folder in the background.
    fn start(project: Project, client: PathBuf) -> Self {
        let progress = Arc::new(Mutex::new(CopyProgress::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let assets = project.assets_dir();
        let worker = {
            let (progress, cancel) = (Arc::clone(&progress), Arc::clone(&cancel));
            std::thread::spawn(move || {
                copy_entries(&client, &assets, &tq_plugin::CONTENT_ENTRIES, |p| {
                    if let Ok(mut shared) = progress.lock() {
                        *shared = p;
                    }
                    !cancel.load(Ordering::Relaxed)
                })
                .map_err(|e| e.to_string())
            })
        };
        Self {
            project,
            progress,
            cancel,
            worker: Some(worker),
        }
    }
}

/// `1.3 GB`, `420 MB`, `12 KB`.
fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit >= 3 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

/// Display name of a TQ client source's `game`.
fn source_label(game: &str) -> &str {
    match game {
        CONQUER_5065 => "Conquer 5065 client",
        other => other,
    }
}

/// A large start-page action button with a description.
fn start_button(ui: &mut egui::Ui, label: &str, hint: &str) -> egui::Response {
    let text = RichText::new(label).size(15.0);
    let r = ui.add_sized([250.0, 38.0], egui::Button::new(text));
    ui.label(RichText::new(hint).small().weak());
    ui.add_space(8.0);
    r
}
