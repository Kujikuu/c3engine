//! The bar at the bottom (level, life, mana, experience, money, bag and quest buttons) and
//! what is drawn over the world: drop labels, names and life of the monster fought or pointed at,
//! and damage numbers.

use crate::fonts;
use crate::icons;
use crate::style;
use crate::{GameView, UiAction, item_name};
use c3_core::input::Action;
use c3_game::{EntityId, Event, Player, SkillKind, SkillTarget, Task};
use egui::{Color32, Pos2, Rect};

/// Seconds a damage number rises before it is gone.
const FLOAT_SECONDS: f64 = 1.0;
const BAR_WIDTH: f32 = 500.0;
/// Width of the bar's right column (money, sound, buttons).
const RIGHT_COLUMN: f32 = 140.0;

/// The windows the bar's buttons open.
pub(crate) struct Open<'a> {
    pub bag: &'a mut bool,
    pub quests: &'a mut bool,
}

/// A number rising from an entity (damage, a miss).
struct Float {
    at: f64,
    entity: EntityId,
    text: String,
    color: Color32,
}

#[derive(Default)]
pub(crate) struct Overlay {
    floats: Vec<Float>,
}

impl Overlay {
    pub fn event(&mut self, event: &Event, hero: EntityId, now: f64) {
        if let Event::Healed { target, amount } = *event
            && amount > 0
        {
            self.floats.push(Float {
                at: now,
                entity: target,
                text: format!("+{amount}"),
                color: style::GOOD,
            });
        }
        if let Event::Attacked {
            attacker,
            target,
            damage,
        } = *event
        {
            let (text, color) = match damage {
                0 => ("Miss".to_owned(), style::DIM),
                d if target == hero => (format!("-{d}"), style::LIFE),
                d if attacker == hero => (d.to_string(), Color32::WHITE),
                d => (d.to_string(), style::TEXT),
            };
            self.floats.push(Float {
                at: now,
                entity: target,
                text,
                color,
            });
        }
    }

    pub fn show(&mut self, ui: &egui::Ui, view: &GameView) {
        let now = ui.input(|i| i.time);
        self.floats.retain(|f| now - f.at < FLOAT_SECONDS);
        let painter = ui.painter_at(view.rect);
        drop_labels(&painter, view);
        player_names(&painter, view);
        let fighting = view.player().and_then(|p| match p.task {
            Task::Attack(t) => Some(t),
            _ => None,
        });
        let plates = [fighting, view.hovered.filter(|h| Some(*h) != fighting)];
        for id in plates.into_iter().flatten() {
            let Some((name, life, max)) = fight_plate(view, id) else {
                continue;
            };
            if let Some(at) = (view.head)(id) {
                name_plate(&painter, at, &name, life, max);
            }
        }
        for f in &self.floats {
            let Some(at) = (view.head)(f.entity) else {
                continue;
            };
            let t = ((now - f.at) / FLOAT_SECONDS) as f32;
            painter.text(
                at + egui::vec2(0.0, -26.0 - 30.0 * t),
                egui::Align2::CENTER_BOTTOM,
                &f.text,
                fonts::font(18.0),
                f.color.gamma_multiply(1.0 - t * t),
            );
        }
    }
}

/// The name and life shown over `id` while fought or pointed at: a living monster, or on a
/// PK map another player.
fn fight_plate(view: &GameView, id: EntityId) -> Option<(String, u32, u32)> {
    if let Some(m) = view.sim.monster(id).filter(|m| m.is_alive()) {
        let rules = view.sim.rules();
        let name = rules.monster(m.kind).map_or("?", |t| t.name.as_str());
        return Some((format!("{name}  Lv {}", m.level), m.life, m.stats.max_life));
    }
    let p = view
        .sim
        .player(id)
        .filter(|p| p.is_alive() && view.sim.pk())?;
    (id != view.hero).then(|| {
        (
            format!("{}  Lv {}", p.name, p.level),
            p.life,
            p.stats.max_life,
        )
    })
}

/// A name with a life bar under it, centred above `at`.
fn name_plate(painter: &egui::Painter, at: Pos2, name: &str, life: u32, max: u32) {
    let galley = painter.layout_no_wrap(name.to_owned(), fonts::font(12.0), Color32::WHITE);
    let width = galley.size().x.max(70.0);
    let top = at - egui::vec2(width / 2.0, galley.size().y + 10.0);
    painter.rect_filled(
        Rect::from_min_size(
            top - egui::vec2(5.0, 3.0),
            egui::vec2(width + 10.0, galley.size().y + 14.0),
        ),
        3.0,
        style::backing(1.0),
    );
    painter.galley(
        top + egui::vec2((width - galley.size().x) / 2.0, 0.0),
        galley,
        Color32::WHITE,
    );
    meter(
        painter,
        Rect::from_min_size(at - egui::vec2(width / 2.0, 6.0), egui::vec2(width, 4.0)),
        life,
        max,
        style::LIFE,
    );
}

/// A bar filled to `value` of `max`.
fn meter(painter: &egui::Painter, rect: Rect, value: u32, max: u32, color: Color32) {
    painter.rect_filled(rect, 2.0, Color32::from_gray(36));
    let fill = if max == 0 {
        0.0
    } else {
        value as f32 / max as f32
    };
    let mut full = rect;
    full.set_width(rect.width() * fill.clamp(0.0, 1.0));
    painter.rect_filled(full, 2.0, color);
}

/// A meter with `label value/max` written on it.
fn labelled_meter(
    painter: &egui::Painter,
    rect: Rect,
    label: &str,
    value: u32,
    max: u32,
    color: Color32,
) {
    meter(painter, rect, value, max, color);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{label}  {value} / {max}"),
        fonts::font(10.5),
        Color32::WHITE,
    );
}

/// The line of what the controls do, top left of the game.
pub(crate) fn hint_line(ui: &egui::Ui, view: &GameView) {
    if view.hint.is_empty() {
        return;
    }
    let painter = ui.painter_at(view.rect);
    let galley = painter.layout(
        view.hint.to_owned(),
        fonts::font(12.0),
        style::TEXT,
        view.rect.width() - 40.0,
    );
    let pad = egui::vec2(8.0, 5.0);
    let at = view.rect.left_top() + egui::vec2(10.0, 10.0);
    painter.rect_filled(
        Rect::from_min_size(at, galley.size() + pad * 2.0),
        4.0,
        style::backing(1.0),
    );
    painter.galley(at + pad, galley, style::TEXT);
}

/// The bottom bar: level, life, mana, experience, money and the window buttons.
pub(crate) fn bar(
    ui: &mut egui::Ui,
    view: &GameView,
    p: &Player,
    open: &mut Open,
    actions: &mut Vec<UiAction>,
) {
    let rect = view.rect;
    let area = Rect::from_center_size(
        egui::pos2(rect.center().x, rect.bottom() - 34.0),
        egui::vec2(BAR_WIDTH, 52.0),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(area, 6.0, style::PANEL);
    painter.rect_stroke(
        area,
        6.0,
        egui::Stroke::new(1.0, style::EDGE),
        egui::StrokeKind::Inside,
    );
    // Level badge.
    let badge = Rect::from_min_size(area.min + egui::vec2(8.0, 8.0), egui::vec2(36.0, 36.0));
    painter.circle_filled(badge.center(), 18.0, Color32::from_gray(28));
    painter.circle_stroke(badge.center(), 18.0, egui::Stroke::new(1.5, style::GOLD));
    painter.text(
        badge.center() - egui::vec2(0.0, 5.0),
        egui::Align2::CENTER_CENTER,
        p.level.to_string(),
        fonts::font(15.0),
        style::GOLD,
    );
    painter.text(
        badge.center() + egui::vec2(0.0, 9.0),
        egui::Align2::CENTER_CENTER,
        "LV",
        fonts::font(8.0),
        style::DIM,
    );
    let left = badge.right() + 10.0;
    let column = area.right() - RIGHT_COLUMN;
    let right = column - 8.0;
    let row = |y: f32, h: f32| Rect::from_min_max(egui::pos2(left, y), egui::pos2(right, y + h));
    // Without mana (warriors) life takes both rows.
    let has_mana = p.stats.max_mana > 0;
    let life_height = if has_mana { 13.0 } else { 27.0 };
    labelled_meter(
        &painter,
        row(area.top() + 8.0, life_height),
        icons::HEART,
        p.life,
        p.stats.max_life,
        style::LIFE,
    );
    if has_mana {
        labelled_meter(
            &painter,
            row(area.top() + 24.0, 11.0),
            icons::DROP,
            p.mana,
            p.stats.max_mana,
            style::MANA,
        );
    }
    let need = view.sim.rules().level_exp(p.level);
    let exp = row(area.top() + 39.0, 5.0);
    match need {
        Some(need) => {
            meter(
                &painter,
                exp,
                p.exp.min(need) as u32,
                need.min(u32::MAX.into()) as u32,
                style::EXP,
            );
            let percent = p.exp as f64 * 100.0 / need.max(1) as f64;
            ui.allocate_rect(exp.expand(3.0), egui::Sense::hover())
                .on_hover_text(format!("Experience {} / {need} ({percent:.1}%)", p.exp));
        }
        None => meter(&painter, exp, 1, 1, style::GOLD),
    }
    // Money and the window buttons on the right.
    painter.text(
        egui::pos2(column, area.top() + 14.0),
        egui::Align2::LEFT_CENTER,
        format!("{}  {}", icons::COINS, p.money),
        fonts::font(13.0),
        style::MONEY,
    );
    let bag = Rect::from_min_size(
        egui::pos2(column, area.top() + 26.0),
        egui::vec2(96.0, 20.0),
    );
    let label = format!(
        "{}  Bag {}/{}",
        icons::BACKPACK,
        p.bag.len(),
        view.sim.bag_size()
    );
    let button = egui::Button::new(egui::RichText::new(label).size(12.0)).selected(*open.bag);
    if ui
        .put(bag, button)
        .on_hover_text("Bag and equipment (B)")
        .clicked()
    {
        *open.bag ^= true;
    }
    let quests = Rect::from_min_size(
        bag.right_top() + egui::vec2(6.0, 0.0),
        egui::vec2(30.0, 20.0),
    );
    let button =
        egui::Button::new(egui::RichText::new(icons::SCROLL).size(13.0)).selected(*open.quests);
    if ui.put(quests, button).on_hover_text("Quests (L)").clicked() {
        *open.quests ^= true;
    }
    // The sound toggle, when there is sound.
    if let Some(on) = view.sound {
        let at = Rect::from_min_size(
            egui::pos2(area.right() - 30.0, area.top() + 4.0),
            egui::vec2(22.0, 18.0),
        );
        let (icon, hint) = if on {
            (icons::SPEAKER_HIGH, "Sound on (click to mute)")
        } else {
            (icons::SPEAKER_X, "Sound muted (click to unmute)")
        };
        let button = egui::Button::new(egui::RichText::new(icon).size(13.0)).frame(false);
        if ui.put(at, button).on_hover_text(hint).clicked() {
            actions.push(UiAction::ToggleSound);
        }
    }
}

/// The statuses the hero bears, under the hint line: name and seconds left, good ones green,
/// bad ones red; the tooltip says what each does.
pub(crate) fn status_row(ui: &mut egui::Ui, view: &GameView, p: &Player) {
    let rules = view.sim.rules();
    let now = view.sim.now_ms();
    let mut at =
        view.rect.left_top() + egui::vec2(10.0, if view.hint.is_empty() { 10.0 } else { 40.0 });
    let painter = ui.painter_at(view.rect);
    for s in &p.statuses {
        let Some(t) = rules.status(s.id) else {
            continue;
        };
        let bad = t.stun
            || t.life_per_tick < 0
            || t.attack_percent + t.defence_percent + t.dodge_percent + t.speed_percent < 0;
        let color = if bad { style::BAD } else { style::GOOD };
        let left = s.until_ms.saturating_sub(now).div_ceil(1000);
        let text = format!("{}  {}  {left}s", icons::HOURGLASS, t.name);
        let galley = painter.layout_no_wrap(text, fonts::font(12.0), color);
        let rect = Rect::from_min_size(at, galley.size() + egui::vec2(12.0, 6.0));
        painter.rect_filled(rect, 4.0, style::backing(1.0));
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.0, color.gamma_multiply(0.6)),
            egui::StrokeKind::Inside,
        );
        painter.galley(at + egui::vec2(6.0, 3.0), galley, color);
        ui.allocate_rect(rect, egui::Sense::hover())
            .on_hover_text(status_tip(t));
        at.x = rect.right() + 6.0;
    }
}

/// What a status does, for its tooltip.
fn status_tip(t: &c3_game::StatusType) -> String {
    let mut lines = vec![t.name.clone()];
    for (what, percent) in [
        ("Attack", t.attack_percent),
        ("Defence", t.defence_percent),
        ("Dodge and accuracy", t.dodge_percent),
        ("Speed", t.speed_percent),
    ] {
        if percent != 0 {
            lines.push(format!("{what} {percent:+}%"));
        }
    }
    if t.life_per_tick != 0 {
        lines.push(format!(
            "{:+} life every {:.1} s",
            t.life_per_tick,
            t.tick_ms as f32 / 1000.0
        ));
    }
    if t.stun {
        lines.push("Can't act".to_owned());
    }
    lines.join("\n")
}

/// Size of a skill bar slot.
const SKILL_SLOT: f32 = 34.0;
/// Height the skill bar takes above the bottom bar.
pub(crate) const SKILL_ROW: f32 = SKILL_SLOT + 6.0;

/// The skills the hero knows, above the bottom bar: click or press the slot's key to cast;
/// a resting skill shows its cooldown, one short of mana is dimmed.
pub(crate) fn skill_bar(
    ui: &mut egui::Ui,
    view: &GameView,
    p: &Player,
    actions: &mut Vec<UiAction>,
) {
    let count = p.skills.len().min(Action::SKILLS.len());
    if count == 0 {
        return;
    }
    let rules = view.sim.rules();
    let now = view.sim.now_ms();
    let gap = 4.0;
    let width = count as f32 * (SKILL_SLOT + gap) - gap;
    let top_left = egui::pos2(
        view.rect.center().x - width / 2.0,
        view.rect.bottom() - 66.0 - SKILL_SLOT,
    );
    let painter = ui.painter_at(view.rect);
    for (slot, &id) in p.skills.iter().take(count).enumerate() {
        let Some(t) = rules.skill(id) else {
            continue;
        };
        let rect = Rect::from_min_size(
            top_left + egui::vec2(slot as f32 * (SKILL_SLOT + gap), 0.0),
            egui::vec2(SKILL_SLOT, SKILL_SLOT),
        );
        let resp = ui.allocate_rect(rect, egui::Sense::click());
        let short = p.mana < t.mana;
        let resting = p.cooldowns.get(&id).copied().filter(|&ready| ready > now);
        let edge = if resp.hovered() {
            style::GOLD
        } else {
            style::EDGE
        };
        painter.rect_filled(rect, 5.0, style::PANEL);
        painter.rect_stroke(
            rect,
            5.0,
            egui::Stroke::new(1.0, edge),
            egui::StrokeKind::Inside,
        );
        let (icon, color) = match t.kind {
            SkillKind::Heal => (icons::SPARKLE, style::GOOD),
            SkillKind::Damage => (icons::LIGHTNING, style::GOLD),
            SkillKind::Status => (icons::SHIELD, style::MANA),
        };
        let color = if short {
            color.gamma_multiply(0.35)
        } else {
            color
        };
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            egui::FontId::proportional(18.0),
            color,
        );
        if let Some(ready) = resting {
            let left = (ready - now) as f32 / t.cooldown_ms.max(1) as f32;
            let mut cover = rect;
            cover.set_top(rect.bottom() - rect.height() * left.clamp(0.0, 1.0));
            painter.rect_filled(cover, 5.0, Color32::from_black_alpha(150));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("{}", (ready - now).div_ceil(1000)),
                fonts::font(13.0),
                Color32::WHITE,
            );
        }
        let key = skill_key(view, slot);
        if !key.is_empty() {
            painter.text(
                rect.left_top() + egui::vec2(3.0, 1.0),
                egui::Align2::LEFT_TOP,
                &key,
                fonts::font(9.5),
                style::DIM,
            );
        }
        if resp.on_hover_text(skill_tip(t, &key)).clicked() {
            actions.push(UiAction::Cast(slot));
        }
    }
}

/// A skill's tooltip: its name, cost, reach and rest.
fn skill_tip(t: &c3_game::SkillType, key: &str) -> String {
    let aim = match t.target {
        SkillTarget::Enemy => "at the target",
        SkillTarget::Myself => "on yourself",
        SkillTarget::Ground => "where you point",
    };
    let mut tip = format!("{}\n{} mana · range {}", t.name, t.mana, t.range);
    if t.area > 0 {
        tip += &format!(" · area {}", t.area);
    }
    tip += &format!("\nCast {aim}");
    if t.cooldown_ms > 0 {
        tip += &format!(" · rests {:.1} s", t.cooldown_ms as f32 / 1000.0);
    }
    if !key.is_empty() {
        tip += &format!(" ({key})");
    }
    tip
}

/// The keys of skill bar slot `slot`, as the Controls page shows them.
fn skill_key(view: &GameView, slot: usize) -> String {
    let action = Action::SKILLS[slot];
    (view.controls.rows.iter())
        .find(|r| r.action == action)
        .map(|r| r.bindings.clone())
        .unwrap_or_default()
}

/// The names of the other players in view, over their heads.
fn player_names(painter: &egui::Painter, view: &GameView) {
    for p in view.sim.players().filter(|p| p.id != view.hero) {
        let Some(at) = (view.head)(p.id) else {
            continue;
        };
        if !view.rect.contains(at) {
            continue;
        }
        // Killers of players are marked red.
        let color = if p.pk_kills > 0 {
            style::BAD
        } else {
            Color32::WHITE
        };
        let galley = painter.layout_no_wrap(p.name.clone(), fonts::font(12.0), color);
        let pos = at + egui::vec2(-galley.size().x / 2.0, -galley.size().y - 4.0);
        painter.rect_filled(
            Rect::from_min_size(
                pos - egui::vec2(4.0, 1.0),
                galley.size() + egui::vec2(8.0, 2.0),
            ),
            3.0,
            style::backing(0.9),
        );
        painter.galley(pos, galley, Color32::WHITE);
    }
}

/// A marker and label on each drop in view.
fn drop_labels(painter: &egui::Painter, view: &GameView) {
    for item in view.sim.ground_items() {
        let at = (view.cell)(item.cell);
        if !view.rect.contains(at) {
            continue;
        }
        let (text, color) = match item.kind {
            Some(k) => (item_name(view.sim, k), style::ITEM),
            None => (
                format!("{} {}", item.money, view.sim.settings().currency),
                style::MONEY,
            ),
        };
        painter.circle_filled(at, 4.0, color);
        painter.circle_stroke(at, 4.0, egui::Stroke::new(1.0, Color32::BLACK));
        let galley = painter.layout_no_wrap(text, fonts::font(11.0), color);
        let pos = at + egui::vec2(-galley.size().x / 2.0, -galley.size().y - 7.0);
        painter.rect_filled(
            Rect::from_min_size(
                pos - egui::vec2(3.0, 1.0),
                galley.size() + egui::vec2(6.0, 2.0),
            ),
            2.0,
            style::backing(0.9),
        );
        painter.galley(pos, galley, color);
    }
}
