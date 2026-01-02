use std::sync::Arc;

use engine::{assets::blocks::BlockDatabase, game_loop::GameLoopTime};
use wgpu::{RenderPassDescriptor, wgt::CommandEncoderDescriptor};
use winit::window::Window;

use crate::rendering::{
    resolution::Resolution, texture::DepthTexture, world_renderer::WorldRenderer,
};

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth_texture: DepthTexture,
    is_surface_configured: bool,
    window: Arc<Window>,
    pub world_renderer: WorldRenderer,
}

impl Renderer {
    pub async fn new(
        window: Arc<Window>,
        block_database: Arc<BlockDatabase>,
    ) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let window_clone = window.clone();
        let surface = instance.create_surface(window_clone)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::INDIRECT_FIRST_INSTANCE
                    | wgpu::Features::TEXTURE_BINDING_ARRAY
                    | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                required_limits: wgpu::Limits {
                    max_binding_array_elements_per_shader_stage: u16::MAX as u32,
                    ..Default::default()
                },
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
            })
            .await?;

        let surface_capabilities = surface.get_capabilities(&adapter);
        let surface_format = surface_capabilities
            .formats
            .iter()
            .find(|format| format.is_srgb())
            .copied()
            .unwrap_or(surface_capabilities.formats[0]);

        let present_mode = {
            #[cfg(feature = "superluminal")]
            {
                wgpu::PresentMode::AutoNoVsync
            }
            #[cfg(not(feature = "superluminal"))]
            {
                wgpu::PresentMode::AutoVsync
            }
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode,
            alpha_mode: surface_capabilities.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        let depth_texture = DepthTexture::new(
            &device,
            Resolution {
                width: size.width,
                height: size.height,
            },
            "Depth texture",
        );

        let world_renderer = WorldRenderer::new(&device, &queue, size, block_database.clone());

        Ok(Renderer {
            surface,
            device,
            queue,
            config,
            depth_texture,
            is_surface_configured: false,
            window,
            world_renderer,
        })
    }

    pub fn resize(&mut self, size: Resolution) {
        if size.width > 0 && size.height > 0 {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.device, &self.config);
            self.depth_texture.resize(&self.device, size);
            self.is_surface_configured = true;
            self.world_renderer.resize(size);
        } else {
            self.is_surface_configured = false;
        }
    }

    pub fn update(&mut self, time: &GameLoopTime) {
        self.world_renderer.update(time);
    }

    pub fn render(&mut self, time: &GameLoopTime) -> anyhow::Result<()> {
        self.window.request_redraw();

        if !self.is_surface_configured {
            return Ok(());
        }

        let output = match self.surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Lost) => {
                self.resize(Resolution {
                    width: self.config.width,
                    height: self.config.height,
                });
                return Ok(());
            }
            Err(wgpu::SurfaceError::Outdated) => return Ok(()),
            Err(error) => return Err(error.into()),
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor::default());

        {
            let _render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Default render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: self.depth_texture.view(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                multiview_mask: None,
                timestamp_writes: None,
            });
        }

        {
            self.world_renderer
                .render(&mut encoder, &view, &self.depth_texture, time);
        }

        self.queue.submit([encoder.finish()]);
        output.present();
        profiling::finish_frame!();

        Ok(())
    }

    pub fn is_minimized(&self) -> bool {
        self.window.is_minimized().unwrap_or(false)
    }
}
