//! c3-render: wgpu renderer for 2.5D scenes — textured sprites (ground tiles, 2D art)
//! and lit, textured meshes (skinned models, posed on the CPU).
//!
//! It renders into any color view plus a [`DepthBuffer`]: an offscreen [`RenderTarget`]
//! (the Studio shows it as an egui image) or a window surface (the game client).
//! Engine types only: images come in as [`c3_assets::Image`].

pub mod ground;
mod mesh;
mod sprite;
mod target;
mod texture;

pub use mesh::{MeshDraw, MeshVertex};
pub use sprite::Sprite;
pub use target::{DepthBuffer, RenderTarget};
pub use texture::TextureId;

use glam::Mat4;

/// Default color format; offscreen [`RenderTarget`]s always use it.
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

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
        let mesh_batches =
            self.meshes
                .prepare(&self.device, &self.queue, frame.mesh_view, frame.meshes);
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
            self.sprites
                .draw(&mut pass, &sprite_batches, &self.textures);
            self.meshes.draw(&mut pass, &mesh_batches, &self.textures);
        }
        self.queue.submit([encoder.finish()]);
    }
}
