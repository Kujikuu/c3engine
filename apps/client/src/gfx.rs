//! Window surface + engine renderer.

use c3_render::{DepthBuffer, Frame, Renderer};
use std::sync::Arc;
use winit::window::Window;

pub struct Gfx {
    pub window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    depth: DepthBuffer,
    pub renderer: Renderer,
}

impl Gfx {
    pub fn new(window: Arc<Window>) -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| e.to_string())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
            .map_err(|e| e.to_string())?;

        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("surface not supported by adapter")?;
        // Prefer an sRGB format so textures (sRGB) display correctly.
        let caps = surface.get_capabilities(&adapter);
        if let Some(&srgb) = caps.formats.iter().find(|f| f.is_srgb()) {
            config.format = srgb;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        let depth = DepthBuffer::new(&device, [config.width, config.height]);
        let renderer = Renderer::with_format(device, queue, config.format);
        Ok(Self {
            window,
            surface,
            config,
            depth,
            renderer,
        })
    }

    pub fn size(&self) -> [u32; 2] {
        [self.config.width, self.config.height]
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(self.renderer.device(), &self.config);
        self.depth.ensure(self.renderer.device(), [width, height]);
    }

    /// Renders and presents one frame. Reconfigures the surface if it was lost.
    pub fn draw(&mut self, frame: &Frame) {
        use wgpu::CurrentSurfaceTexture as Current;
        let texture = match self.surface.get_current_texture() {
            Current::Success(t) => t,
            // Draw this frame, then reconfigure for the next one.
            Current::Suboptimal(t) => {
                self.surface.configure(self.renderer.device(), &self.config);
                t
            }
            Current::Outdated | Current::Lost => {
                self.surface.configure(self.renderer.device(), &self.config);
                return;
            }
            // Minimized / busy: skip the frame.
            _ => return,
        };
        let view = texture.texture.create_view(&Default::default());
        self.renderer.render_into(&view, &self.depth, frame);
        self.window.pre_present_notify();
        self.renderer.queue().present(texture);
    }
}
