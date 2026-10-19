//! Forms over the game's tables: under a row of the monsters, items, skills or levels table,
//! the row Inspector shows what the game makes of it (from the saved tables) and how it ties
//! to the rest of the game — a monster's look (previewed), drops and spawn areas on every
//! map; an item's icon, the NPCs selling it and the monsters dropping it; a skill's effect;
//! a level's stats. Every link jumps to the row or the map.

use crate::app::Studio;
use crate::icons;
use crate::maps::MapRef;
use crate::selection::Selection;
use crate::shell::Panel;
use crate::theme;
use c3_game::Rules;
use c3_play::TableRef;
use eframe::egui::{self, RichText};
use std::collections::HashMap;

/// Where a form's link leads.
enum Jump {
    Row {
        table: String,
        key: i64,
    },
    Npc {
        map: String,
        id: u32,
        cell: (i32, i32),
    },
    Spawn {
        map: String,
        id: u32,
        cell: (i32, i32),
    },
}

/// What the forms read: the game's rules from the saved tables, and item icons.
#[derive(Default)]
pub(crate) struct FormCache {
    rules: Option<Box<dyn Rules>>,
    icons: HashMap<u32, Option<egui::TextureHandle>>,
}

impl FormCache {
    /// Reads the rules again (a table was saved).
    pub fn forget(&mut self) {
        self.rules = None;
    }
}

impl Studio {
    /// The form under row `key` of table `table`, when it is one of the game's tables.
    pub(crate) fn row_form(&mut self, ui: &mut egui::Ui, table: &str, key: i64) {
        let Some(game) = self.game.clone() else {
            return;
        };
        let tables = game.data_tables();
        let roles = [
            (&tables.monsters, Form::Monster),
            (&tables.items, Form::Item),
            (&tables.skills, Form::Skill),
            (&tables.levels, Form::Level),
        ];
        let Some((table_ref, form)) = roles.into_iter().find(|(t, _)| t.table == table) else {
            return;
        };
        let Some(id) = self.rules_id(table_ref, key) else {
            return;
        };
        self.ensure_rules();
        self.ensure_world_index();
        // Items show which monsters drop them: the monster rows, read first.
        self.referred(&tables.monsters.table);
        ui.add_space(8.0);
        let jump = match form {
            Form::Monster => self.monster_form(ui, id),
            Form::Item => self.item_form(ui, id, &tables.monsters.table),
            Form::Skill => self.skill_form(ui, id),
            Form::Level => self.level_form(ui, id),
        };
        if let Some(jump) = jump {
            self.follow(jump);
        }
    }

    fn rules_id(&self, table_ref: &TableRef, key: i64) -> Option<u32> {
        let open = self.data.open.as_ref()?;
        table_ref.id_of(open.doc.table(), key)
    }

    fn ensure_rules(&mut self) {
        if self.forms.rules.is_some() {
            return;
        }
        let (Some(game), Some(project)) = (&self.game, &self.project) else {
            return;
        };
        let (rules, _) = game.rules().rules(Some(&project.tables_dir()));
        self.forms.rules = Some(rules);
    }

    fn monster_form(&mut self, ui: &mut egui::Ui, id: u32) -> Option<Jump> {
        let items = self.game.as_ref()?.data_tables().items.table;
        let look = (self.forms.rules.as_deref())
            .map(|rules| self.game.as_ref().map(|g| g.looks().monster(id, rules)))?;
        let rules = self.forms.rules.as_deref()?;
        let Some(m) = rules.monster(id) else {
            note(
                ui,
                "The game doesn't read this row as a monster (yet: save the table)",
            );
            return None;
        };
        let mut jump = None;
        let mut preview = false;
        theme::section(ui, &format!("{} {}", icons::SWORD, m.name), |ui| {
            stats_grid(
                ui,
                "monster stats",
                &[
                    ("Level", m.level.to_string()),
                    ("Life", m.stats.max_life.to_string()),
                    (
                        "Attack",
                        format!("{}–{}", m.stats.attack_min, m.stats.attack_max),
                    ),
                    ("Defence", m.stats.defence.to_string()),
                    ("Look", m.look.to_string()),
                ],
            );
            preview = ui
                .button(theme::icon_text(icons::PERSON_SIMPLE, "Preview its look"))
                .on_hover_text("Shows it in the Preview panel")
                .clicked();
        });
        let loot = rules.loot(id);
        theme::section(ui, "Drops", |ui| {
            if loot.money > 0 {
                ui.label(format!(
                    "{} up to {} ({}%)",
                    icons::COINS,
                    loot.money,
                    loot.money_percent
                ));
            }
            for &(item, percent) in &loot.items {
                let name = rules
                    .item(item)
                    .map_or_else(|| format!("item {item}"), |t| t.name.clone());
                if link(ui, &format!("{name} · {percent}%")) {
                    jump = Some(Jump::Row {
                        table: items.clone(),
                        key: i64::from(item),
                    });
                }
            }
            if loot.money == 0 && loot.items.is_empty() {
                note(ui, "Drops nothing");
            }
        });
        theme::section(ui, "Spawns", |ui| {
            let spawns: Vec<_> = (self.world.spawns.iter())
                .filter(|(_, s)| s.monster == id)
                .collect();
            if spawns.is_empty() {
                note(ui, "Not placed on any map (drag the row onto a map)");
            }
            for (card, s) in spawns {
                let (key, name) = self.world.card(*card);
                let label = format!("{name} · area {} · {} at once", s.id, s.max_alive);
                if link(ui, &label) {
                    jump = Some(Jump::Spawn {
                        map: key.to_owned(),
                        id: s.id,
                        cell: s.center(),
                    });
                }
            }
        });
        if preview {
            self.preview_look = look;
            self.show_panel(Panel::Preview);
            self.notify("Showing its look in the Preview panel".into());
        }
        jump
    }

    fn item_form(&mut self, ui: &mut egui::Ui, id: u32, monsters: &str) -> Option<Jump> {
        let icon = self.item_icon(ui.ctx(), id);
        let rules = self.forms.rules.as_deref()?;
        let Some(item) = rules.item(id) else {
            note(
                ui,
                "The game doesn't read this row as an item (yet: save the table)",
            );
            return None;
        };
        let mut jump = None;
        let mut try_on = false;
        let worn = item.slot.is_some();
        theme::section(ui, &format!("{} {}", icons::PACKAGE, item.name), |ui| {
            ui.horizontal(|ui| {
                if let Some(icon) = &icon {
                    ui.add(egui::Image::new(icon).fit_to_exact_size(egui::vec2(48.0, 48.0)));
                }
                stats_grid(
                    ui,
                    "item stats",
                    &[
                        ("Price", format!("{} {}", item.price, item.currency)),
                        (
                            "Worn",
                            item.slot.map_or("no".into(), |s| format!("slot {s}")),
                        ),
                        ("Stack", item.stack.to_string()),
                    ],
                );
            });
            try_on = worn
                && ui
                    .button(theme::icon_text(icons::PERSON_SIMPLE, "Try it on"))
                    .on_hover_text("Shows the default hero wearing it in the Preview panel")
                    .clicked();
        });
        theme::section(ui, "Sold by", |ui| {
            let sellers: Vec<_> = (self.world.npcs.iter())
                .filter(|(_, n)| rules.shop(n.id).contains(&id))
                .collect();
            if sellers.is_empty() {
                note(ui, "No NPC sells it");
            }
            for (card, n) in sellers {
                let (key, map) = self.world.card(*card);
                if link(ui, &format!("{} · {map}", n.name)) {
                    jump = Some(Jump::Npc {
                        map: key.to_owned(),
                        id: n.id,
                        cell: n.cell,
                    });
                }
            }
        });
        if try_on && let Some(game) = &self.game {
            let mut look = game.looks().default_player();
            look.worn.push(id);
            self.preview_look = Some(look);
            self.show_panel(Panel::Preview);
            return None;
        }
        let droppers: Vec<(i64, String)> = (self.data.referred_rows(monsters))
            .into_iter()
            .filter(|(k, _)| {
                u32::try_from(*k).is_ok_and(|k| rules.loot(k).items.iter().any(|(i, _)| *i == id))
            })
            .collect();
        theme::section(ui, "Dropped by", |ui| {
            if droppers.is_empty() {
                note(ui, "No monster drops it");
            }
            for (key, label) in &droppers {
                if link(ui, label) {
                    jump = Some(Jump::Row {
                        table: monsters.to_owned(),
                        key: *key,
                    });
                }
            }
        });
        jump
    }

    fn skill_form(&mut self, ui: &mut egui::Ui, id: u32) -> Option<Jump> {
        let rules = self.forms.rules.as_deref()?;
        let Some(skill) = rules.skill(id) else {
            note(
                ui,
                "The game doesn't read this row as a skill (yet: save the table)",
            );
            return None;
        };
        let effect = skill.effect.clone();
        let mut play = false;
        theme::section(ui, &format!("{} {}", icons::SPARKLE, skill.name), |ui| {
            stats_grid(
                ui,
                "skill stats",
                &[
                    ("Mana", skill.mana.to_string()),
                    ("Range", format!("{} cells", skill.range)),
                    ("Power", skill.power.to_string()),
                    ("Cooldown", format!("{} ms", skill.cooldown_ms)),
                    (
                        "Effect",
                        if effect.is_empty() {
                            "none".into()
                        } else {
                            effect.clone()
                        },
                    ),
                ],
            );
            play = !effect.is_empty()
                && ui
                    .button(theme::icon_text(icons::PLAY, "Preview the effect"))
                    .clicked();
        });
        if play {
            self.preview_effect = Some(effect);
            self.show_panel(Panel::Preview);
        }
        None
    }

    fn level_form(&mut self, ui: &mut egui::Ui, level: u32) -> Option<Jump> {
        let rules = self.forms.rules.as_deref()?;
        let s = rules.player_stats(&c3_game::StatsFor::level(level));
        let exp = rules.level_exp(level);
        theme::section(ui, &format!("{} Level {level}", icons::USER), |ui| {
            stats_grid(
                ui,
                "level stats",
                &[
                    ("Life", s.max_life.to_string()),
                    ("Mana", s.max_mana.to_string()),
                    ("Attack", format!("{}–{}", s.attack_min, s.attack_max)),
                    ("Defence", s.defence.to_string()),
                    ("To next", exp.map_or("top level".into(), |e| e.to_string())),
                ],
            );
        });
        None
    }

    /// Item `kind`'s icon as a texture, made once.
    fn item_icon(&mut self, ctx: &egui::Context, kind: u32) -> Option<egui::TextureHandle> {
        if let Some(icon) = self.forms.icons.get(&kind) {
            return icon.clone();
        }
        let icon = self
            .game
            .as_ref()
            .and_then(|g| g.item_icon(kind))
            .map(|img| {
                let size = [img.width as usize, img.height as usize];
                let pixels = egui::ColorImage::from_rgba_unmultiplied(size, &img.rgba);
                ctx.load_texture(format!("item icon {kind}"), pixels, Default::default())
            });
        self.forms.icons.insert(kind, icon.clone());
        icon
    }

    fn follow(&mut self, jump: Jump) {
        match jump {
            Jump::Row { table, key } => {
                self.open_table(&table);
                if self.data.open.as_ref().is_some_and(|t| t.name == table) {
                    self.data.selected = Some(key);
                }
            }
            Jump::Npc { map, id, cell } => self.show_on_map(&map, Selection::Npc(id), cell),
            Jump::Spawn { map, id, cell } => self.show_on_map(&map, Selection::Spawn(id), cell),
        }
    }

    /// Opens map `key` with `what` (on `cell`) selected and in view.
    pub(crate) fn show_on_map(&mut self, key: &str, what: Selection, cell: (i32, i32)) {
        let map = MapRef::parse(key);
        self.open_map(&map);
        self.show_panel(Panel::Scene);
        if let Some(doc) = self.doc.as_ref().filter(|d| d.map == map) {
            self.fit_pending = false;
            self.cam.center = doc.cell_to_world(cell);
            self.cam.zoom = self.cam.zoom.max(0.75);
            self.editor.select(Some(what));
        }
    }
}

#[derive(Clone, Copy)]
enum Form {
    Monster,
    Item,
    Skill,
    Level,
}

/// A clickable line; true when clicked.
fn link(ui: &mut egui::Ui, text: &str) -> bool {
    ui.add(egui::Button::new(RichText::new(text).color(theme::ACCENT)).frame(false))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(theme::TEXT_DIM).small());
}

fn stats_grid(ui: &mut egui::Ui, id: &str, rows: &[(&str, String)]) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([12.0, 3.0])
        .show(ui, |ui| {
            for (label, value) in rows {
                ui.label(RichText::new(*label).color(theme::TEXT_DIM));
                ui.label(value);
                ui.end_row();
            }
        });
}
