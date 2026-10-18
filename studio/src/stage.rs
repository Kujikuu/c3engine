//! The Preview panel: a small stage that shows characters, actions and effects exactly as the
//! project's game draws them. Pick a body, hair and worn items, an action and a facing, then
//! play effects on the character or the ground; pause, step and slow down to study them frame
//! by frame.

use crate::app::Studio;
use crate::gpu::GpuView;
use crate::icons;
use crate::theme;
use c3_play::actor::Actor;
use c3_play::world::World;
use c3_play::{Act, GameModule, Look};
use c3_render::camera::Camera2d;
use c3_scene::{CellPos, Grid, IsoProjection, Scene};
use eframe::egui::{self, Color32, DragValue, Pos2, Rect, RichText, Sense, Stroke};
use eframe::egui_wgpu;
use glam::Vec2;
use std::rc::Rc;

/// Cells along each side of the stage floor.
const STAGE_CELLS: u32 = 21;
/// Game time one Step runs.
const STEP_SECONDS: f32 = 1.0 / 30.0;
/// Longest frame the stage advances by.
const MAX_FRAME_SECONDS: f32 = 0.1;
/// Width of the controls column.
const CONTROLS_WIDTH: f32 = 250.0;

/// Where a previewed effect plays.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EffectTarget {
    Hero,
    Ground,
}

pub struct Stage {
    world: World,
    camera: Camera2d,
    gpu: GpuView,
    pub look: Look,
    pub action: u32,
    /// The game's named actions, for the action list.
    actions: Vec<(u32, String)>,
    /// The action plays once, then the character stands.
    pub once: bool,
    pub direction: u8,
    pub effect: String,
    pub target: EffectTarget,
    pub looping: bool,
    pub paused: bool,
    pub speed: f32,
    step: bool,
    /// Why the character couldn't be built, if it couldn't.
    problem: Option<String>,
}

impl Stage {
    /// A stage of `game`'s roles, tilted by `tilt_deg`.
    pub fn new(
        game: Rc<dyn GameModule>,
        render_state: &egui_wgpu::RenderState,
        tilt_deg: f32,
    ) -> Self {
        let side = STAGE_CELLS;
        let scene = Scene {
            grid: Grid::new(side, side),
            projection: IsoProjection::for_grid(64.0, 32.0, side),
            ground: None,
            portals: Vec::new(),
            entities: Vec::new(),
            layers: Vec::new(),
            npcs: Vec::new(),
            spawns: Vec::new(),
            regions: Vec::new(),
            meta: Default::default(),
        };
        let center = Vec2::from(scene.cell_to_world(Self::center()));
        let looks = game.looks();
        let (look, action, actions) = (
            looks.default_player(),
            looks.action(Act::Stand),
            looks.action_names(),
        );
        let mut stage = Self {
            world: World::new(game, "Preview".into(), scene, tilt_deg),
            camera: Camera2d { center, zoom: 1.5 },
            gpu: GpuView::new(render_state),
            look,
            action,
            actions,
            once: false,
            direction: 1,
            effect: String::new(),
            target: EffectTarget::Hero,
            looping: true,
            paused: false,
            speed: 1.0,
            step: false,
            problem: None,
        };
        stage.rebuild();
        stage
    }

    fn center() -> CellPos {
        let mid = STAGE_CELLS as i32 / 2;
        (mid, mid)
    }

    /// Rebuilds the character from the look and equipment, then starts the action.
    pub fn rebuild(&mut self) {
        for mut old in self.world.actors.drain(..) {
            old.release_textures(&mut self.gpu.renderer);
        }
        match Actor::from_look(
            &*self.world.game,
            &self.look,
            self.action,
            Self::center(),
            self.direction,
        ) {
            Ok(hero) => {
                self.world.actors.push(hero);
                self.problem = None;
                self.play_action();
            }
            Err(e) => self.problem = Some(e),
        }
    }

    /// Starts the chosen action from its first frame.
    pub fn play_action(&mut self) {
        self.world.hero_action(self.action, self.once);
    }

    pub fn turn(&mut self, steps: i8) {
        self.direction = (self.direction as i8 + steps).rem_euclid(8) as u8;
        if let Some(hero) = self.world.hero_mut() {
            hero.direction = self.direction;
        }
    }

    /// Plays the chosen effect on the character or the ground.
    pub fn play_effect(&mut self) -> Result<(), String> {
        let name = self.effect.trim().to_owned();
        if name.is_empty() {
            return Err("choose an effect first".into());
        }
        match self.target {
            EffectTarget::Hero => self.world.hero_effect(&name, self.looping),
            EffectTarget::Ground => self.world.place_effect(&name, Self::center(), self.looping),
        }
    }

    /// Reads the game's art again (the project's assets changed on disk) and replays the
    /// chosen effect.
    pub fn reload_assets(&mut self) -> Result<(), String> {
        self.world.reload_assets(&mut self.gpu.renderer);
        if self.effect.trim().is_empty() {
            Ok(())
        } else {
            self.play_effect()
        }
    }

    /// Stops every effect.
    pub fn clear_effects(&mut self) {
        self.world.clear_effects();
    }

    pub fn step(&mut self) {
        self.paused = true;
        self.step = true;
    }

    /// Draws the stage with a floor grid.
    fn viewport(&mut self, ui: &mut egui::Ui) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let viewport = Vec2::new(rect.width(), rect.height());
        if resp.dragged() {
            let d = resp.drag_delta();
            self.camera.center -= Vec2::new(d.x, d.y) / self.camera.zoom;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.camera.zoom = (self.camera.zoom * (scroll * 0.002).exp()).clamp(0.3, 4.0);
            }
        }
        if resp.double_clicked() {
            self.camera.center = Vec2::from(self.world.scene.cell_to_world(Self::center()));
        }
        let dt = ui.input(|i| i.stable_dt).min(MAX_FRAME_SECONDS);
        if !self.paused {
            self.world.advance(dt * self.speed);
        } else if std::mem::take(&mut self.step) {
            self.world.advance(STEP_SECONDS);
        }
        self.world.tick();

        let ppp = ui.ctx().pixels_per_point();
        let size = [
            (rect.width() * ppp).round() as u32,
            (rect.height() * ppp).round() as u32,
        ];
        self.world
            .prepare(&mut self.gpu.renderer, &self.camera, viewport);
        let meshes = self.world.mesh_draws();
        let frame = self.world.frame(&meshes, self.camera.view_rect(viewport));
        let texture = self.gpu.render(size, &frame);
        let painter = ui.painter_at(rect);
        painter.image(
            texture,
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
        self.floor(&painter, rect, viewport);
        if let Some(problem) = &self.problem {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("Can't build this character: {problem}"),
                egui::FontId::proportional(13.0),
                theme::WARNING,
            );
        }
        ui.ctx().request_repaint();
    }

    /// A faint diamond grid where the character stands, for scale and facing.
    fn floor(&self, painter: &egui::Painter, rect: Rect, viewport: Vec2) {
        let scene = &self.world.scene;
        let to_screen = |c: (f32, f32)| {
            let world = Vec2::new(c.0, c.1);
            let p = (world - self.camera.center) * self.camera.zoom + viewport / 2.0;
            Pos2::new(rect.left() + p.x, rect.top() + p.y)
        };
        let edge = |x: f32, y: f32| {
            let o = scene.cell_to_world((0, 0));
            let ex = scene.cell_to_world((1, 0));
            let ey = scene.cell_to_world((0, 1));
            (
                o.0 + (ex.0 - o.0) * x + (ey.0 - o.0) * y,
                o.1 + (ex.1 - o.1) * x + (ey.1 - o.1) * y,
            )
        };
        let n = STAGE_CELLS as f32 - 0.5;
        let stroke = Stroke::new(1.0, Color32::from_white_alpha(14));
        for i in 0..=STAGE_CELLS {
            let t = i as f32 - 0.5;
            painter.line_segment([to_screen(edge(t, -0.5)), to_screen(edge(t, n))], stroke);
            painter.line_segment([to_screen(edge(-0.5, t)), to_screen(edge(n, t))], stroke);
        }
    }
}

/// The worn item types, each editable and removable, with a button to add one.
fn worn_items(ui: &mut egui::Ui, worn: &mut Vec<u32>) -> bool {
    let mut changed = false;
    let mut remove = None;
    for (i, item) in worn.iter_mut().enumerate() {
        ui.label(RichText::new(format!("Item {}", i + 1)).color(theme::TEXT_DIM));
        ui.horizontal(|ui| {
            changed |= ui
                .add(DragValue::new(item).speed(1.0))
                .on_hover_text("The game's item type")
                .changed();
            if theme::icon_toggle(ui, false, icons::X, "Take it off").clicked() {
                remove = Some(i);
            }
        });
        ui.end_row();
    }
    if let Some(i) = remove {
        worn.remove(i);
        changed = true;
    }
    ui.label("");
    if ui
        .button(theme::icon_text(icons::PLUS, "Wear an item"))
        .clicked()
    {
        worn.push(0);
    }
    ui.end_row();
    changed
}

/// An item id field with a label.
fn item_field(ui: &mut egui::Ui, label: &str, value: &mut u32, hint: &str) -> bool {
    ui.label(RichText::new(label).color(theme::TEXT_DIM));
    let changed = ui
        .add(DragValue::new(value).speed(1.0))
        .on_hover_text(hint)
        .changed();
    ui.end_row();
    changed
}

impl Studio {
    pub(crate) fn preview_panel(&mut self, ui: &mut egui::Ui) {
        let (Some(game), Some(state)) = (&self.game, &self.render_state) else {
            theme::empty_state(
                ui,
                icons::PERSON_SIMPLE,
                "Nothing to preview",
                "Open a project to preview its characters and effects.",
            );
            return;
        };
        let tilt = self.tilt_deg();
        if self.stage.is_none() {
            self.stage = Some(Stage::new(Rc::clone(game), state, tilt));
        }
        // A project effect or animation set opens in the editor beside the stage.
        if self.effect_editor.is_some() || self.character_editor.is_some() {
            egui::Panel::right("asset editor")
                .resizable(true)
                .default_size(300.0)
                .size_range(240.0..=480.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            if self.character_editor.is_some() {
                                self.character_editor_ui(ui)
                            } else {
                                self.effect_editor_ui(ui)
                            }
                        });
                });
        }
        let Some(stage) = &mut self.stage else { return };
        let mut play_effect = false;
        let mut pick_effect = None;
        egui::Panel::left("stage controls")
            .resizable(true)
            .default_size(CONTROLS_WIDTH)
            .size_range(200.0..=400.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        stage_controls(ui, stage, &mut play_effect);
                        theme::section(ui, "Effect list", |ui| {
                            pick_effect =
                                effect_names(ui, &self.effect_names, &mut self.effect_search);
                        });
                    });
            });
        egui::CentralPanel::default().show(ui, |ui| stage.viewport(ui));
        if let Some(name) = self.preview_effect.take() {
            pick_effect = Some(name);
        }
        if let Some(look) = self.preview_look.take() {
            stage.look = look;
            stage.rebuild();
        }
        if let Some(name) = pick_effect {
            stage.effect = name.clone();
            play_effect = true;
            self.character_editor = None;
            self.open_effect(&name);
        }
        let Some(stage) = &mut self.stage else { return };
        if play_effect && let Err(e) = stage.play_effect() {
            self.error = Some(format!("Can't play the effect: {e}"));
        }
    }
}

/// The searchable effect list (the Studio's effect names); returns the one clicked.
fn effect_names(ui: &mut egui::Ui, names: &[String], search: &mut String) -> Option<String> {
    ui.add(
        egui::TextEdit::singleline(search)
            .hint_text(theme::icon_text(icons::MAGNIFYING_GLASS, "Search effects"))
            .desired_width(f32::INFINITY),
    );
    let needle = search.to_lowercase();
    let mut chosen = None;
    egui::ScrollArea::vertical()
        .id_salt("stage effects")
        .max_height(260.0)
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                for name in names
                    .iter()
                    .filter(|n| n.to_lowercase().contains(&needle))
                    .take(300)
                {
                    if ui
                        .add(egui::Button::new(name).frame(false))
                        .on_hover_text("Click to play it")
                        .clicked()
                    {
                        chosen = Some(name.clone());
                    }
                }
            });
        });
    chosen
}

fn stage_controls(ui: &mut egui::Ui, stage: &mut Stage, play_effect: &mut bool) {
    theme::section(ui, "Character", |ui| {
        let mut rebuild = false;
        egui::Grid::new("stage look")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                let look = &mut stage.look;
                rebuild |= item_field(ui, "Body", &mut look.body, "The game's body id");
                rebuild |= item_field(ui, "Hair", &mut look.hair, "The game's hair id");
                rebuild |= worn_items(ui, &mut look.worn);
            });
        if ui
            .button(theme::icon_text(
                icons::ARROW_COUNTER_CLOCKWISE,
                "Default look",
            ))
            .clicked()
        {
            stage.look = stage.world.game.looks().default_player();
            rebuild = true;
        }
        if rebuild {
            stage.rebuild();
        }
    });
    theme::section(ui, "Action", |ui| {
        let current = (stage.actions.iter())
            .find(|(id, _)| *id == stage.action)
            .map_or("Other", |(_, name)| name.as_str());
        let mut chosen = None;
        egui::ComboBox::from_id_salt("stage action")
            .selected_text(format!("{current}  ({})", stage.action))
            .width(CONTROLS_WIDTH - 30.0)
            .show_ui(ui, |ui| {
                for (id, name) in &stage.actions {
                    let id = *id;
                    if ui
                        .selectable_label(stage.action == id, format!("{name}  ({id})"))
                        .clicked()
                    {
                        chosen = Some(id);
                    }
                }
            });
        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut stage.once, "Once")
                .on_hover_text("Play the action once, then stand")
                .changed()
            {
                chosen = Some(stage.action);
            }
            if ui
                .button(theme::icon_text(icons::ARROW_CLOCKWISE, "Restart"))
                .clicked()
            {
                chosen = Some(stage.action);
            }
        });
        if let Some(id) = chosen {
            stage.action = id;
            stage.play_action();
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Facing").color(theme::TEXT_DIM));
            if theme::icon_toggle(ui, false, icons::ARROW_COUNTER_CLOCKWISE, "Turn left").clicked()
            {
                stage.turn(-1);
            }
            ui.label(RichText::new(stage.direction.to_string()).monospace());
            if theme::icon_toggle(ui, false, icons::ARROW_CLOCKWISE, "Turn right").clicked() {
                stage.turn(1);
            }
        });
    });
    theme::section(ui, "Effect", |ui| {
        ui.add(
            egui::TextEdit::singleline(&mut stage.effect)
                .hint_text("Effect name, or pick below")
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            ui.radio_value(&mut stage.target, EffectTarget::Hero, "On character");
            ui.radio_value(&mut stage.target, EffectTarget::Ground, "On ground");
        });
        ui.checkbox(&mut stage.looping, "Loop forever");
        ui.horizontal(|ui| {
            *play_effect |= ui
                .button(theme::icon_text(icons::SPARKLE, "Play"))
                .clicked();
            if ui
                .button(theme::icon_text(icons::BROOM, "Clear"))
                .on_hover_text("Stop every effect")
                .clicked()
            {
                stage.clear_effects();
            }
        });
    });
    theme::section(ui, "Playback", |ui| {
        ui.horizontal(|ui| {
            let (icon, hint) = match stage.paused {
                true => (icons::PLAY, "Play"),
                false => (icons::PAUSE, "Pause"),
            };
            if theme::icon_toggle(ui, stage.paused, icon, hint).clicked() {
                stage.paused ^= true;
            }
            if theme::icon_toggle(ui, false, icons::SKIP_FORWARD, "Step one frame").clicked() {
                stage.step();
            }
        });
        ui.add(
            egui::Slider::new(&mut stage.speed, 0.1..=2.0)
                .text("speed")
                .suffix("×"),
        );
        ui.label(
            RichText::new("Drag to pan · Scroll zooms · Double-click recentres")
                .color(theme::TEXT_DIM)
                .small(),
        );
    });
}
