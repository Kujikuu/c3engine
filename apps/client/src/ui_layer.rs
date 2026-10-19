//! egui over the game window: input from winit, drawing with egui-wgpu on top of the frame
//! the engine rendered.

use std::sync::Arc;
use winit::window::Window;

/// One frame of UI, tessellated and waiting to be painted.
struct Painted {
    primitives: Vec<egui::ClippedPrimitive>,
    textures: egui::TexturesDelta,
    pixels_per_point: f32,
}

pub struct UiLayer {
    pub ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    painted: Option<Painted>,
}

impl UiLayer {
    pub fn new(window: &Arc<Window>, device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let ctx = egui::Context::default();
        c3_ui::fonts::install(&ctx, Vec::new());
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            None,
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        let renderer = egui_wgpu::Renderer::new(device, format, Default::default());
        Self {
            ctx,
            state,
            renderer,
            painted: None,
        }
    }

    /// `view` (a picture the engine drew) as a texture egui can show: `id` again when given.
    pub fn native_texture(
        &mut self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        id: Option<egui::TextureId>,
    ) -> egui::TextureId {
        let filter = wgpu::FilterMode::Linear;
        match id {
            Some(id) => {
                self.renderer
                    .update_egui_texture_from_wgpu_texture(device, view, filter, id);
                id
            }
            None => self.renderer.register_native_texture(device, view, filter),
        }
    }

    /// Passes a window event to egui.
    pub fn on_event(&mut self, window: &Window, event: &winit::event::WindowEvent) {
        let _ = self.state.on_window_event(window, event);
    }

    /// The input gathered for the next frame.
    pub fn take_input(&mut self, window: &Window) -> egui::RawInput {
        self.state.take_egui_input(window)
    }

    /// Runs one UI frame of `input` with `ui` on the root (whole-window) Ui.
    pub fn run(&mut self, window: &Window, input: egui::RawInput, ui: impl FnMut(&mut egui::Ui)) {
        let output = self.ctx.run_ui(input, ui);
        self.state
            .handle_platform_output(window, output.platform_output);
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        self.painted = Some(Painted {
            primitives,
            textures: output.textures_delta,
            pixels_per_point: output.pixels_per_point,
        });
    }

    /// Paints the last UI frame over `view` (`size` in pixels).
    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        size: [u32; 2],
    ) {
        let Some(painted) = self.painted.take() else {
            return;
        };
        for (id, deltas) in &painted.textures.set {
            for delta in deltas {
                self.renderer.update_texture(device, queue, *id, delta);
            }
        }
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: size,
            pixels_per_point: painted.pixels_per_point,
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        let commands =
            self.renderer
                .update_buffers(device, queue, &mut encoder, &painted.primitives, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("game ui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            // Over the frame the engine drew.
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            self.renderer
                .render(&mut pass, &painted.primitives, &screen);
        }
        queue.submit(commands.into_iter().chain([encoder.finish()]));
        for id in &painted.textures.free {
            self.renderer.free_texture(id);
        }
    }
}
