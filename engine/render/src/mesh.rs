//! Lit, textured triangle meshes with depth testing. Vertices arrive already posed
//! (skinning happens on the CPU, like the classic client).

use crate::texture::{TextureId, Textures};
use crate::{COLOR_FORMAT, DEPTH_FORMAT};
use glam::Mat4;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// One mesh to draw this frame.
pub struct MeshDraw<'a> {
    pub vertices: &'a [MeshVertex],
    pub indices: &'a [u32],
    pub texture: Option<TextureId>,
    /// Model → world transform.
    pub transform: Mat4,
    pub tint: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    mvp: [f32; 16],
    model: [f32; 16],
    tint: [f32; 4],
}

pub(crate) struct Prepared {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    uniforms: wgpu::BindGroup,
    texture: Option<TextureId>,
}

pub(crate) struct MeshPass {
    pipeline: wgpu::RenderPipeline,
    uniform_layout: wgpu::BindGroupLayout,
}

const SHADER: &str = r#"
struct U { mvp: mat4x4<f32>, model: mat4x4<f32>, tint: vec4<f32> };
@group(0) @binding(0) var<uniform> u: U;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct Out { @builtin(position) pos: vec4<f32>, @location(0) normal: vec3<f32>, @location(1) uv: vec2<f32> };

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>) -> Out {
    var o: Out;
    o.pos = u.mvp * vec4(pos, 1.0);
    o.normal = (u.model * vec4(normal, 0.0)).xyz;
    o.uv = uv;
    return o;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, in.uv) * u.tint;
    if (c.a < 0.05) { discard; }
    let n = normalize(in.normal);
    // Two-sided half-Lambert so thin, unlit-authored meshes never go black.
    let light = 0.55 + 0.45 * abs(dot(n, normalize(vec3(0.4, -0.7, -0.6))));
    return vec4(c.rgb * light, c.a);
}
"#;

/// Creates a uniform buffer of `size` bytes with its layout and bind group (binding 0).
pub(crate) fn uniform(
    device: &wgpu::Device,
    label: &str,
    size: u64,
) -> (wgpu::Buffer, wgpu::BindGroupLayout, wgpu::BindGroup) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let layout = uniform_layout(device, label);
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    (buffer, layout, group)
}

fn uniform_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

impl MeshPass {
    pub fn new(device: &wgpu::Device, texture_layout: &wgpu::BindGroupLayout) -> Self {
        let uniform_layout = uniform_layout(device, "c3 mesh uniforms");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("c3 mesh shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("c3 mesh layout"),
            bind_group_layouts: &[Some(&uniform_layout), Some(texture_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("c3 mesh pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<MeshVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            // C3 meshes mix windings; draw both faces.
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            uniform_layout,
        }
    }

    pub fn prepare(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        view: Mat4,
        draws: &[MeshDraw],
    ) -> Vec<Prepared> {
        draws
            .iter()
            .filter(|d| !d.vertices.is_empty() && !d.indices.is_empty())
            .map(|d| {
                let uniforms = Uniforms {
                    mvp: (view * d.transform).to_cols_array(),
                    model: d.transform.to_cols_array(),
                    tint: d.tint,
                };
                let buffer = |label, contents: &[u8], usage| {
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(label),
                        contents,
                        usage,
                    })
                };
                let uniform_buffer = buffer(
                    "c3 mesh uniforms",
                    bytemuck::bytes_of(&uniforms),
                    wgpu::BufferUsages::UNIFORM,
                );
                Prepared {
                    vertices: buffer(
                        "c3 mesh vertices",
                        bytemuck::cast_slice(d.vertices),
                        wgpu::BufferUsages::VERTEX,
                    ),
                    indices: buffer(
                        "c3 mesh indices",
                        bytemuck::cast_slice(d.indices),
                        wgpu::BufferUsages::INDEX,
                    ),
                    count: d.indices.len() as u32,
                    uniforms: device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("c3 mesh uniforms"),
                        layout: &self.uniform_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform_buffer.as_entire_binding(),
                        }],
                    }),
                    texture: d.texture,
                }
            })
            .collect()
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass, prepared: &[Prepared], textures: &Textures) {
        if prepared.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        for p in prepared {
            pass.set_bind_group(0, &p.uniforms, &[]);
            pass.set_bind_group(1, textures.group(p.texture), &[]);
            pass.set_vertex_buffer(0, p.vertices.slice(..));
            pass.set_index_buffer(p.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..p.count, 0, 0..1);
        }
    }
}
