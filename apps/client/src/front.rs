//! The front end before the world (see `c3_ui::front`): logs in to the server the player
//! picks, lists or makes their characters, and waits until the server lets the hero in.

use c3_core::ProjectSettings;
use c3_core::settings::{CreationLook, ServerEntry};
use c3_game::{PlayerLook, WorldUpdate};
use c3_online::{CharacterSummary, Connection, Enter, RemoteEvent, Welcome};
use c3_play::GameModule;
use c3_play::ui_textures::GameTextures;
use c3_ui::front::{CharacterRow, FrontAction, FrontEnd, FrontScreen};
use std::rc::Rc;

/// What the front end came to this frame.
pub enum FrontOutcome {
    Stay,
    Quit,
    /// The server let the hero in: play on this connection.
    Play(Connection, Welcome, Vec<WorldUpdate>),
}

pub struct FrontPhase {
    ui: FrontEnd,
    servers: Vec<ServerEntry>,
    looks: Vec<CreationLook>,
    conn: Option<Connection>,
    welcome: Option<Welcome>,
    updates: Vec<WorldUpdate>,
    textures: GameTextures,
}

impl FrontPhase {
    /// The login of `settings`' servers (`server`, when given, is offered first), with
    /// `account` and its password typed in already.
    pub fn new(
        game: Rc<dyn GameModule>,
        settings: &ProjectSettings,
        title: String,
        server: Option<String>,
        account: Option<(String, String)>,
    ) -> Self {
        let mut servers = settings.network.login_servers();
        if let Some(address) = server {
            servers.insert(
                0,
                ServerEntry {
                    name: address.clone(),
                    address,
                },
            );
        }
        let looks = settings.characters.looks.clone();
        let gui = c3_play::session::game_gui(&*game, &settings.ui);
        let mut ui = FrontEnd::new(
            title,
            servers.iter().map(|s| s.name.clone()).collect(),
            looks.iter().map(|l| l.name.clone()).collect(),
            gui,
        );
        if let Some((account, password)) = account {
            ui.account = account;
            ui.password = password;
        }
        ui.enter_alone = settings.characters.per_account == 1;
        Self {
            ui,
            servers,
            looks,
            conn: None,
            welcome: None,
            updates: Vec::new(),
            textures: GameTextures::new(game),
        }
    }

    /// Shows the front end over `rect` and follows the server; what came of it.
    pub fn update(&mut self, ui: &mut egui::Ui, rect: egui::Rect) -> FrontOutcome {
        let actions = self.ui.show(ui, rect, &mut self.textures);
        for action in actions {
            if let Some(outcome) = self.act(action) {
                return outcome;
            }
        }
        self.follow_server()
    }

    fn act(&mut self, action: FrontAction) -> Option<FrontOutcome> {
        match action {
            FrontAction::Quit => return Some(FrontOutcome::Quit),
            FrontAction::Login { server } => {
                let Some(entry) = self.servers.get(server) else {
                    self.ui.refused("No such server.".into());
                    return None;
                };
                match Connection::open(entry.address.as_str()) {
                    Ok(conn) => {
                        self.ui.progress = 0.2;
                        conn.ask_characters(&self.ui.account, &self.ui.password);
                        self.conn = Some(conn);
                    }
                    Err(e) => self.ui.refused(format!("Can't reach {}: {e}", entry.name)),
                }
            }
            FrontAction::Enter(name) => self.log_in(Enter::Existing(name)),
            FrontAction::Create { name, look } => {
                let look = (self.looks.get(look)).map_or(PlayerLook::default(), |l| PlayerLook {
                    body: l.body,
                    hair: l.hair,
                });
                self.log_in(Enter::New { name, look });
            }
        }
        None
    }

    fn log_in(&mut self, enter: Enter) {
        match &self.conn {
            Some(conn) => {
                self.ui.progress = 0.5;
                conn.login(&self.ui.account, &self.ui.password, enter);
            }
            None => self.ui.refused("Not connected.".into()),
        }
    }

    /// What the server said: the characters, a refusal, or the way in.
    fn follow_server(&mut self) -> FrontOutcome {
        let Some(conn) = &mut self.conn else {
            return FrontOutcome::Stay;
        };
        let mut enter = None;
        for event in conn.poll() {
            match event {
                RemoteEvent::Characters(list) => {
                    let rows = list.iter().map(|c| self.row(c)).collect();
                    if let Some(FrontAction::Enter(name)) = self.ui.got_characters(rows) {
                        enter = Some(name);
                    }
                }
                RemoteEvent::Refused(why) | RemoteEvent::Closed(Some(why)) => {
                    self.conn = None;
                    self.ui.refused(why);
                    return FrontOutcome::Stay;
                }
                RemoteEvent::Closed(None) => {
                    self.conn = None;
                    self.ui.refused("The server closed the connection.".into());
                    return FrontOutcome::Stay;
                }
                RemoteEvent::Welcome(w) => {
                    self.ui.progress = 0.8;
                    self.welcome = Some(w);
                }
                RemoteEvent::Update(u) => self.updates.push(u),
            }
        }
        if let Some(name) = enter {
            self.log_in(Enter::Existing(name));
        }
        // The first update (the hero and what is around) follows the welcome.
        if self.welcome.is_some() && !self.updates.is_empty() {
            self.ui.screen = FrontScreen::Loading;
            self.ui.progress = 1.0;
            if let (Some(conn), Some(welcome)) = (self.conn.take(), self.welcome.take()) {
                return FrontOutcome::Play(conn, welcome, std::mem::take(&mut self.updates));
            }
        }
        FrontOutcome::Stay
    }

    fn row(&self, c: &CharacterSummary) -> CharacterRow {
        CharacterRow {
            name: c.name.clone(),
            level: c.level,
            look: (self.looks.iter()).position(|l| l.body == c.look.body && l.hair == c.look.hair),
        }
    }

    /// The front end lost the game (it failed to load): back to the login with `why`.
    pub fn failed(&mut self, why: String) {
        self.ui.refused(why);
    }
}
