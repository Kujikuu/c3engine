//! Renders offscreen and reads pixels back. Skips when no GPU adapter is available.

use c3_assets::Image;
use c3_render::{Frame, MeshDraw, MeshVertex, RenderTarget, Renderer, Sprite};
use glam::Mat4;

fn renderer() -> Option<Renderer> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).ok()?;
    Some(Renderer::new(device, queue))
}

/// Reads the target back as RGBA8.
fn read_pixels(r: &Renderer, target: &RenderTarget, queue: &wgpu::Queue) -> Vec<u8> {
    let device = r.device();
    let [w, h] = target.size;
    let row = (4 * w).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
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
    queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let data = buffer.slice(..).get_mapped_range().expect("map readback");
    (0..h)
        .flat_map(|y| data[(y * row) as usize..(y * row + 4 * w) as usize].to_vec())
        .collect()
}

#[test]
fn draws_textured_sprite_and_lit_mesh() {
    let Some(mut r) = renderer() else {
        eprintln!("no GPU adapter, skipping");
        return;
    };
    let queue = r.queue().clone();
    let red = r.upload_texture(&Image::new(1, 1, vec![255, 0, 0, 255]));

    // Left half: red sprite. Right half: white triangle mesh.
    let sprites = [Sprite {
        rect: [-1.0, -1.0, 1.0, 2.0],
        uv: [0.0, 0.0, 1.0, 1.0],
        texture: Some(red),
        tint: [1.0; 4],
    }];
    let vertices = [
        MeshVertex {
            position: [0.1, -0.9, 0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [0.0, 0.0],
        },
        MeshVertex {
            position: [0.9, -0.9, 0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [0.0, 0.0],
        },
        MeshVertex {
            position: [0.5, 0.9, 0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [0.0, 0.0],
        },
    ];
    let meshes = [MeshDraw {
        vertices: &vertices,
        indices: &[0, 1, 2],
        texture: None,
        transform: Mat4::IDENTITY,
        tint: [1.0; 4],
        group: None,
    }];

    let mut target = None;
    r.ensure_target(&mut target, [64, 64]);
    let target = target.unwrap();
    r.render(
        &target,
        &Frame {
            clear: [0.0, 0.0, 0.0, 1.0],
            sprite_view: Mat4::IDENTITY,
            sprites: &sprites,
            mesh_view: Mat4::IDENTITY,
            meshes: &meshes,
            order: &[],
        },
    );
    let px = read_pixels(&r, &target, &queue);
    let at = |x: u32, y: u32| &px[((y * 64 + x) * 4) as usize..][..4];
    assert_eq!(at(16, 32), [255, 0, 0, 255], "sprite");
    let mesh = at(48, 40);
    assert!(
        mesh[0] > 100 && mesh[0] == mesh[1] && mesh[1] == mesh[2],
        "lit white mesh, got {mesh:?}"
    );
    assert_eq!(at(62, 2), [0, 0, 0, 255], "clear color");
}

/// A later mesh in painter's order covers an earlier one even when it is farther away.
#[test]
fn painter_order_beats_depth_between_meshes() {
    use c3_render::DrawCmd;
    let Some(mut r) = renderer() else {
        eprintln!("no GPU adapter, skipping");
        return;
    };
    let queue = r.queue().clone();
    let red = r.upload_texture(&Image::new(1, 1, vec![255, 0, 0, 255]));
    let green = r.upload_texture(&Image::new(1, 1, vec![0, 255, 0, 255]));
    let quad = |z: f32| {
        [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]].map(|[x, y]| MeshVertex {
            position: [x, y, z],
            normal: [0.0, 0.0, -1.0],
            uv: [0.0, 0.0],
        })
    };
    let (near, far) = (quad(0.2), quad(0.8));
    let idx = [0u32, 1, 2, 0, 2, 3];
    let mesh = |v, t| MeshDraw {
        vertices: v,
        indices: &idx,
        texture: Some(t),
        transform: Mat4::IDENTITY,
        tint: [1.0; 4],
        group: None,
    };
    let meshes = [mesh(&near, red), mesh(&far, green)];
    let mut target = None;
    r.ensure_target(&mut target, [8, 8]);
    let target = target.unwrap();
    // Far green mesh is drawn last, so it must cover the near red one.
    let order = [DrawCmd::Mesh(0), DrawCmd::Mesh(1)];
    r.render(
        &target,
        &Frame {
            clear: [0.0; 4],
            meshes: &meshes,
            order: &order,
            ..Default::default()
        },
    );
    let px = read_pixels(&r, &target, &queue);
    let c = &px[(4 * 8 + 4) * 4..][..4];
    assert!(
        c[1] > 200 && c[0] == 0,
        "expected the lit green mesh in front, got {c:?}"
    );
}

/// Meshes of one group (e.g. a body and its weapon) occlude each other by real depth.
#[test]
fn same_group_meshes_use_real_depth() {
    use c3_render::DrawCmd;
    let Some(mut r) = renderer() else {
        eprintln!("no GPU adapter, skipping");
        return;
    };
    let queue = r.queue().clone();
    let red = r.upload_texture(&Image::new(1, 1, vec![255, 0, 0, 255]));
    let green = r.upload_texture(&Image::new(1, 1, vec![0, 255, 0, 255]));
    let quad = |z: f32| {
        [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]].map(|[x, y]| MeshVertex {
            position: [x, y, z],
            normal: [0.0, 0.0, -1.0],
            uv: [0.0, 0.0],
        })
    };
    let (near, far) = (quad(0.2), quad(0.8));
    let idx = [0u32, 1, 2, 0, 2, 3];
    let mesh = |v, t| MeshDraw {
        vertices: v,
        indices: &idx,
        texture: Some(t),
        transform: Mat4::IDENTITY,
        tint: [1.0; 4],
        group: Some(7),
    };
    let meshes = [mesh(&near, red), mesh(&far, green)];
    let mut target = None;
    r.ensure_target(&mut target, [8, 8]);
    let target = target.unwrap();
    // Far green is drawn last but shares the group, so the near red one stays in front.
    let order = [DrawCmd::Mesh(0), DrawCmd::Mesh(1)];
    r.render(
        &target,
        &Frame {
            clear: [0.0; 4],
            meshes: &meshes,
            order: &order,
            ..Default::default()
        },
    );
    let px = read_pixels(&r, &target, &queue);
    let c = &px[(4 * 8 + 4) * 4..][..4];
    assert!(
        c[0] > 200 && c[1] == 0,
        "expected the near red mesh in front, got {c:?}"
    );
}
