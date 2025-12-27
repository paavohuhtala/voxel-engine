use std::sync::Arc;

use wgpu::ShaderModuleDescriptor;

use crate::rendering::{
    common::{FULLSCREEN_TRIANGLE_PRIMITIVE_STATE, FullscreenVertexShader},
    postfx_constants::PostFxConstantsBuffer,
    util::bind_group_builder::BindGroupBuilder,
};

pub struct PostFxPass {
    device: wgpu::Device,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    pass_label: String,
    post_fx_constants: Arc<PostFxConstantsBuffer>,
}

impl PostFxPass {
    pub fn new(
        label: &'static str,
        device: &wgpu::Device,
        fullscreen_shader: &FullscreenVertexShader,
        fragment_shader_source: wgpu::ShaderSource,
        texture_format: wgpu::TextureFormat,
        input_view: &wgpu::TextureView,
        post_fx_constants: Arc<PostFxConstantsBuffer>,
    ) -> Self {
        let module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some(format!("{} shader", label).as_str()),
            source: fragment_shader_source,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(format!("{} sampler", label).as_str()),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let (bind_group_layout, bind_group) =
            Self::create_bind_group(device, input_view, &sampler, &post_fx_constants);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(format!("{} pipeline layout", label).as_str()),
            bind_group_layouts: &[&bind_group_layout],
            ..Default::default()
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(format!("{} pipeline", label).as_str()),
            layout: Some(&pipeline_layout),
            vertex: fullscreen_shader.vertex_state(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: texture_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: FULLSCREEN_TRIANGLE_PRIMITIVE_STATE,
            cache: None,
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
        });

        PostFxPass {
            device: device.clone(),
            pipeline,
            bind_group,
            sampler,
            pass_label: format!("{} pass", label),
            post_fx_constants,
        }
    }

    fn create_bind_group(
        device: &wgpu::Device,
        input_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        post_fx_constants: &PostFxConstantsBuffer,
    ) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
        let (bind_group_layout, bind_group) =
            BindGroupBuilder::new("fxaa", wgpu::ShaderStages::FRAGMENT)
                .texture(
                    0,
                    "Screen texture",
                    wgpu::BindingResource::TextureView(input_view),
                    wgpu::TextureSampleType::Float { filterable: true },
                )
                .sampler(
                    1,
                    "Sampler",
                    wgpu::BindingResource::Sampler(&sampler),
                    wgpu::SamplerBindingType::Filtering,
                )
                .uniform(
                    2,
                    "Post FX constants",
                    wgpu::BindingResource::Buffer(
                        post_fx_constants.buffer().as_entire_buffer_binding(),
                    ),
                )
                .build(device);

        (bind_group_layout, bind_group)
    }

    pub fn update_bind_group(&mut self, input_view: &wgpu::TextureView) {
        let (_, bind_group) = Self::create_bind_group(
            &self.device,
            input_view,
            &self.sampler,
            &self.post_fx_constants,
        );
        self.bind_group = bind_group;
    }

    pub fn render(&self, encoder: &mut wgpu::CommandEncoder, output_view: &wgpu::TextureView) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(&self.pass_label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}
