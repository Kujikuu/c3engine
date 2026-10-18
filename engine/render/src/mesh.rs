//! Textured triangle meshes with depth testing. Vertices arrive already posed
//! (skinning happens on the CPU, like the classic client).

use crate::DEPTH_FORMAT;
use crate::texture::{TextureId, Textures, Wrap};
use c3_assets::effect::{Blend, BlendFactor};
use glam::Mat4;
use std::collections::HashMap;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// How a mesh is shaded and combined with the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshStyle {
    pub blend: Blend,
    /// Simple directional light. Off draws texture × tint, like the classic client.
    pub lit: bool,
    /// Off for see-through effects, so they never hide what is drawn after them.
    pub depth_write: bool,
}

impl MeshStyle {
    /// Opaque or alpha-tested surfaces (characters, props).
    pub const SOLID: Self = Self {
        blend: Blend::ALPHA,
        lit: false,
        depth_write: true,
    };

    /// See-through effect geometry.
    pub fn effect(blend: Blend) -> Self {
        Self {
            blend,
            lit: false,
            depth_write: false,
        }
    }
}

/// One mesh to draw this frame.
pub struct MeshDraw<'a> {
    pub vertices: &'a [MeshVertex],
    pub indices: &'a [u32],
    pub texture: Option<TextureId>,
    /// Model → world transform.
    pub transform: Mat4,
    pub tint: [f32; 4],
    /// Meshes with the same group that are consecutive in painter's order share one depth
    /// slice, so they occlude each other by real depth (e.g. a body and its weapon).
    pub group: Option<u32>,
    pub style: MeshStyle,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    mvp: [f32; 16],
    model: [f32; 16],
    tint: [f32; 4],
    /// Depth slice: x = start, y = size (clip depth 0..1 is remapped into it).
    depth: [f32; 4],
    /// x = lit (0/1).
    shading: [f32; 4],
    /// rgb = colour multiplier of the scene light.
    ambient: [f32; 4],
    /// xyz = direction the sunlight travels.
    sun: [f32; 4],
}

/// Pipelines differ only in fixed-function state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PipelineKey {
    blend: Blend,
    depth_write: bool,
}

pub(crate) struct Prepared {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    uniforms: wgpu::BindGroup,
    texture: Option<TextureId>,
    pipeline: PipelineKey,
}

pub(crate) struct MeshPass {
    shader: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    color_format: wgpu::TextureFormat,
    pipelines: HashMap<PipelineKey, wgpu::RenderPipeline>,
    uniform_layout: wgpu::BindGroupLayout,
}

const SHADER: &str = r#"
struct U {
    mvp: mat4x4<f32>, model: mat4x4<f32>, tint: vec4<f32>, depth: vec4<f32>, shading: vec4<f32>,
    ambient: vec4<f32>, sun: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: U;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct Out { @builtin(position) pos: vec4<f32>, @location(0) normal: vec3<f32>, @location(1) uv: vec2<f32> };

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>) -> Out {
    var o: Out;
    o.pos = u.mvp * vec4(pos, 1.0);
    // Painter's order between meshes: each mesh owns a slice of the depth range.
    o.pos.z = (u.depth.x + clamp(o.pos.z / o.pos.w, 0.0, 1.0) * u.depth.y) * o.pos.w;
    o.normal = (u.model * vec4(normal, 0.0)).xyz;
    o.uv = uv;
    return o;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, in.uv) * u.tint * vec4(u.ambient.rgb, 1.0);
    // Classic alpha test (D3D ALPHAREF 8, GREATEREQUAL).
    if (c.a < 8.0 / 255.0) { discard; }
    if (u.shading.x == 0.0) { return c; }
    // Two-sided half-Lambert so thin meshes never go black.
    let n = normalize(in.normal);
    let light = 0.55 + 0.45 * abs(dot(n, normalize(u.sun.xyz)));
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

fn blend_factor(f: BlendFactor) -> wgpu::BlendFactor {
    match f {
        BlendFactor::Zero => wgpu::BlendFactor::Zero,
        BlendFactor::One => wgpu::BlendFactor::One,
        BlendFactor::SrcColor => wgpu::BlendFactor::Src,
        BlendFactor::OneMinusSrcColor => wgpu::BlendFactor::OneMinusSrc,
        BlendFactor::SrcAlpha => wgpu::BlendFactor::SrcAlpha,
        BlendFactor::OneMinusSrcAlpha => wgpu::BlendFactor::OneMinusSrcAlpha,
        BlendFactor::DstColor => wgpu::BlendFactor::Dst,
        BlendFactor::OneMinusDstColor => wgpu::BlendFactor::OneMinusDst,
    }
}

impl MeshPass {
    pub fn new(
        device: &wgpu::Device,
        texture_layout: &wgpu::BindGroupLayout,
        color_format: wgpu::TextureFormat,
    ) -> Self {
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
        let mut pass = Self {
            shader,
            layout,
            color_format,
            pipelines: HashMap::new(),
            uniform_layout,
        };
        pass.ensure_pipeline(device, MeshStyle::SOLID);
        pass
    }

    fn ensure_pipeline(&mut self, device: &wgpu::Device, style: MeshStyle) -> PipelineKey {
        let key = PipelineKey {
            blend: style.blend,
            depth_write: style.depth_write,
        };
        self.pipelines.entry(key).or_insert_with(|| {
            let color = wgpu::BlendComponent {
                src_factor: blend_factor(key.blend.src),
                dst_factor: blend_factor(key.blend.dst),
                operation: wgpu::BlendOperation::Add,
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("c3 mesh pipeline"),
                layout: Some(&self.layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<MeshVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.color_format,
                        blend: Some(wgpu::BlendState {
                            color,
                            // Keeps the target opaque wherever it was cleared opaque.
                            alpha: wgpu::BlendComponent::OVER,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                // C3 meshes mix windings; draw both faces.
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(key.depth_write),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        });
        key
    }

    /// GPU resources per draw (`None` for empty meshes). `depth(i)` gives draw `i`'s
    /// depth slice `(start, size)`.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        view: Mat4,
        light: crate::Light,
        draws: &[MeshDraw],
        depth: impl Fn(usize) -> (f32, f32),
    ) -> Vec<Option<Prepared>> {
        draws
            .iter()
            .enumerate()
            .map(|(i, d)| {
                if d.vertices.is_empty() || d.indices.is_empty() {
                    return None;
                }
                let pipeline = self.ensure_pipeline(device, d.style);
                let (start, size) = depth(i);
                // Solid meshes take the scene light; see-through effects keep their glow.
                let solid = d.style.depth_write;
                let lit = d.style.lit || (solid && light.shade_models);
                let ambient = if solid { light.ambient } else { [1.0; 3] };
                let uniforms = Uniforms {
                    mvp: (view * d.transform).to_cols_array(),
                    model: d.transform.to_cols_array(),
                    tint: d.tint,
                    depth: [start, size, 0.0, 0.0],
                    shading: [f32::from(u8::from(lit)), 0.0, 0.0, 0.0],
                    ambient: [ambient[0], ambient[1], ambient[2], 1.0],
                    sun: [light.sun[0], light.sun[1], light.sun[2], 0.0],
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
                Some(Prepared {
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
                    pipeline,
                })
            })
            .collect()
    }

    pub fn draw_one(&self, pass: &mut wgpu::RenderPass, prepared: &Prepared, textures: &Textures) {
        pass.set_pipeline(&self.pipelines[&prepared.pipeline]);
        pass.set_bind_group(0, &prepared.uniforms, &[]);
        // Meshes tile their textures, as D3D does by default (UV scrolling relies on it).
        pass.set_bind_group(1, textures.group(prepared.texture, Wrap::Repeat), &[]);
        pass.set_vertex_buffer(0, prepared.vertices.slice(..));
        pass.set_index_buffer(prepared.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..prepared.count, 0, 0..1);
    }
}
