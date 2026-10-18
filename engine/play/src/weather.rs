//! Weather over the game view: rain, snow or falling leaves, drawn in screen space while the
//! hero stands in a region that has it (as TQ's client draws its weather over the map).

use c3_scene::{Weather, WeatherKind};
use egui::{Color32, Rect, Stroke, Vec2};

/// Particles at 100% (fewer at less).
const MOST: usize = 400;

/// One falling thing, its position as a fraction of the view.
struct Flake {
    at: Vec2,
    /// Fraction of the view per second.
    speed: Vec2,
    /// Size factor and phase, for variety.
    size: f32,
    phase: f32,
}

#[derive(Default)]
pub struct WeatherOverlay {
    weather: Option<Weather>,
    flakes: Vec<Flake>,
    seed: u32,
    clock: f32,
}

impl WeatherOverlay {
    /// What falls from now on (`None`: clear skies).
    pub fn set(&mut self, weather: Option<Weather>) {
        if weather == self.weather {
            return;
        }
        self.weather = weather;
        let count = weather.map_or(0, |w| MOST * usize::from(w.amount.min(100)) / 100);
        self.flakes = (0..count).map(|_| self.flake(false)).collect();
    }

    /// Moves the weather on by `dt` seconds and draws it over `rect`.
    pub fn show(&mut self, painter: &egui::Painter, rect: Rect, dt: f32) {
        let Some(weather) = self.weather else { return };
        self.clock += dt;
        for i in 0..self.flakes.len() {
            let f = &mut self.flakes[i];
            f.at += f.speed * dt;
            if f.at.y > 1.05 || f.at.x < -0.05 || f.at.x > 1.05 {
                self.flakes[i] = self.flake(true);
            }
        }
        let size = rect.size();
        for f in &self.flakes {
            let p = rect.min + f.at * size;
            match weather.kind {
                WeatherKind::Rain => {
                    let tail = f.speed.normalized() * 14.0 * f.size;
                    let color = Color32::from_rgba_unmultiplied(190, 210, 255, 110);
                    painter.line_segment([p - tail, p], Stroke::new(1.0, color));
                }
                WeatherKind::Snow => {
                    let sway = (self.clock * 1.5 + f.phase).sin() * 6.0;
                    let color = Color32::from_rgba_unmultiplied(255, 255, 255, 200);
                    painter.circle_filled(p + Vec2::new(sway, 0.0), 1.5 * f.size + 0.5, color);
                }
                WeatherKind::Leaves => {
                    let turn = self.clock * 2.0 + f.phase;
                    let sway = Vec2::new(turn.sin() * 10.0, 0.0);
                    let (w, h) = (4.0 * f.size, 2.0 * f.size * turn.cos().abs().max(0.3));
                    let color = Color32::from_rgba_unmultiplied(210, 130, 60, 220);
                    let leaf = Rect::from_center_size(p + sway, Vec2::new(w, h));
                    painter.rect_filled(leaf, 1.5, color);
                }
            }
        }
    }

    /// A new flake: anywhere in the view, or (`top`) just above it.
    fn flake(&mut self, top: bool) -> Flake {
        let (x, y, size, phase) = (self.next(), self.next(), self.next(), self.next());
        let kind = self.weather.map_or(WeatherKind::Rain, |w| w.kind);
        let speed = match kind {
            WeatherKind::Rain => Vec2::new(-0.25, 1.6 + size * 0.6),
            WeatherKind::Snow => Vec2::new(-0.02, 0.10 + size * 0.08),
            WeatherKind::Leaves => Vec2::new(0.06, 0.12 + size * 0.06),
        };
        Flake {
            at: Vec2::new(x * 1.1, if top { -0.05 } else { y }),
            speed,
            size: 0.6 + size,
            phase: phase * std::f32::consts::TAU,
        }
    }

    /// A number in 0..1 (a small xorshift; looks need no better).
    fn next(&mut self) -> f32 {
        let mut x = self.seed.wrapping_add(0x9E37_79B9);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}
