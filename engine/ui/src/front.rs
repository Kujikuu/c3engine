//! The client's front end, before the world: log in, pick a server, pick or make a
//! character, and wait while the world loads. A game whose UI (see [`crate::gui`]) has
//! dialogs for a screen (`screen=login`, `servers`, `characters`, `create`, `loading`) shows
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
            Self::Create => "create",
            Self::Loading => "loading",
        }
    }
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
    /// Make a character: its name and which of the creation looks.
    Create {
        name: String,
        look: usize,
    },
    Quit,
}

/// The front end's state: what was typed and picked, and what the host said.
pub struct FrontEnd {
    pub screen: FrontScreen,
    pub account: String,
    pub password: String,
    /// The servers to pick from (their names) and the one picked.
    pub servers: Vec<String>,
    pub server: usize,
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
    /// An account's one character is entered at once, with no list (one per account).
    pub enter_alone: bool,
    gui: Option<Gui>,
}

impl FrontEnd {
    pub fn new(title: String, servers: Vec<String>, looks: Vec<String>, gui: Option<Gui>) -> Self {
        Self {
            screen: FrontScreen::Login,
            account: String::new(),
            password: String::new(),
            servers,
            server: 0,
            characters: Vec::new(),
            character: 0,
            looks,
            look: 0,
            new_name: String::new(),
            message: None,
            progress: 0.0,
            title,
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
        self.screen = if self.characters.is_empty() {
            FrontScreen::Create
        } else {
            FrontScreen::Characters
        };
        None
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
                "front.servers" => self.server = index,
                "front.characters" => self.character = index,
                "front.looks" => self.look = index,
                _ => {}
            },
            GuiEvent::Action(action) => self.act(&action, actions),
        }
    }

    /// Carries out a named action (a button's `action=`).
    fn act(&mut self, action: &str, actions: &mut Vec<FrontAction>) {
        let looks = self.looks.len().max(1);
        match action {
            "login" => self.log_in(actions),
            "servers" => self.screen = FrontScreen::Servers,
            "enter" => {
                if let Some(c) = self.characters.get(self.character) {
                    self.screen = FrontScreen::Loading;
                    actions.push(FrontAction::Enter(c.name.clone()));
                }
            }
            "new" => self.screen = FrontScreen::Create,
            "create" => self.make(actions),
            "next_look" => self.look = (self.look + 1) % looks,
            "prev_look" => self.look = (self.look + looks - 1) % looks,
            "back" => {
                self.message = None;
                self.screen = match self.screen {
                    FrontScreen::Create if !self.characters.is_empty() => FrontScreen::Characters,
                    _ => FrontScreen::Login,
                }
            }
            "quit" => actions.push(FrontAction::Quit),
            other => {
                if let Some(look) = other.strip_prefix("look:").and_then(|n| n.parse().ok()) {
                    self.look = look;
                }
            }
        }
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
            let server = self.servers.get(self.server).cloned().unwrap_or_default();
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
        for (i, name) in self.servers.iter().enumerate() {
            let row = egui::Button::selectable(self.server == i, name)
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

    fn loading(&mut self, ui: &mut egui::Ui) {
        Self::heading(ui, icons::HOURGLASS, "Entering the world");
        ui.add(egui::ProgressBar::new(self.progress.clamp(0.0, 1.0)).show_percentage());
    }
}

/// What the screen's dialogs read.
impl GuiData for FrontEnd {
    fn number(&self, key: &str) -> Option<f64> {
        match key {
            "front.progress" => Some(f64::from(self.progress)),
            "front.progress_max" => Some(1.0),
            _ => None,
        }
    }

    fn text(&self, key: &str) -> Option<String> {
        Some(match key {
            "front.account" => self.account.clone(),
            "front.password" => self.password.clone(),
            "front.name" => self.new_name.clone(),
            "front.message" => self.message.clone().unwrap_or_default(),
            "front.server" => self.servers.get(self.server).cloned().unwrap_or_default(),
            "front.look" => self.looks.get(self.look).cloned().unwrap_or_default(),
            "front.title" => self.title.clone(),
            _ => return self.number(key).map(|n| format!("{n}")),
        })
    }

    fn rows(&self, key: &str) -> (Vec<String>, Option<usize>) {
        match key {
            "front.servers" => (self.servers.clone(), Some(self.server)),
            "front.looks" => (self.looks.clone(), Some(self.look)),
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
