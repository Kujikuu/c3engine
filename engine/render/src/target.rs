//! Offscreen color + depth target.

use crate::{COLOR_FORMAT, DEPTH_FORMAT};

pub struct RenderTarget {
    pub size: [u32; 2],
    pub color: wgpu::Texture,
    /// sRGB view the renderer draws into.
    pub color_view: wgpu::TextureView,
    /// Same pixels viewed as plain `Rgba8Unorm`: for hosts that composite in gamma space
    /// (e.g. egui), so the encoded sRGB bytes are shown as-is.
    pub display_view: wgpu::TextureView,
    pub depth_view: wgpu::TextureView,
}

impl RenderTarget {
    pub fn new(device: &wgpu::Device, size: [u32; 2]) -> Self {
        let extent = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };
        let make = |label, format, usage, view_formats: &[wgpu::TextureFormat]| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats,
            })
        };
        let color = make(
            "c3 target color",
            COLOR_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            &[wgpu::TextureFormat::Rgba8Unorm],
        );
        let depth = make(
            "c3 target depth",
            DEPTH_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
            &[],
        );
        Self {
            size,
            color_view: color.create_view(&Default::default()),
            display_view: color.create_view(&wgpu::TextureViewDescriptor {
                format: Some(wgpu::TextureFormat::Rgba8Unorm),
                ..Default::default()
            }),
            depth_view: depth.create_view(&Default::default()),
            color,
        }
    }
}
