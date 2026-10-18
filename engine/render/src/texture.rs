//! GPU texture storage, addressed by [`TextureId`].

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureId(u32);

pub(crate) struct Textures {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    groups: HashMap<TextureId, wgpu::BindGroup>,
    /// 1×1 white, used for untextured draws.
    white: wgpu::BindGroup,
    next: u32,
}

impl Textures {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("c3 texture layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("c3 sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let white_texture = Self::create(device, [1, 1]);
        Self::write(queue, &white_texture, [1, 1], &[255; 4]);
        let white = Self::bind(
            device,
            &layout,
            &sampler,
            &white_texture.create_view(&Default::default()),
        );
        Self {
            layout,
            sampler,
            groups: HashMap::new(),
            white,
            next: 0,
        }
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    fn create(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("c3 texture"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    fn bind(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("c3 texture group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    fn write(queue: &wgpu::Queue, texture: &wgpu::Texture, size: [u32; 2], rgba: &[u8]) {
        queue.write_texture(
            texture.as_image_copy(),
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * size[0]),
                rows_per_image: Some(size[1]),
            },
            wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &c3_assets::Image,
    ) -> TextureId {
        let size = [image.width.max(1), image.height.max(1)];
        let texture = Self::create(device, size);
        if image.width > 0 && image.height > 0 {
            Self::write(queue, &texture, size, &image.rgba);
        }
        let group = Self::bind(
            device,
            &self.layout,
            &self.sampler,
            &texture.create_view(&Default::default()),
        );
        let id = TextureId(self.next);
        self.next += 1;
        self.groups.insert(id, group);
        id
    }

    pub fn release(&mut self, id: TextureId) {
        self.groups.remove(&id);
    }

    /// Bind group for `id`, or plain white when `None` or released.
    pub fn group(&self, id: Option<TextureId>) -> &wgpu::BindGroup {
        id.and_then(|id| self.groups.get(&id))
            .unwrap_or(&self.white)
    }
}
