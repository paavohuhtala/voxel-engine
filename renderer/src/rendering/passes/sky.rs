use wgpu::{
    LoadOpDontCare, MultisampleState, RenderPassDescriptor, ShaderModuleDescriptor, ShaderStages,
};

use crate::rendering::{
    common::FULLSCREEN_TRIANGLE_PRIMITIVE_STATE, memory::typed_buffer::GpuBuffer,
    render_camera::CameraUniform, util::bind_group_builder::BindGroupBuilder,
};

pub struct SkyPass {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
}

impl SkyPass {
    pub fn new(device: &wgpu::Device, camera_uniform_buffer: &GpuBuffer<CameraUniform>) -> Self {
        let source = include_str!(concat!(env!("OUT_DIR"), "/sky.wgsl"));
        let module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Sky shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });

        let (bind_group_layout, bind_group) =
            BindGroupBuilder::new("sky", ShaderStages::VERTEX | ShaderStages::FRAGMENT)
                .uniform(
                    0,
                    "Camera uniform buffer",
                    wgpu::BindingResource::Buffer(
                        camera_uniform_buffer.inner().as_entire_buffer_binding(),
                    ),
                )
                .build(device);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sky Pass Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            ..Default::default()
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sky Pass Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Bgra8UnormSrgb,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: FULLSCREEN_TRIANGLE_PRIMITIVE_STATE,
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        SkyPass {
            pipeline,
            bind_group,
        }
    }

    pub fn render(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Sky pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Sky overwrites every pixel, so we can use DontCare load operation
                    load: wgpu::LoadOp::DontCare(unsafe { LoadOpDontCare::enabled() }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            multiview_mask: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}
