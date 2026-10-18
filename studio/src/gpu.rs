//! Bridges the engine renderer and egui: renders a `c3_render::Frame` offscreen and
//! exposes the result as an egui texture.

use c3_render::{Frame, RenderTarget, Renderer};
use eframe::{egui, egui_wgpu, wgpu};

pub struct GpuView {
    state: egui_wgpu::RenderState,
    pub renderer: Renderer,
    target: Option<RenderTarget>,
    texture: Option<egui::TextureId>,
}

impl GpuView {
    pub fn new(state: &egui_wgpu::RenderState) -> Self {
        Self {
            state: state.clone(),
            renderer: Renderer::new(state.device.clone(), state.queue.clone()),
            target: None,
            texture: None,
        }
    }

    /// Renders `frame` at `size` physical pixels and returns the texture to paint.
    pub fn render(&mut self, size: [u32; 2], frame: &Frame) -> egui::TextureId {
        let resized = self
            .target
            .as_ref()
            .is_none_or(|t| t.size != [size[0].max(1), size[1].max(1)]);
        self.renderer.ensure_target(&mut self.target, size);
        let target = self.target.as_ref().expect("target ensured");
        self.renderer.render(target, frame);

        let mut egui_renderer = self.state.renderer.write();
        let filter = wgpu::FilterMode::Linear;
        match self.texture {
            Some(id) if resized => egui_renderer.update_egui_texture_from_wgpu_texture(
                &self.state.device,
                &target.display_view,
                filter,
                id,
            ),
            Some(_) => {}
            None => {
                self.texture = Some(egui_renderer.register_native_texture(
                    &self.state.device,
                    &target.display_view,
                    filter,
                ))
            }
        }
        self.texture.expect("texture registered")
    }
}
