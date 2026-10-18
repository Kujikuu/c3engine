//! Model viewport: renders an engine `Model` with the engine renderer, with an orbit
//! camera and animation playback. Models are −Z up (C3 convention).

use crate::gpu::GpuView;
use crate::tq_plugin::ModelEntry;
use c3_anim::Clip;
use c3_assets::model::Model;
use c3_render::{Frame, MeshDraw, MeshVertex, SceneAssets, TextureId, actor};
use eframe::egui::{self, Color32, Pos2, Rect, Sense};
use glam::camera::rh::{proj::directx, view::look_at_mat4};
use glam::{Mat4, Vec3};
use std::collections::HashMap;

/// Playback rate of C3 motions.
pub const FPS: f32 = 30.0;

/// A model's named animation: one clip per part.
pub struct ModelAnimation {
    pub name: String,
    pub clips: Vec<Option<Clip>>,
}

pub struct ModelDoc {
    pub entry: ModelEntry,
    pub model: Model,
    /// One texture for the whole model (a TQ model's); else each part's own.
    pub texture_path: Option<String>,
    /// Uploaded textures by path (`None`: unavailable).
    textures: HashMap<String, Option<TextureId>>,
    /// The model's animations (glTF), and the one playing.
    pub animations: Vec<ModelAnimation>,
    pub animation: usize,
    /// Parts are tinted by their vertex colour (a glTF material's base colour).
    pub material_colors: bool,
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
            textures: HashMap::new(),
            animations: Vec::new(),
            animation: 0,
            material_colors: false,
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

    /// A glTF model: its animations (playing the first) and material colours.
    pub fn native(mut self, animations: Vec<ModelAnimation>) -> Self {
        self.animations = animations;
        self.material_colors = true;
        self.play_animation(0);
        self
    }

    /// The tint part `part` is drawn with.
    fn tint(&self, part: usize) -> [f32; 4] {
        let color = self.model.parts[part]
            .mesh
            .vertices
            .first()
            .map(|v| v.color);
        match color {
            Some(argb) if self.material_colors => {
                let [a, r, g, b] = argb.to_be_bytes().map(|c| f32::from(c) / 255.0);
                [r, g, b, a]
            }
            _ => [1.0; 4],
        }
    }

    /// Poses the model with animation `index`.
    pub fn play_animation(&mut self, index: usize) {
        let Some(a) = self.animations.get(index) else {
            return;
        };
        for (part, clip) in self.model.parts.iter_mut().zip(&a.clips) {
            part.clip = clip.clone();
        }
        self.animation = index;
        self.frame = 0.0;
    }

    /// Releases GPU resources owned by this document.
    pub fn release(&mut self, gpu: &mut GpuView) {
        for t in self.textures.drain().filter_map(|(_, t)| t) {
            gpu.renderer.release_texture(t);
        }
    }

    /// The texture part `part` is drawn with.
    fn texture(
        &mut self,
        part: usize,
        assets: &dyn SceneAssets,
        gpu: &mut GpuView,
    ) -> Option<TextureId> {
        let path = match &self.texture_path {
            Some(p) => p.clone(),
            None => self.model.parts[part].mesh.texture.clone(),
        };
        if path.is_empty() {
            return None;
        }
        *self.textures.entry(path).or_insert_with_key(|path| {
            let img = assets.image(path)?;
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
    assets: &dyn SceneAssets,
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

    let textures: Vec<Option<TextureId>> = (0..doc.model.parts.len())
        .map(|i| doc.texture(i, assets, gpu))
        .collect();
    let tints: Vec<[f32; 4]> = (0..doc.model.parts.len()).map(|i| doc.tint(i)).collect();
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
        .zip(textures.iter().zip(&tints))
        .map(|((v, i), (&texture, &tint))| MeshDraw {
            vertices: v,
            indices: i,
            texture,
            transform: Mat4::IDENTITY,
            tint,
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
