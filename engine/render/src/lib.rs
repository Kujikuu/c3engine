//! c3-render: wgpu renderer for 2.5D scenes — textured sprites (ground tiles, 2D art)
//! and lit, textured meshes (skinned models, posed on the CPU).
//!
//! It renders into any color view plus a [`DepthBuffer`]: an offscreen [`RenderTarget`]
//! (the Studio shows it as an egui image) or a window surface (the game client).
//! Engine types only: images come in as [`c3_assets::Image`].

pub mod actor;
mod assets;
pub mod ground;
mod mesh;
pub mod objects;
pub mod parallax;
mod sprite;
mod target;
mod texture;

pub use assets::SceneAssets;
pub use mesh::{MeshDraw, MeshVertex};
pub use sprite::Sprite;
pub use target::{DepthBuffer, RenderTarget};
pub use texture::TextureId;

use glam::Mat4;

/// Default color format; offscreen [`RenderTarget`]s always use it.
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The camera's viewpoint for building a frame's draw lists.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// Visible world rect `[min_x, min_y, max_x, max_y]`.
    pub rect: [f32; 4],
    /// World point at the screen centre.
    pub center: (f32, f32),
    /// Screen pixels per world pixel.
    pub zoom: f32,
    /// Animation clock.
    pub time_ms: u64,
}

impl View {
    /// Extra world size per tile that hides seams (half a screen pixel).
    pub fn seam(&self) -> f32 {
        0.5 / self.zoom
    }
}

/// Everything to draw in one frame.
#[derive(Default)]
pub struct Frame<'a> {
    pub clear: [f64; 4],
    /// World → clip transform for sprites (usually an orthographic 2D camera).
    pub sprite_view: Mat4,
    pub sprites: &'a [Sprite],
    /// World → clip transform for meshes.
    pub mesh_view: Mat4,
    pub meshes: &'a [MeshDraw<'a>],
    /// Painter's order mixing sprites and meshes. Empty = all sprites, then all meshes.
    pub order: &'a [DrawCmd],
}

/// One step of a frame's painter's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawCmd {
    /// Sprites with these indices.
    Sprites(std::ops::Range<u32>),
    /// The mesh with this index; each later mesh is drawn in front of earlier ones.
    Mesh(u32),
}

pub struct Renderer {
    color_format: wgpu::TextureFormat,
    device: wgpu::Device,
    queue: wgpu::Queue,
    textures: texture::Textures,
    sprites: sprite::SpritePass,
    meshes: mesh::MeshPass,
}

impl Renderer {
    /// Renderer drawing in [`COLOR_FORMAT`] (for offscreen targets).
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self::with_format(device, queue, COLOR_FORMAT)
    }

    /// Renderer drawing into views of `color_format` (e.g. a window surface's format).
    pub fn with_format(
        device: wgpu::Device,
        queue: wgpu::Queue,
        color_format: wgpu::TextureFormat,
    ) -> Self {
        let textures = texture::Textures::new(&device, &queue);
        let sprites = sprite::SpritePass::new(&device, textures.layout(), color_format);
        let meshes = mesh::MeshPass::new(&device, textures.layout(), color_format);
        Self {
            color_format,
            device,
            queue,
            textures,
            sprites,
            meshes,
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Uploads an image; the id stays valid until [`Renderer::release_texture`].
    pub fn upload_texture(&mut self, image: &c3_assets::Image) -> TextureId {
        self.textures.upload(&self.device, &self.queue, image)
    }

    pub fn release_texture(&mut self, id: TextureId) {
        self.textures.release(id);
    }

    pub fn color_format(&self) -> wgpu::TextureFormat {
        self.color_format
    }

    /// Creates or resizes `target` to `size` (physical pixels).
    pub fn ensure_target(&self, target: &mut Option<RenderTarget>, size: [u32; 2]) {
        debug_assert_eq!(
            self.color_format, COLOR_FORMAT,
            "offscreen targets need a `Renderer::new` renderer"
        );
        let size = [size[0].max(1), size[1].max(1)];
        if target.as_ref().is_none_or(|t| t.size != size) {
            *target = Some(RenderTarget::new(&self.device, size));
        }
    }

    /// Draws `frame` into an offscreen `target` and submits the work.
    pub fn render(&mut self, target: &RenderTarget, frame: &Frame) {
        self.render_into(&target.color_view, &target.depth, frame);
    }

    /// Draws `frame` into `color` (of this renderer's format) using `depth`, and submits.
    pub fn render_into(&mut self, color: &wgpu::TextureView, depth: &DepthBuffer, frame: &Frame) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("c3 frame"),
            });
        let sprite_batches =
            self.sprites
                .prepare(&self.device, &self.queue, frame.sprite_view, frame.sprites);
        let default_order: Vec<DrawCmd>;
        let order = if frame.order.is_empty() {
            default_order = std::iter::once(DrawCmd::Sprites(0..frame.sprites.len() as u32))
                .chain((0..frame.meshes.len() as u32).map(DrawCmd::Mesh))
                .collect();
            &default_order[..]
        } else {
            frame.order
        };
        // The k-th mesh in painter's order gets depth slice k of n; later = nearer.
        let mut rank = vec![0usize; frame.meshes.len()];
        let mut n = 0;
        for cmd in order {
            if let DrawCmd::Mesh(i) = cmd {
                rank[*i as usize] = n;
                n += 1;
            }
        }
        let slice = 1.0 / n.max(1) as f32;
        let meshes = self
            .meshes
            .prepare(&self.device, frame.mesh_view, frame.meshes, |i| {
                (1.0 - (rank[i] + 1) as f32 * slice, slice)
            });
        {
            let [r, g, b, a] = frame.clear;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("c3 main pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            for cmd in order {
                match cmd {
                    DrawCmd::Sprites(range) => self.sprites.draw_range(
                        &mut pass,
                        &sprite_batches,
                        &self.textures,
                        range.clone(),
                    ),
                    DrawCmd::Mesh(i) => {
                        if let Some(Some(p)) = meshes.get(*i as usize) {
                            self.meshes.draw_one(&mut pass, p, &self.textures);
                        }
                    }
                }
            }
        }
        self.queue.submit([encoder.finish()]);
    }
}
