// Based on https://github.com/kaphula/winit-egui-wgpu-template/blob/5b5ffc9eaf561e688f7864a9f79c8c33da7487f5/src/egui_tools.rs

use std::sync::Arc;

use egui::{Context, Shadow, ViewportId};
use egui_wgpu::{
    Renderer, RendererOptions, ScreenDescriptor,
    wgpu::{self, CommandEncoder, Device, Queue, TextureView},
};
use egui_winit::{EventResponse, State};
use winit::{event::WindowEvent, window::Window};

pub struct EguiInstance {
    device: Device,
    queue: Queue,
    window: Arc<Window>,
    state: State,
    renderer: Renderer,
    frame_started: bool,
}

impl EguiInstance {
    pub fn new(window: Arc<Window>, device: &Device, queue: &Queue) -> Self {
        let egui = Context::default();
        egui.set_visuals({
            let mut visuals = egui::Visuals::dark();
            visuals.window_shadow = Shadow::NONE;
            visuals
        });

        let state = State::new(
            egui,
            ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            Some(2 * 1024),
        );
        let renderer = Renderer::new(
            device,
            egui_wgpu::wgpu::TextureFormat::Bgra8UnormSrgb,
            RendererOptions::default(),
        );

        EguiInstance {
            device: device.clone(),
            queue: queue.clone(),
            window,
            state,
            renderer,
            frame_started: false,
        }
    }

    pub fn handle_input(&mut self, event: &WindowEvent) -> EventResponse {
        self.state.on_window_event(&self.window, event)
    }

    pub fn begin_frame(&mut self) {
        let input = self.state.take_egui_input(&self.window);
        self.state.egui_ctx().begin_pass(input);
        self.frame_started = true;
    }

    pub fn ctx(&self) -> &Context {
        self.state.egui_ctx()
    }

    pub fn end_frame_and_draw(
        &mut self,
        encoder: &mut CommandEncoder,
        target: &TextureView,
        screen_descriptor: ScreenDescriptor,
    ) {
        let window = &self.window;

        if !self.frame_started {
            panic!("begin_frame must be called before end_frame_and_draw");
        }

        // Sync pixel per point, in case the window was moved to another monitor
        self.ctx()
            .set_pixels_per_point(screen_descriptor.pixels_per_point);

        let full_output = self.ctx().end_pass();

        self.state
            .handle_platform_output(window, full_output.platform_output);

        let tris = self
            .state
            .egui_ctx()
            .tessellate(full_output.shapes, self.ctx().pixels_per_point());

        for (id, image_delta) in &full_output.textures_delta.set {
            self.renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }

        self.renderer.update_buffers(
            &self.device,
            &self.queue,
            encoder,
            &tris,
            &screen_descriptor,
        );

        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Egui render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            ..Default::default()
        });

        self.renderer
            .render(&mut pass.forget_lifetime(), &tris, &screen_descriptor);

        for x in &full_output.textures_delta.free {
            self.renderer.free_texture(x);
        }

        self.frame_started = false;
    }
}
