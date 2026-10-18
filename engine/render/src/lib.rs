//! c3-render: wgpu renderer for 2.5D scenes — textured sprites (ground tiles, 2D art)
//! and lit, textured meshes (skinned models, posed on the CPU).
//!
//! It renders into any color view plus a [`DepthBuffer`]: an offscreen [`RenderTarget`]
//! (the Studio shows it as an egui image) or a window surface (the game client).
//! Engine types only: images come in as [`c3_assets::Image`].

pub mod actor;
mod assets;
pub mod camera;
pub mod effect;
pub mod effect_library;
pub mod ground;
mod mesh;
pub mod minimap;
pub mod objects;
pub mod parallax;
pub mod scene_view;
mod sprite;
mod target;
mod texture;

pub use assets::SceneAssets;
pub use mesh::{MeshDraw, MeshStyle, MeshVertex};
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
    pub light: Light,
}

impl Frame<'_> {
    /// Nothing but black (what draws under a screen of UI alone).
    pub fn empty() -> Frame<'static> {
        Frame {
            clear: [0.0, 0.0, 0.0, 1.0],
            sprite_view: Mat4::IDENTITY,
            sprites: &[],
            mesh_view: Mat4::IDENTITY,
            meshes: &[],
            order: &[],
            light: Light::default(),
        }
    }
}

/// The scene's light: a colour over the map, sprites and solid meshes (effects keep
/// their glow), and whether models are shaded by a sun.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Light {
    /// RGB multiplier (linear); white leaves colours as they are.
    pub ambient: [f32; 3],
    /// Shades models by `sun` (half-Lambert), as if lit from one side.
    pub shade_models: bool,
    /// Direction the sunlight travels (world, normalized in the shader).
    pub sun: [f32; 3],
}

impl Default for Light {
    fn default() -> Self {
        Self {
            ambient: [1.0; 3],
            shade_models: false,
            sun: [0.4, -0.7, -0.6],
        }
    }
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
    /// The pixels of `target` (sRGB RGBA), read back from the GPU.
    pub fn read_target(&self, target: &RenderTarget) -> c3_assets::Image {
        let [w, h] = target.size;
        let row = (4 * w).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("c3 readback"),
            size: u64::from(row * h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            target.color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        // Waiting can only fail if the device is lost; the pixels then read as black.
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let rgba = match buffer.slice(..).get_mapped_range() {
            Ok(data) => (0..h)
                .flat_map(|y| data[(y * row) as usize..(y * row + 4 * w) as usize].to_vec())
                .collect(),
            Err(_) => vec![0; (4 * w * h) as usize],
        };
        c3_assets::Image::new(w, h, rgba)
    }

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
        let sprite_batches = self.sprites.prepare(
            &self.device,
            &self.queue,
            frame.sprite_view,
            frame.light,
            frame.sprites,
        );
        let default_order: Vec<DrawCmd>;
        let order = if frame.order.is_empty() {
            default_order = std::iter::once(DrawCmd::Sprites(0..frame.sprites.len() as u32))
                .chain((0..frame.meshes.len() as u32).map(DrawCmd::Mesh))
                .collect();
            &default_order[..]
        } else {
            frame.order
        };
        // The k-th mesh (or run of same-group meshes) in painter's order gets depth slice k
        // of n; later = nearer.
        let mut rank = vec![0usize; frame.meshes.len()];
        let mut n = 0;
        let mut previous_group = None;
        for cmd in order {
            if let DrawCmd::Mesh(i) = cmd {
                let group = frame.meshes[*i as usize].group;
                if group.is_none() || group != previous_group {
                    n += 1;
                }
                previous_group = group;
                rank[*i as usize] = n - 1;
            }
        }
        let slice = 1.0 / n.max(1) as f32;
        let meshes = self.meshes.prepare(
            &self.device,
            frame.mesh_view,
            frame.light,
            frame.meshes,
            |i| (1.0 - (rank[i] + 1) as f32 * slice, slice),
        );
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
