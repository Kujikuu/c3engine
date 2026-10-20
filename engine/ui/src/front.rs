//! The client's front end, before the world: log in, pick a server, pick or make a
//! character, and wait while the world loads. A game whose UI (see [`crate::gui`]) has
//! dialogs for a screen (`screen=login`, `servers`, `characters`, `classes`, `create`,
//! `loading`) shows
//! those; other screens are the engine's own.
//!
//! The front end only asks: the host connects, logs in and loads, and tells it what came of
//! that (the characters, a refusal, how far loading got).

use crate::gui::{Gui, GuiData, GuiEvent, Textures};
use crate::{fonts, icons, style};
use egui::{Align2, Rect, RichText};

/// Which screen is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontScreen {
    Login,
    Servers,
    Characters,
    /// Picking a class before the look (TQ's RoleChoose), when the game has classes.
    Classes,
    Create,
    Loading,
}

impl FrontScreen {
    /// The name dialogs give the screen (`screen=`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Servers => "servers",
            Self::Characters => "characters",
            Self::Classes => "classes",
            Self::Create => "create",
            Self::Loading => "loading",
        }
    }
}

/// Group pictures a login shows at once (CDlgLogin's four; the rest scroll).
pub const GROUP_SLOTS: usize = 4;

/// A server on the login: its name, group (an index of the groups) and picture.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerRow {
    pub name: String,
    pub group: usize,
    /// A skin section.
    pub icon: String,
}

/// A group of servers on the login: its name and picture (a skin section).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupRow {
    pub name: String,
    pub icon: String,
}

/// A class creation offers: its name, what it is, and which looks it may wear.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClassRow {
    pub name: String,
    pub description: String,
    /// Indices of the looks (none: all of them).
    pub looks: Vec<usize>,
}

/// A character on the character screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterRow {
    pub name: String,
    pub level: u32,
    /// Which of the creation looks it wears, if one.
    pub look: Option<usize>,
}

/// What the player asked the host for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontAction {
    /// Connect to server `server` and ask for the account's characters.
    Login {
        server: usize,
    },
    /// Play the character of this name.
    Enter(String),
    /// Make a character: its name, which of the creation looks and which class.
    Create {
        name: String,
        look: usize,
        class: Option<usize>,
    },
    Quit,
}

/// The front end's state: what was typed and picked, and what the host said.
pub struct FrontEnd {
    pub screen: FrontScreen,
    pub account: String,
    pub password: String,
    /// The servers to pick from and the one picked (an index of all of them).
    pub servers: Vec<ServerRow>,
    pub server: usize,
    /// The groups servers are sorted into, the one shown, and the first group pictured.
    pub groups: Vec<GroupRow>,
    pub group: usize,
    pub group_top: usize,
    pub characters: Vec<CharacterRow>,
    pub character: usize,
    /// The looks creation offers (their names) and the one picked.
    pub looks: Vec<String>,
    pub look: usize,
    pub new_name: String,
    /// What went wrong last (a refusal), shown on the screen.
    pub message: Option<String>,
    /// Loading, 0 to 1.
    pub progress: f32,
    /// The game's name, over the login.
    pub title: String,
    /// Each creation look drawn by the host, by look.
    pub previews: std::collections::HashMap<usize, egui::TextureId>,
    /// The classes creation starts with (none: it starts at the looks) and the one picked.
    pub classes: Vec<ClassRow>,
    pub class: usize,
    /// The picked class's figure, drawn by the host.
    pub class_preview: Option<egui::TextureId>,
    /// An account's one character is entered at once, with no list (one per account).
    pub enter_alone: bool,
    gui: Option<Gui>,
}

impl FrontEnd {
    pub fn new(
        title: String,
        servers: Vec<ServerRow>,
        groups: Vec<GroupRow>,
        looks: Vec<String>,
        gui: Option<Gui>,
    ) -> Self {
        Self {
            screen: FrontScreen::Login,
            account: String::new(),
            password: String::new(),
            group: servers.first().map_or(0, |s| s.group),
            servers,
            server: 0,
            groups,
            group_top: 0,
            characters: Vec::new(),
            character: 0,
            looks,
            look: 0,
            new_name: String::new(),
            message: None,
            progress: 0.0,
            title,
            previews: Default::default(),
            classes: Vec::new(),
            class: 0,
            class_preview: None,
            enter_alone: false,
            gui,
        }
    }

    /// The host connected: these are the account's characters (none: make the first).
    /// Returns the character to enter at once, when the account's one is entered alone.
    pub fn got_characters(&mut self, characters: Vec<CharacterRow>) -> Option<FrontAction> {
        self.characters = characters;
        self.character = 0;
        self.message = None;
        if self.enter_alone && self.characters.len() == 1 {
            self.screen = FrontScreen::Loading;
            return Some(FrontAction::Enter(self.characters[0].name.clone()));
        }
        if self.characters.is_empty() {
            self.start_creation();
        } else {
            self.screen = FrontScreen::Characters;
        }
        None
    }

    /// Opens creation: at the classes when the game has them, else at the looks.
    fn start_creation(&mut self) {
        self.message = None;
        if self.classes.is_empty() {
            self.screen = FrontScreen::Create;
        } else {
            self.screen = FrontScreen::Classes;
        }
    }

    /// Picks class `class` and the first look it offers.
    fn pick_class(&mut self, class: usize) {
        if class < self.classes.len() {
            self.class = class;
            self.look = self.offered_looks().first().copied().unwrap_or(0);
        }
    }

    /// The looks creation offers now: the picked class's, else all of them.
    fn offered_looks(&self) -> Vec<usize> {
        match self.classes.get(self.class) {
            Some(class) if !class.looks.is_empty() => class.looks.clone(),
            _ => (0..self.looks.len()).collect(),
        }
    }

    /// Steps the picked look `by` places through the offered ones.
    fn step_look(&mut self, by: isize) {
        let offered = self.offered_looks();
        if offered.is_empty() {
            return;
        }
        let at = offered.iter().position(|&l| l == self.look).unwrap_or(0) as isize;
        let next = (at + by).rem_euclid(offered.len() as isize) as usize;
        self.look = offered[next];
    }

    /// The host was refused (or lost the server): back to the login with `why`.
    pub fn refused(&mut self, why: String) {
        self.screen = FrontScreen::Login;
        self.message = Some(why);
    }

    /// Shows the screen over `rect`; returns what the player asked for.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        textures: &mut dyn Textures,
    ) -> Vec<FrontAction> {
        fonts::use_game_font(ui);
        let mut actions = Vec::new();
        let screen = self.screen.name();
        if let Some(mut gui) = self.gui.take() {
            if gui.has_screen(screen) {
                gui.set_screen(screen);
                let events = gui.show(ui.ctx(), rect, &*self, textures);
                for event in events {
                    self.gui_event(event, &mut actions);
                }
                self.gui = Some(gui);
                return actions;
            }
            self.gui = Some(gui);
        }
        ui.painter()
            .rect_filled(rect, 0.0, egui::Color32::from_rgb(12, 14, 18));
        egui::Area::new(egui::Id::unique("front end"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.center())
            .pivot(Align2::CENTER_CENTER)
            .constrain_to(rect)
            .show(ui.ctx(), |ui| {
                fonts::use_game_font(ui);
                style::panel().show(ui, |ui| {
                    ui.set_width(320.0);
                    match self.screen {
                        FrontScreen::Login => self.login(ui, &mut actions),
                        FrontScreen::Servers => self.server_list(ui, &mut actions),
                        FrontScreen::Characters => self.character_list(ui, &mut actions),
                        FrontScreen::Classes => self.class_list(ui, &mut actions),
                        FrontScreen::Create => self.create(ui, &mut actions),
                        FrontScreen::Loading => self.loading(ui),
                    }
                });
            });
        actions
    }

    fn gui_event(&mut self, event: GuiEvent, actions: &mut Vec<FrontAction>) {
        match event {
            GuiEvent::Edited { field, text } => match field.as_str() {
                "front.account" => self.account = text,
                "front.password" => self.password = text,
                "front.name" => self.new_name = text,
                _ => {}
            },
            GuiEvent::Item { list, index } => match list.as_str() {
                "front.servers" => self.pick_server(index),
                "front.characters" => self.character = index,
                "front.looks" => self.act(&format!("look:{index}"), actions),
                "front.classes" => self.pick_class(index),
                _ => {}
            },
            GuiEvent::Action(action) => self.act(&action, actions),
            // The front end plays no sounds yet, and its edits take no keys.
            GuiEvent::Sound(_)
            | GuiEvent::Pointed(_)
            | GuiEvent::EditKey { .. }
            | GuiEvent::Pick { .. }
            | GuiEvent::Put { .. }
            | GuiEvent::Slid { .. } => {}
        }
    }

    /// Carries out a named action (a button's `action=`).
    fn act(&mut self, action: &str, actions: &mut Vec<FrontAction>) {
        match action {
            "login" => self.log_in(actions),
            "servers" => self.screen = FrontScreen::Servers,
            "enter" => {
                if let Some(c) = self.characters.get(self.character) {
                    self.screen = FrontScreen::Loading;
                    actions.push(FrontAction::Enter(c.name.clone()));
                }
            }
            "new" => self.start_creation(),
            "choose" => {
                self.pick_class(self.class);
                self.screen = FrontScreen::Create;
            }
            "create" => self.make(actions),
            "next_look" => self.step_look(1),
            "prev_look" => self.step_look(-1),
            "back" => {
                self.message = None;
                self.screen = match self.screen {
                    FrontScreen::Create if !self.classes.is_empty() => FrontScreen::Classes,
                    FrontScreen::Create | FrontScreen::Classes if !self.characters.is_empty() => {
                        FrontScreen::Characters
                    }
                    _ => FrontScreen::Login,
                }
            }
            "quit" => actions.push(FrontAction::Quit),
            "group_up" => self.group_top = self.group_top.saturating_sub(1),
            "group_down" => {
                let last = self.groups.len().saturating_sub(GROUP_SLOTS);
                self.group_top = (self.group_top + 1).min(last);
            }
            other => {
                let slot = |prefix: &str| other.strip_prefix(prefix)?.parse::<usize>().ok();
                if let Some(n) = slot("look:") {
                    if let Some(&look) = self.offered_looks().get(n) {
                        self.look = look;
                    }
                } else if let Some(n) = slot("class:") {
                    self.pick_class(n);
                } else if let Some(n) = slot("group:") {
                    self.pick_group(self.group_top + n);
                } else if let Some(n) = slot("server:") {
                    self.pick_server(n);
                }
            }
        }
    }

    /// The servers of the group shown (all of them when there are no groups), as indices.
    fn shown_servers(&self) -> Vec<usize> {
        (0..self.servers.len())
            .filter(|&i| self.groups.is_empty() || self.servers[i].group == self.group)
            .collect()
    }

    /// Shows group `group`, its first server picked.
    fn pick_group(&mut self, group: usize) {
        if group >= self.groups.len() {
            return;
        }
        self.group = group;
        if let Some(&first) = self.shown_servers().first() {
            self.server = first;
        }
    }

    /// Picks the `n`th server shown.
    fn pick_server(&mut self, n: usize) {
        if let Some(&server) = self.shown_servers().get(n) {
            self.server = server;
        }
    }

    /// What a login slot shows: `front.group:N` (a group's picture), `front.server_icon:N`
    /// and `front.server_name:N` (the `N`th server shown).
    fn slot_text(&self, key: &str) -> Option<String> {
        let (prefix, n) = key.rsplit_once(':')?;
        let n: usize = n.parse().ok()?;
        let server = || self.shown_servers().get(n).map(|&i| &self.servers[i]);
        match prefix {
            "front.group" => self.groups.get(self.group_top + n).map(|g| g.icon.clone()),
            "front.server_icon" => server().map(|s| s.icon.clone()),
            "front.server_name" => server().map(|s| s.name.clone()),
            _ => None,
        }
    }

    fn server_name(&self) -> String {
        (self.servers.get(self.server)).map_or(String::new(), |s| s.name.clone())
    }

    fn log_in(&mut self, actions: &mut Vec<FrontAction>) {
        if self.account.trim().is_empty() || self.password.is_empty() {
            self.message = Some("Type your account and password.".into());
            return;
        }
        self.message = None;
        self.progress = 0.0;
        self.screen = FrontScreen::Loading;
        actions.push(FrontAction::Login {
            server: self.server,
        });
    }

    fn make(&mut self, actions: &mut Vec<FrontAction>) {
        let name = self.new_name.trim().to_owned();
        if name.is_empty() {
            self.message = Some("Name your character.".into());
            return;
        }
        self.message = None;
        self.screen = FrontScreen::Loading;
        actions.push(FrontAction::Create {
            name,
            look: self.look,
            class: (!self.classes.is_empty()).then_some(self.class),
        });
    }

    fn heading(ui: &mut egui::Ui, icon: &str, text: &str) {
        ui.vertical_centered(|ui| ui.label(style::title(icon, text).size(18.0)));
        ui.add_space(10.0);
    }

    fn message(&self, ui: &mut egui::Ui) {
        if let Some(why) = &self.message {
            ui.add_space(6.0);
            ui.label(RichText::new(why).color(style::BAD));
        }
    }

    fn wide(ui: &mut egui::Ui, icon: &str, text: &str) -> egui::Response {
        ui.add(
            egui::Button::new(RichText::new(format!("{icon}  {text}")).size(14.0))
                .min_size(egui::vec2(ui.available_width(), 30.0)),
        )
    }

    fn login(&mut self, ui: &mut egui::Ui, actions: &mut Vec<FrontAction>) {
        let title = if self.title.is_empty() {
            "Log in"
        } else {
            &self.title
        };
        Self::heading(ui, icons::USER, title);
        let field = |ui: &mut egui::Ui, icon: &str, text: &mut String, hint: &str, hide: bool| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon).color(style::DIM));
                ui.add(
                    egui::TextEdit::singleline(text)
                        .hint_text(hint)
                        .password(hide)
                        .desired_width(f32::INFINITY),
                )
            })
            .inner
        };
        let account = field(ui, icons::USER, &mut self.account, "Account", false);
        let password = field(ui, icons::LOCK, &mut self.password, "Password", true);
        if !account.has_focus() && !password.has_focus() && self.account.is_empty() {
            account.request_focus();
        }
        let enter = password.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if self.servers.len() > 1 {
            ui.add_space(4.0);
            let server = self.server_name();
            if ui
                .add(egui::Button::new(format!("{}  {server}", icons::GLOBE_SIMPLE)).frame(false))
                .on_hover_text("Pick another server")
                .clicked()
            {
                self.screen = FrontScreen::Servers;
            }
        }
        self.message(ui);
        ui.add_space(10.0);
        if Self::wide(ui, icons::PLAY, "Log in").clicked() || enter {
            self.log_in(actions);
        }
        if Self::wide(ui, icons::SIGN_OUT, "Quit").clicked() {
            actions.push(FrontAction::Quit);
        }
    }

    fn server_list(&mut self, ui: &mut egui::Ui, _actions: &mut [FrontAction]) {
        Self::heading(ui, icons::GLOBE_SIMPLE, "Servers");
        if self.groups.len() > 1 {
            let mut picked = None;
            ui.horizontal_wrapped(|ui| {
                for (g, group) in self.groups.iter().enumerate() {
                    if ui.selectable_label(self.group == g, &group.name).clicked() {
                        picked = Some(g);
                    }
                }
            });
            if let Some(g) = picked {
                self.pick_group(g);
            }
            ui.separator();
        }
        for i in self.shown_servers() {
            let row = egui::Button::selectable(self.server == i, &self.servers[i].name)
                .min_size(egui::vec2(ui.available_width(), 28.0));
            if ui.add(row).clicked() {
                self.server = i;
                self.screen = FrontScreen::Login;
            }
        }
        ui.add_space(8.0);
        if Self::wide(ui, icons::ARROW_LEFT, "Back").clicked() {
            self.screen = FrontScreen::Login;
        }
    }

    fn character_list(&mut self, ui: &mut egui::Ui, actions: &mut Vec<FrontAction>) {
        Self::heading(ui, icons::PERSON_SIMPLE_RUN, "Characters");
        for (i, c) in self.characters.iter().enumerate() {
            let look = (c.look.and_then(|l| self.looks.get(l)))
                .map_or(String::new(), |l| format!("  ·  {l}"));
            let text = format!("{}   Lv {}{look}", c.name, c.level);
            let row = egui::Button::selectable(self.character == i, text)
                .min_size(egui::vec2(ui.available_width(), 28.0));
            if ui.add(row).clicked() {
                self.character = i;
            }
        }
        self.message(ui);
        ui.add_space(10.0);
        if Self::wide(ui, icons::PLAY, "Enter the world").clicked() {
            self.act("enter", actions);
        }
        if Self::wide(ui, icons::PLUS, "New character").clicked() {
            self.act("new", actions);
        }
        if Self::wide(ui, icons::ARROW_LEFT, "Log out").clicked() {
            self.act("back", actions);
        }
    }

    fn create(&mut self, ui: &mut egui::Ui, actions: &mut Vec<FrontAction>) {
        Self::heading(ui, icons::PLUS, "New character");
        ui.add(
            egui::TextEdit::singleline(&mut self.new_name)
                .hint_text("Name")
                .desired_width(f32::INFINITY),
        );
        if let Some(&texture) = self.previews.get(&self.look) {
            ui.vertical_centered(|ui| {
                ui.add(egui::Image::new((texture, egui::vec2(160.0, 200.0))));
            });
        }
        if !self.looks.is_empty() {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(icons::ARROW_LEFT).clicked() {
                    self.act("prev_look", actions);
                }
                let look = self.looks.get(self.look).cloned().unwrap_or_default();
                ui.label(RichText::new(look).color(style::GOLD).size(15.0));
                if ui.button(icons::ARROW_RIGHT).clicked() {
                    self.act("next_look", actions);
                }
            });
        }
        self.message(ui);
        ui.add_space(10.0);
        if Self::wide(ui, icons::CHECK_CIRCLE, "Create").clicked() {
            self.make(actions);
        }
        if Self::wide(ui, icons::ARROW_LEFT, "Back").clicked() {
            self.act("back", actions);
        }
    }

    fn class_list(&mut self, ui: &mut egui::Ui, actions: &mut Vec<FrontAction>) {
        Self::heading(ui, icons::SHIELD, "Choose a class");
        let mut picked = None;
        egui::Grid::new("front classes")
            .num_columns(2)
            .spacing([6.0, 4.0])
            .show(ui, |ui| {
                for (i, class) in self.classes.iter().enumerate() {
                    let row = egui::Button::selectable(self.class == i, &class.name)
                        .min_size(egui::vec2(154.0, 26.0));
                    if ui.add(row).clicked() {
                        picked = Some(i);
                    }
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
        if let Some(i) = picked {
            self.pick_class(i);
        }
        if let Some(texture) = self.class_preview {
            ui.vertical_centered(|ui| {
                ui.add(egui::Image::new((texture, egui::vec2(160.0, 200.0))));
            });
        }
        if let Some(class) = self.classes.get(self.class) {
            ui.label(
                RichText::new(&class.description)
                    .color(style::DIM)
                    .size(12.0),
            );
        }
        ui.add_space(10.0);
        if Self::wide(ui, icons::ARROW_RIGHT, "Next").clicked() {
            self.act("choose", actions);
        }
        if Self::wide(ui, icons::ARROW_LEFT, "Back").clicked() {
            self.act("back", actions);
        }
    }

    fn loading(&mut self, ui: &mut egui::Ui) {
        Self::heading(ui, icons::HOURGLASS, "Entering the world");
        ui.add(egui::ProgressBar::new(self.progress.clamp(0.0, 1.0)).show_percentage());
    }
}

/// What the screen's dialogs read.
impl GuiData for FrontEnd {
    fn number(&self, key: &str) -> Option<f64> {
        let slot = |prefix: &str| key.strip_prefix(prefix)?.parse::<usize>().ok();
        let flag = |b: bool| Some(f64::from(u8::from(b)));
        match key {
            "front.progress" => Some(f64::from(self.progress)),
            "front.progress_max" => Some(1.0),
            _ => {
                if let Some(n) = slot("front.group_picked:") {
                    flag(self.group_top + n == self.group && !self.groups.is_empty())
                } else if let Some(n) = slot("front.server_picked:") {
                    flag(self.shown_servers().get(n) == Some(&self.server))
                } else {
                    None
                }
            }
        }
    }

    fn text(&self, key: &str) -> Option<String> {
        Some(match key {
            "front.account" => self.account.clone(),
            "front.password" => self.password.clone(),
            "front.name" => self.new_name.clone(),
            "front.message" => self.message.clone().unwrap_or_default(),
            "front.server" => self.server_name(),
            "front.group" => {
                (self.groups.get(self.group)).map_or(String::new(), |g| g.name.clone())
            }
            "front.look" => self.looks.get(self.look).cloned().unwrap_or_default(),
            "front.class" => {
                (self.classes.get(self.class)).map_or(String::new(), |c| c.name.clone())
            }
            "front.class_text" => {
                (self.classes.get(self.class)).map_or(String::new(), |c| c.description.clone())
            }
            "front.title" => self.title.clone(),
            _ => {
                return (self.slot_text(key)).or_else(|| self.number(key).map(|n| format!("{n}")));
            }
        })
    }

    fn image(&self, key: &str) -> Option<egui::TextureId> {
        if key == "front.class" {
            return self.class_preview;
        }
        let look = match key.strip_prefix("front.look")? {
            "" => self.look,
            n => *self
                .offered_looks()
                .get(n.strip_prefix(':')?.parse::<usize>().ok()?)?,
        };
        self.previews.get(&look).copied()
    }

    fn rows(&self, key: &str) -> (Vec<String>, Option<usize>) {
        match key {
            "front.servers" => {
                let shown = self.shown_servers();
                let names = shown.iter().map(|&i| self.servers[i].name.clone());
                (
                    names.collect(),
                    shown.iter().position(|&i| i == self.server),
                )
            }
            "front.looks" => {
                let offered = self.offered_looks();
                let names = (offered.iter()).filter_map(|&l| self.looks.get(l).cloned());
                (
                    names.collect(),
                    offered.iter().position(|&l| l == self.look),
                )
            }
            "front.classes" => (
                self.classes.iter().map(|c| c.name.clone()).collect(),
                Some(self.class),
            ),
            "front.characters" => (
                (self.characters.iter())
                    .map(|c| format!("{}  Lv {}", c.name, c.level))
                    .collect(),
                Some(self.character),
            ),
            _ => (Vec::new(), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, group: usize, icon: &str) -> ServerRow {
        ServerRow {
            name: name.into(),
            group,
            icon: icon.into(),
        }
    }

    #[test]
    fn login_slots_follow_the_picked_group() {
        let groups = (1..=5)
            .map(|g| GroupRow {
                name: format!("G{g}"),
                icon: format!("GroupPic{g}"),
            })
            .collect();
        let servers = vec![row("A", 0, "s1"), row("B", 1, "s2"), row("C", 1, "s3")];
        let mut front = FrontEnd::new(String::new(), servers, groups, Vec::new(), None);
        let mut actions = Vec::new();
        assert_eq!(front.text("front.server_name:0").as_deref(), Some("A"));
        front.act("group:1", &mut actions);
        assert_eq!(front.server, 1, "a group's first server is picked");
        assert_eq!(front.text("front.server_icon:1").as_deref(), Some("s3"));
        front.act("server:1", &mut actions);
        assert_eq!(front.text("front.server").as_deref(), Some("C"));
        assert_eq!(front.number("front.server_picked:1"), Some(1.0));
        front.act("group_down", &mut actions);
        front.act("group_down", &mut actions);
        assert_eq!(front.group_top, 1, "scrolls no further than the last four");
        assert_eq!(front.text("front.group:0").as_deref(), Some("GroupPic2"));
        assert_eq!(front.number("front.group_picked:0"), Some(1.0));
    }

    #[test]
    fn creation_starts_at_the_classes_and_offers_their_looks() {
        let looks = ["Small male", "Large male", "Small female", "Large female"];
        let mut front = FrontEnd::new(
            String::new(),
            Vec::new(),
            Vec::new(),
            looks.map(String::from).to_vec(),
            None,
        );
        front.classes = [("Trojan", vec![0, 1]), ("Trojan", vec![2, 3])]
            .map(|(name, looks)| ClassRow {
                name: name.into(),
                description: String::new(),
                looks,
            })
            .to_vec();
        front.got_characters(Vec::new());
        assert_eq!(front.screen, FrontScreen::Classes);
        let mut actions = Vec::new();
        front.act("class:1", &mut actions);
        front.act("choose", &mut actions);
        assert_eq!(front.screen, FrontScreen::Create);
        assert_eq!(front.look, 2, "the class's first look");
        front.act("next_look", &mut actions);
        front.act("next_look", &mut actions);
        assert_eq!(front.look, 2, "looks cycle within the class");
        front.act("look:1", &mut actions);
        assert_eq!(front.text("front.look").as_deref(), Some("Large female"));
        front.new_name = "Lina".into();
        front.act("create", &mut actions);
        assert_eq!(
            actions.last(),
            Some(&FrontAction::Create {
                name: "Lina".into(),
                look: 3,
                class: Some(1),
            })
        );
        front.screen = FrontScreen::Create;
        front.act("back", &mut actions);
        assert_eq!(front.screen, FrontScreen::Classes);
    }
}
