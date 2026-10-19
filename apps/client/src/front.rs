//! The front end before the world (see `c3_ui::front`): logs in to the server the player
//! picks, lists or makes their characters, and waits until the server lets the hero in.

use c3_core::ProjectSettings;
use c3_core::settings::{CreationClass, CreationLook, ServerEntry};
use c3_game::{PlayerLook, WorldUpdate};
use c3_online::{CharacterSummary, Connection, Enter, RemoteEvent, Welcome};
use c3_play::GameModule;
use c3_play::preview::LookPreview;
use c3_play::ui_textures::GameTextures;
use c3_render::Renderer;
use c3_ui::front::{
    CharacterRow, ClassRow, FrontAction, FrontEnd, FrontScreen, GroupRow, ServerRow,
};
use std::rc::Rc;

/// Pixels each creation look is drawn at.
const PREVIEW_SIZE: [u32; 2] = [256, 320];

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
    classes: Vec<CreationClass>,
    conn: Option<Connection>,
    welcome: Option<Welcome>,
    updates: Vec<WorldUpdate>,
    textures: GameTextures,
    /// The creation looks drawn, each with its texture; made when creation first shows.
    previews: Vec<(LookPreview, Option<egui::TextureId>)>,
    /// The picked class's figure: which class, the figure and its texture.
    class_preview: Option<(usize, LookPreview, Option<egui::TextureId>)>,
    /// Draws the previews (its own: offscreen targets need the engine's color format).
    preview_renderer: Option<Renderer>,
    game: Rc<dyn GameModule>,
    tilt_deg: f32,
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
            // Offered first, in the first server's group and with its picture.
            let like = servers.first().cloned().unwrap_or_default();
            servers.insert(
                0,
                ServerEntry {
                    name: address.clone(),
                    address,
                    ..like
                },
            );
        }
        let groups = &settings.network.groups;
        let rows = (servers.iter())
            .map(|s| ServerRow {
                name: s.name.clone(),
                group: groups.iter().position(|g| g.name == s.group).unwrap_or(0),
                icon: s.icon.clone(),
            })
            .collect();
        let group_rows = (groups.iter())
            .map(|g| GroupRow {
                name: g.name.clone(),
                icon: g.icon.clone(),
            })
            .collect();
        let looks = settings.characters.looks.clone();
        let gui = c3_play::session::game_gui(&*game, &settings.ui);
        let mut ui = FrontEnd::new(
            title,
            rows,
            group_rows,
            looks.iter().map(|l| l.name.clone()).collect(),
            gui,
        );
        if let Some((account, password)) = account {
            ui.account = account;
            ui.password = password;
        }
        ui.enter_alone = settings.characters.per_account == 1;
        let classes = settings.characters.classes.clone();
        ui.classes = (classes.iter())
            .map(|c| ClassRow {
                name: c.name.clone(),
                description: c3_play::settings::resolve_text(&*game, &c.description),
                looks: c.looks.clone(),
            })
            .collect();
        Self {
            ui,
            servers,
            looks,
            classes,
            conn: None,
            welcome: None,
            updates: Vec::new(),
            textures: GameTextures::new(Rc::clone(&game)),
            previews: Vec::new(),
            class_preview: None,
            preview_renderer: None,
            tilt_deg: c3_play::settings::tilt_deg(settings, &*game),
            game,
        }
    }

    /// Draws the creation looks while character creation shows (each a picture the
    /// screen's model controls and the engine's own screen show).
    pub fn draw_previews(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        ui: &mut crate::ui_layer::UiLayer,
        seconds: f32,
    ) {
        if self.ui.screen == FrontScreen::Classes {
            self.draw_class(device, queue, ui, seconds);
            return;
        }
        if self.ui.screen != FrontScreen::Create || self.looks.is_empty() {
            return;
        }
        let renderer = (self.preview_renderer)
            .get_or_insert_with(|| Renderer::new(device.clone(), queue.clone()));
        if self.previews.is_empty() {
            self.previews = (self.looks.iter())
                .map(|l| {
                    let look = c3_play::Look {
                        body: l.body,
                        hair: l.hair,
                        ..c3_play::Look::default()
                    };
                    (
                        LookPreview::new(Rc::clone(&self.game), &look, self.tilt_deg),
                        None,
                    )
                })
                .collect();
        }
        for (i, (preview, texture)) in self.previews.iter_mut().enumerate() {
            let target = preview.render(renderer, PREVIEW_SIZE, seconds);
            let id = ui.native_texture(device, &target.display_view, *texture);
            *texture = Some(id);
            self.ui.previews.insert(i, id);
        }
    }

    /// Draws the picked class's figure: its showcase figure, else its first look.
    fn draw_class(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        ui: &mut crate::ui_layer::UiLayer,
        seconds: f32,
    ) {
        let picked = self.ui.class;
        let Some(class) = self.classes.get(picked) else {
            return;
        };
        if self
            .class_preview
            .as_ref()
            .is_none_or(|(c, ..)| *c != picked)
        {
            let look = match class.showcase {
                Some(figure) => c3_play::Look {
                    kind: c3_play::LookKind::Showcase,
                    body: figure,
                    ..c3_play::Look::default()
                },
                None => {
                    let first = class.looks.first().and_then(|&l| self.looks.get(l));
                    first.map_or_else(c3_play::Look::default, |l| c3_play::Look {
                        body: l.body,
                        hair: l.hair,
                        ..c3_play::Look::default()
                    })
                }
            };
            let texture = self.class_preview.take().and_then(|(.., t)| t);
            let preview = LookPreview::new(Rc::clone(&self.game), &look, self.tilt_deg);
            self.class_preview = Some((picked, preview, texture));
        }
        let renderer = (self.preview_renderer)
            .get_or_insert_with(|| Renderer::new(device.clone(), queue.clone()));
        let (_, preview, texture) = self.class_preview.as_mut().expect("made above");
        let target = preview.render(renderer, PREVIEW_SIZE, seconds);
        let id = ui.native_texture(device, &target.display_view, *texture);
        *texture = Some(id);
        self.ui.class_preview = Some(id);
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
            FrontAction::Create { name, look, class } => {
                let profession =
                    (class.and_then(|c| self.classes.get(c))).map_or(0, |c| c.profession);
                let look = (self.looks.get(look)).map_or(PlayerLook::default(), |l| PlayerLook {
                    body: l.body,
                    hair: l.hair,
                });
                self.log_in(Enter::New {
                    name,
                    look,
                    profession,
                });
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
