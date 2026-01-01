use std::sync::Arc;

use crate::rendering::{
    common::FullscreenVertexShader, passes::postfx::postfx_pass::PostFxPass,
    postfx_constants::PostFxConstantsBuffer,
};

pub struct NoisePass(PostFxPass);

impl NoisePass {
    pub fn new(
        device: &wgpu::Device,
        fullscreen_shader: &FullscreenVertexShader,
        texture_format: wgpu::TextureFormat,
        input_view: &wgpu::TextureView,
        post_fx_constants: Arc<PostFxConstantsBuffer>,
    ) -> Self {
        let source = include_str!(concat!(env!("OUT_DIR"), "/postfx_noise.wgsl"));
        let post_fx_pass = PostFxPass::new(
            "Noise",
            device,
            fullscreen_shader,
            wgpu::ShaderSource::Wgsl(source.into()),
            texture_format,
            input_view,
            post_fx_constants,
        );
        NoisePass(post_fx_pass)
    }

    pub fn render(&self, encoder: &mut wgpu::CommandEncoder, output_view: &wgpu::TextureView) {
        self.0.render(encoder, output_view);
    }

    pub fn update_bind_group(&mut self, input_view: &wgpu::TextureView) {
        self.0.update_bind_group(input_view);
    }
}
