//! 2D world camera for isometric maps: a world-pixel centre and a zoom factor.

use glam::camera::rh::proj::directx;
use glam::{Mat4, Vec2};

pub const MIN_ZOOM: f32 = 0.05;
pub const MAX_ZOOM: f32 = 4.0;

pub struct Camera2d {
    /// World point at the centre of the screen.
    pub center: Vec2,
    /// Screen pixels per world pixel (1.0 = classic client scale).
    pub zoom: f32,
}

impl Camera2d {
    /// Visible world rect `[min_x, min_y, max_x, max_y]` for a viewport in pixels.
    pub fn view_rect(&self, viewport: Vec2) -> [f32; 4] {
        let half = viewport / (2.0 * self.zoom);
        [
            self.center.x - half.x,
            self.center.y - half.y,
            self.center.x + half.x,
            self.center.y + half.y,
        ]
    }

    /// World → clip transform (y down, like screen space).
    pub fn view_proj(&self, viewport: Vec2) -> Mat4 {
        let [l, t, r, b] = self.view_rect(viewport);
        directx::orthographic(l, r, b, t, -1.0, 1.0)
    }

    pub fn screen_to_world(&self, screen: Vec2, viewport: Vec2) -> Vec2 {
        self.center + (screen - viewport / 2.0) / self.zoom
    }

    /// Zooms by `factor`, keeping the world point under `anchor` (screen pixels) fixed.
    pub fn zoom_at(&mut self, factor: f32, anchor: Vec2, viewport: Vec2) {
        let before = self.screen_to_world(anchor, viewport);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.center += before - self.screen_to_world(anchor, viewport);
    }

    /// Centres and zooms so `world` (`[min_x, min_y, w, h]`) fills the viewport.
    pub fn fit(&mut self, world: [f32; 4], viewport: Vec2) {
        self.center = Vec2::new(world[0] + world[2] / 2.0, world[1] + world[3] / 2.0);
        // 5% margin, applied before clamping so the result stays in range.
        self.zoom =
            ((viewport.x / world[2]).min(viewport.y / world[3]) * 0.95).clamp(MIN_ZOOM, MAX_ZOOM);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_anchor_fixed() {
        let vp = Vec2::new(800.0, 600.0);
        let mut c = Camera2d {
            center: Vec2::new(100.0, 50.0),
            zoom: 1.0,
        };
        let anchor = Vec2::new(700.0, 100.0);
        let before = c.screen_to_world(anchor, vp);
        c.zoom_at(2.0, anchor, vp);
        assert!((c.screen_to_world(anchor, vp) - before).length() < 1e-3);
    }
}
