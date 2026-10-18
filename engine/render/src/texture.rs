//! GPU texture storage, addressed by [`TextureId`].

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureId(u32);

/// What sampling outside 0..1 texture coordinates does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wrap {
    /// Repeats the edge texel: tiles must not bleed into their opposite edge.
    Clamp = 0,
    /// Tiles the texture, as D3D does by default (meshes, scrolling effects).
    Repeat = 1,
}

/// A texture's bind groups, indexed by [`Wrap`].
type Groups = [wgpu::BindGroup; 2];

pub(crate) struct Textures {
    layout: wgpu::BindGroupLayout,
    /// Smooth, then sharp (pixel art); each by [`Wrap`].
    samplers: [[wgpu::Sampler; 2]; 2],
    groups: HashMap<TextureId, Groups>,
    /// 1×1 white, used for untextured draws.
    white: Groups,
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
        let sampler = |label, address_mode, filter| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                address_mode_u: address_mode,
                address_mode_v: address_mode,
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            })
        };
        let samplers = [wgpu::FilterMode::Linear, wgpu::FilterMode::Nearest].map(|filter| {
            [
                sampler("c3 clamp sampler", wgpu::AddressMode::ClampToEdge, filter),
                sampler("c3 repeat sampler", wgpu::AddressMode::Repeat, filter),
            ]
        });
        let white_texture = Self::create(device, [1, 1]);
        Self::write(queue, &white_texture, [1, 1], &[255; 4]);
        let white = Self::bind(
            device,
            &layout,
            &samplers[0],
            &white_texture.create_view(&Default::default()),
        );
        Self {
            layout,
            samplers,
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

    /// One bind group per [`Wrap`] mode.
    fn bind(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        samplers: &[wgpu::Sampler; 2],
        view: &wgpu::TextureView,
    ) -> Groups {
        samplers.each_ref().map(|sampler| {
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
            &self.samplers[usize::from(image.sharp)],
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
    pub fn group(&self, id: Option<TextureId>, wrap: Wrap) -> &wgpu::BindGroup {
        let groups = id
            .and_then(|id| self.groups.get(&id))
            .unwrap_or(&self.white);
        &groups[wrap as usize]
    }
}
