//! Render destinations: a depth buffer, and an offscreen color + depth target.

use crate::{COLOR_FORMAT, DEPTH_FORMAT};

fn texture(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
    view_formats: &[wgpu::TextureFormat],
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size[0].max(1),
            height: size[1].max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats,
    })
}

pub struct DepthBuffer {
    pub size: [u32; 2],
    pub view: wgpu::TextureView,
}

impl DepthBuffer {
    pub fn new(device: &wgpu::Device, size: [u32; 2]) -> Self {
        let t = texture(
            device,
            "c3 depth",
            size,
            DEPTH_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
            &[],
        );
        Self {
            size,
            view: t.create_view(&Default::default()),
        }
    }

    /// Recreates the buffer if `size` changed.
    pub fn ensure(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.size != size {
            *self = Self::new(device, size);
        }
    }
}

pub struct RenderTarget {
    pub size: [u32; 2],
    pub color: wgpu::Texture,
    /// sRGB view the renderer draws into.
    pub color_view: wgpu::TextureView,
    /// Same pixels viewed as plain `Rgba8Unorm`: for hosts that composite in gamma space
    /// (e.g. egui), so the encoded sRGB bytes are shown as-is.
    pub display_view: wgpu::TextureView,
    pub depth: DepthBuffer,
}

impl RenderTarget {
    pub fn new(device: &wgpu::Device, size: [u32; 2]) -> Self {
        let color = texture(
            device,
            "c3 target color",
            size,
            COLOR_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            &[wgpu::TextureFormat::Rgba8Unorm],
        );
        Self {
            size,
            color_view: color.create_view(&Default::default()),
            display_view: color.create_view(&wgpu::TextureViewDescriptor {
                format: Some(wgpu::TextureFormat::Rgba8Unorm),
                ..Default::default()
            }),
            depth: DepthBuffer::new(device, size),
            color,
        }
    }
}
