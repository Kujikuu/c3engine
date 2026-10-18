//! Model viewport: renders an engine `Model` with the engine renderer, with an orbit
//! camera and animation playback. Models are −Z up (C3 convention).

use crate::gpu::GpuView;
use crate::tq_plugin::{Client, ModelEntry};
use c3_assets::model::Model;
use c3_render::{Frame, MeshDraw, MeshVertex, TextureId, actor};
use eframe::egui::{self, Color32, Pos2, Rect, Sense};
use glam::camera::rh::{proj::directx, view::look_at_mat4};
use glam::{Mat4, Vec3};

/// Playback rate of C3 motions.
pub const FPS: f32 = 30.0;

pub struct ModelDoc {
    pub entry: ModelEntry,
    pub model: Model,
    pub texture_path: Option<String>,
    /// Uploaded texture: outer `None` = not attempted yet, inner `None` = unavailable.
    texture: Option<Option<TextureId>>,
    /// Bind-pose bounds centre and radius, for framing.
    pub center: Vec3,
    pub radius: f32,
    pub frame: f32,
    pub playing: bool,
    pub speed: f32,
}

impl ModelDoc {
    pub fn new(entry: ModelEntry, model: Model, texture_path: Option<String>) -> Self {
        let points = model
            .parts
            .iter()
            .flat_map(|p| p.mesh.vertices.iter().map(|v| v.position));
        let (min, max) = points.fold(
            (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
            |(a, b), v| (a.min(v), b.max(v)),
        );
        let (center, radius) = if min.x <= max.x {
            ((min + max) / 2.0, ((max - min).length() / 2.0).max(1.0))
        } else {
            (Vec3::ZERO, 1.0)
        };
        Self {
            entry,
            model,
            texture_path,
            texture: None,
            center,
            radius,
            frame: 0.0,
            playing: true,
            speed: 1.0,
        }
    }

    pub fn frame_count(&self) -> u32 {
        self.model
            .parts
            .iter()
            .filter_map(|p| p.clip.as_ref())
            .map(|c| c.frame_count)
            .max()
            .unwrap_or(0)
    }

    pub fn advance(&mut self, dt: f32) {
        let frames = self.frame_count() as f32;
        if self.playing && frames > 0.0 {
            self.frame = (self.frame + dt * FPS * self.speed).rem_euclid(frames);
        }
    }

    pub fn step(&mut self, delta: i32) {
        let frames = self.frame_count().max(1) as f32;
        self.playing = false;
        self.frame = (self.frame.floor() + delta as f32).rem_euclid(frames);
    }

    /// Releases GPU resources owned by this document.
    pub fn release(&mut self, gpu: &mut GpuView) {
        if let Some(Some(t)) = self.texture.take() {
            gpu.renderer.release_texture(t);
        }
    }

    fn texture(&mut self, client: &Client, gpu: &mut GpuView) -> Option<TextureId> {
        *self.texture.get_or_insert_with(|| {
            let img = self
                .texture_path
                .as_deref()
                .and_then(|p| client.load_image(p))?;
            Some(gpu.renderer.upload_texture(&img))
        })
    }
}

pub struct OrbitCamera {
    /// Radians around the up axis.
    pub yaw: f32,
    /// Radians above the horizon.
    pub pitch: f32,
    /// Multiple of the model radius.
    pub distance: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: 0.9,
            pitch: 0.35,
            distance: 2.6,
        }
    }
}

pub fn show(
    ui: &mut egui::Ui,
    client: &Client,
    gpu: &mut GpuView,
    doc: &mut ModelDoc,
    cam: &mut OrbitCamera,
) {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    if resp.dragged() {
        let d = resp.drag_delta();
        cam.yaw -= d.x * 0.01;
        cam.pitch = (cam.pitch + d.y * 0.01).clamp(-1.5, 1.5);
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        cam.distance = (cam.distance * (-scroll * 0.002).exp()).clamp(0.4, 12.0);
    }
    doc.advance(ui.input(|i| i.stable_dt));
    if doc.playing {
        ui.ctx().request_repaint();
    }

    let texture = doc.texture(client, gpu);
    let parts: Vec<(Vec<MeshVertex>, &[u32])> = (0..doc.model.parts.len())
        .map(|i| {
            (
                actor::posed_vertices(
                    &doc.model.parts[i].mesh,
                    doc.model.parts[i].clip.as_ref(),
                    doc.frame,
                ),
                doc.model.parts[i].mesh.indices.as_slice(),
            )
        })
        .collect();
    let draws: Vec<MeshDraw> = parts
        .iter()
        .map(|(v, i)| MeshDraw {
            vertices: v,
            indices: i,
            texture,
            transform: Mat4::IDENTITY,
            tint: [1.0; 4],
            group: Some(0),
            // Lit, so shapes read clearly in the editor.
            style: c3_render::MeshStyle {
                lit: true,
                ..c3_render::MeshStyle::SOLID
            },
        })
        .collect();

    // −Z up: the horizon is the XY plane.
    let dir = Vec3::new(
        cam.pitch.cos() * cam.yaw.cos(),
        cam.pitch.cos() * cam.yaw.sin(),
        -cam.pitch.sin(),
    );
    let eye = doc.center + dir * doc.radius * cam.distance;
    let view = look_at_mat4(eye, doc.center, Vec3::NEG_Z);
    let aspect = rect.width() / rect.height().max(1.0);
    let far = doc.radius * (cam.distance + 4.0);
    let proj = directx::perspective(45f32.to_radians(), aspect, far / 1000.0, far);

    let ppp = ui.ctx().pixels_per_point();
    let size = [
        (rect.width() * ppp).round() as u32,
        (rect.height() * ppp).round() as u32,
    ];
    let frame = Frame {
        clear: [0.010, 0.011, 0.014, 1.0],
        mesh_view: proj * view,
        meshes: &draws,
        ..Default::default()
    };
    let tex = gpu.render(size, &frame);
    ui.painter_at(rect).image(
        tex,
        rect,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
}
