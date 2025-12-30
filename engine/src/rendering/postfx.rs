use std::sync::Arc;

use crate::rendering::{
    common::FullscreenVertexShader,
    passes::postfx::{fxaa::FXAAPass, noise::NoisePass},
    postfx_constants::PostFxConstantsBuffer,
    resolution::Resolution,
    texture::Texture,
};

pub struct PostFxRenderer {
    device: wgpu::Device,
    pub fxaa_pass: FXAAPass,
    pub noise_pass: NoisePass,
    pub intermediate_texture: Texture,
    constants_buffer: Arc<PostFxConstantsBuffer>,
}

impl PostFxRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture_format: wgpu::TextureFormat,
        input_view: &wgpu::TextureView,
        size: Resolution,
    ) -> Self {
        let constants_buffer = PostFxConstantsBuffer::new(device, queue);
        let constants_buffer = Arc::new(constants_buffer);

        let fullscreen_vertex_shader = FullscreenVertexShader::new(device);

        let intermediate_texture = Texture::from_descriptor(
            device,
            wgpu::TextureDescriptor {
                label: Some("PostFX intermediate texture"),
                size: wgpu::Extent3d {
                    width: size.width,
                    height: size.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: texture_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            None,
        );

        let fxaa_pass = FXAAPass::new(
            device,
            &fullscreen_vertex_shader,
            texture_format,
            input_view,
            constants_buffer.clone(),
        );
        let noise_pass = NoisePass::new(
            device,
            &fullscreen_vertex_shader,
            texture_format,
            &intermediate_texture.view,
            constants_buffer.clone(),
        );

        PostFxRenderer {
            device: device.clone(),
            fxaa_pass,
            noise_pass,
            intermediate_texture,
            constants_buffer,
        }
    }

    pub fn update(&mut self, time: f32) {
        self.constants_buffer.update(time);
    }

    pub fn render(&self, encoder: &mut wgpu::CommandEncoder, output_view: &wgpu::TextureView) {
        self.fxaa_pass
            .render(encoder, &self.intermediate_texture.view);
        self.noise_pass.render(encoder, output_view);
    }

    pub fn resize(&mut self, size: Resolution, input_view: &wgpu::TextureView) {
        self.intermediate_texture.resize(&self.device, size);
        self.fxaa_pass.update_bind_group(input_view);
        self.noise_pass
            .update_bind_group(&self.intermediate_texture.view);
    }
}
