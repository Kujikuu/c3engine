//! Instanced textured quads in world space (ground tiles, 2D sprites). No depth test:
//! sprites draw in submission order, under meshes.

use crate::DEPTH_FORMAT;
use crate::texture::{TextureId, Textures, Wrap};
use glam::Mat4;
use wgpu::util::DeviceExt;

/// An axis-aligned textured rectangle in world units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    /// Top-left corner and size.
    pub rect: [f32; 4],
    /// Texture sub-rectangle (u0, v0, u1, v1).
    pub uv: [f32; 4],
    pub texture: Option<TextureId>,
    /// RGBA multiplier (linear).
    pub tint: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    rect: [f32; 4],
    uv: [f32; 4],
    tint: [f32; 4],
}

pub(crate) struct Batches {
    instances: Option<wgpu::Buffer>,
    /// (texture, first instance, count), in draw order.
    runs: Vec<(Option<TextureId>, u32, u32)>,
}

pub(crate) struct SpritePass {
    pipeline: wgpu::RenderPipeline,
    view_buffer: wgpu::Buffer,
    view_group: wgpu::BindGroup,
}

const SHADER: &str = r#"
struct View { m: mat4x4<f32> };
@group(0) @binding(0) var<uniform> view: View;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct Out { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) tint: vec4<f32> };

@vertex
fn vs(@builtin(vertex_index) i: u32, @location(0) rect: vec4<f32>, @location(1) uv: vec4<f32>, @location(2) tint: vec4<f32>) -> Out {
    var corners = array<vec2<f32>, 6>(vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0));
    let c = corners[i];
    var o: Out;
    o.pos = view.m * vec4(rect.xy + c * rect.zw, 0.0, 1.0);
    o.uv = mix(uv.xy, uv.zw, c);
    o.tint = tint;
    return o;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, in.uv) * in.tint;
}
"#;

impl SpritePass {
    pub fn new(
        device: &wgpu::Device,
        texture_layout: &wgpu::BindGroupLayout,
        color_format: wgpu::TextureFormat,
    ) -> Self {
        let (view_buffer, view_layout, view_group) =
            crate::mesh::uniform(device, "c3 sprite view", 64);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("c3 sprite shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("c3 sprite layout"),
            bind_group_layouts: &[Some(&view_layout), Some(texture_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("c3 sprite pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            view_buffer,
            view_group,
        }
    }

    pub fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: Mat4,
        sprites: &[Sprite],
    ) -> Batches {
        queue.write_buffer(
            &self.view_buffer,
            0,
            bytemuck::cast_slice(&view.to_cols_array()),
        );
        let mut runs: Vec<(Option<TextureId>, u32, u32)> = Vec::new();
        let instances: Vec<Instance> = sprites
            .iter()
            .enumerate()
            .map(|(i, s)| {
                match runs.last_mut() {
                    Some(run) if run.0 == s.texture => run.2 += 1,
                    _ => runs.push((s.texture, i as u32, 1)),
                }
                Instance {
                    rect: s.rect,
                    uv: s.uv,
                    tint: s.tint,
                }
            })
            .collect();
        let instances = (!instances.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("c3 sprite instances"),
                contents: bytemuck::cast_slice(&instances),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        Batches { instances, runs }
    }

    /// Draws sprites with indices in `range` (submission order).
    pub fn draw_range(
        &self,
        pass: &mut wgpu::RenderPass,
        batches: &Batches,
        textures: &Textures,
        range: std::ops::Range<u32>,
    ) {
        let Some(instances) = &batches.instances else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.view_group, &[]);
        pass.set_vertex_buffer(0, instances.slice(..));
        for &(texture, first, count) in &batches.runs {
            let (start, end) = (first.max(range.start), (first + count).min(range.end));
            if start < end {
                pass.set_bind_group(1, textures.group(texture, Wrap::Clamp), &[]);
                pass.draw(0..6, start..end);
            }
        }
    }
}
