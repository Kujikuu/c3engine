//! c3-render: wgpu renderer for 2.5D scenes — textured sprites (ground tiles, 2D art)
//! and lit, textured meshes (skinned models, posed on the CPU).
//!
//! It renders into a [`RenderTarget`] (color + depth). Hosts decide what to do with the
//! result: the Studio shows it as an egui image, a game presents it to a window.
//! Engine types only: images come in as [`c3_assets::Image`].

mod mesh;
mod sprite;
mod target;
mod texture;

pub use mesh::{MeshDraw, MeshVertex};
pub use sprite::Sprite;
pub use target::RenderTarget;
pub use texture::TextureId;

use glam::Mat4;

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
    device: wgpu::Device,
    queue: wgpu::Queue,
    textures: texture::Textures,
    sprites: sprite::SpritePass,
    meshes: mesh::MeshPass,
}

impl Renderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let textures = texture::Textures::new(&device, &queue);
        let sprites = sprite::SpritePass::new(&device, textures.layout());
        let meshes = mesh::MeshPass::new(&device, textures.layout());
        Self {
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

    /// Creates or resizes `target` to `size` (physical pixels).
    pub fn ensure_target(&self, target: &mut Option<RenderTarget>, size: [u32; 2]) {
        let size = [size[0].max(1), size[1].max(1)];
        if target.as_ref().is_none_or(|t| t.size != size) {
            *target = Some(RenderTarget::new(&self.device, size));
        }
    }

    /// Draws `frame` into `target` and submits the work.
    pub fn render(&mut self, target: &RenderTarget, frame: &Frame) {
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
                    view: &target.color_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
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
