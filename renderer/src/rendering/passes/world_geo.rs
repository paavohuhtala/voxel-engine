use std::mem::size_of;

use wgpu::wgt::DrawIndexedIndirectArgs;
use wgpu::{
    BindGroup, CompareFunction, ComputePipeline, DepthStencilState, PrimitiveState, RenderPipeline,
    RenderPipelineDescriptor, ShaderModuleDescriptor, ShaderStages, VertexState,
};

use crate::rendering::limits::MAX_GPU_CHUNKS;
use crate::rendering::{
    memory::typed_buffer::{GpuBuffer, GpuBufferArray},
    texture::DepthTexture,
    texture_manager::TextureManager,
    util::bind_group_builder::BindGroupBuilder,
    world_renderer::{CullingParams, WorldBuffers},
};

pub struct WorldGeometryPass {
    camera_bind_group: BindGroup,
    chunks_bind_group: BindGroup,
    culling_bind_group: BindGroup,
    textures_bind_group: BindGroup,
    culling_pipeline: ComputePipeline,
    draw_pipeline: RenderPipeline,

    quad_indices: GpuBuffer<[u16; 6]>,

    culling_params: GpuBuffer<CullingParams>,
    input_chunk_ids: GpuBufferArray<u32>,
    draw_commands: GpuBufferArray<DrawIndexedIndirectArgs>,
}

impl WorldGeometryPass {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        buffers: &WorldBuffers,
        texture_manager: &TextureManager,
    ) -> Self {
        let (camera_bind_group_layout, camera_bind_group) =
            BindGroupBuilder::new("camera", ShaderStages::VERTEX | ShaderStages::COMPUTE)
                .uniform(
                    0,
                    "Camera uniform buffer",
                    wgpu::BindingResource::Buffer(
                        buffers.camera.inner().as_entire_buffer_binding(),
                    ),
                )
                .build(device);

        let (chunks_bind_group_layout, chunks_bind_group) =
            BindGroupBuilder::new("chunks", ShaderStages::VERTEX | ShaderStages::COMPUTE)
                .storage_r(
                    0,
                    "Chunks buffer",
                    wgpu::BindingResource::Buffer(
                        buffers.chunks.buffer().as_entire_buffer_binding(),
                    ),
                )
                .storage_r(
                    1,
                    "Chunk face data buffer",
                    wgpu::BindingResource::Buffer(
                        buffers.faces.buffer().as_entire_buffer_binding(),
                    ),
                )
                .build(device);

        let culling_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Culling params buffer"),
            size: size_of::<CullingParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let input_chunk_ids_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Input chunk IDs buffer"),
            size: (MAX_GPU_CHUNKS * size_of::<u32>() as u64),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let draw_commands_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Draw commands buffer"),
            size: (MAX_GPU_CHUNKS * size_of::<DrawIndexedIndirectArgs>() as u64),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
            mapped_at_creation: false,
        });

        let (culling_bind_group_layout, culling_bind_group) =
            BindGroupBuilder::new("culling", ShaderStages::COMPUTE)
                .uniform(
                    0,
                    "Culling params buffer",
                    wgpu::BindingResource::Buffer(culling_params_buffer.as_entire_buffer_binding()),
                )
                .storage_r(
                    1,
                    "Input chunk IDs buffer",
                    wgpu::BindingResource::Buffer(
                        input_chunk_ids_buffer.as_entire_buffer_binding(),
                    ),
                )
                .storage_rw(
                    2,
                    "Draw commands buffer",
                    wgpu::BindingResource::Buffer(draw_commands_buffer.as_entire_buffer_binding()),
                )
                .build(device);

        let (textures_bind_group_layout, textures_bind_group) =
            BindGroupBuilder::new("textures", ShaderStages::FRAGMENT)
                .array_texture(
                    0,
                    "World texture array",
                    wgpu::BindingResource::TextureView(texture_manager.array_texture_view()),
                    wgpu::TextureSampleType::Float { filterable: true },
                )
                .sampler(
                    1,
                    "World texture sampler",
                    wgpu::BindingResource::Sampler(texture_manager.sampler()),
                    wgpu::SamplerBindingType::Filtering,
                )
                .build(device);

        let culling_pipeline = create_draw_command_pipeline(
            device,
            &camera_bind_group_layout,
            &chunks_bind_group_layout,
            &culling_bind_group_layout,
        );
        let draw_pipeline = create_draw_pipeline(
            device,
            &camera_bind_group_layout,
            &chunks_bind_group_layout,
            &textures_bind_group_layout,
        );

        let quad_indices = GpuBuffer::from_data(
            device,
            queue,
            "Quad indices buffer",
            wgpu::BufferUsages::INDEX,
            &[0, 3, 1, 3, 2, 1],
        );

        Self {
            culling_pipeline,
            draw_pipeline,
            camera_bind_group,
            chunks_bind_group,
            culling_bind_group,
            textures_bind_group,

            quad_indices,

            culling_params: GpuBuffer::from_buffer(queue, culling_params_buffer),
            input_chunk_ids: GpuBufferArray::from_buffer(queue, input_chunk_ids_buffer),
            draw_commands: GpuBufferArray::from_buffer(queue, draw_commands_buffer),
        }
    }

    #[profiling::function]
    pub fn cull_chunks(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        culling_params: &CullingParams,
        chunk_ids: &[u32],
    ) {
        self.culling_params.write_data(culling_params);
        self.input_chunk_ids.write_data(chunk_ids);

        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("World geometry culling pass"),
            timestamp_writes: None,
        });
        compute_pass.set_pipeline(&self.culling_pipeline);
        compute_pass.set_bind_group(0, &self.camera_bind_group, &[]);
        compute_pass.set_bind_group(1, &self.chunks_bind_group, &[]);
        compute_pass.set_bind_group(2, &self.culling_bind_group, &[]);

        let workgroup_size = 64;
        let workgroup_count = culling_params.input_chunk_count.div_ceil(workgroup_size);
        compute_pass.dispatch_workgroups(workgroup_count, 1, 1);
    }

    #[profiling::function]
    pub fn draw_chunks(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_texture: &DepthTexture,
        max_draw_count: u32,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("World geometry render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_texture.view(),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });

        render_pass.set_pipeline(&self.draw_pipeline);
        render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
        render_pass.set_bind_group(1, &self.chunks_bind_group, &[]);
        render_pass.set_bind_group(2, &self.textures_bind_group, &[]);
        render_pass.set_index_buffer(
            self.quad_indices.inner().slice(..),
            wgpu::IndexFormat::Uint16,
        );

        render_pass.multi_draw_indexed_indirect(self.draw_commands.inner(), 0, max_draw_count);
    }
}

fn create_draw_command_pipeline(
    device: &wgpu::Device,
    camera_bind_group_layout: &wgpu::BindGroupLayout,
    chunks_bind_group_layout: &wgpu::BindGroupLayout,
    culling_bind_group_layout: &wgpu::BindGroupLayout,
) -> ComputePipeline {
    let source = include_str!(concat!(
        env!("OUT_DIR"),
        "/world_geo_generate_commands.wgsl"
    ));
    let module = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("World geometry command generation shader"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("World geometry command generation pipeline layout"),
        bind_group_layouts: &[
            camera_bind_group_layout,
            chunks_bind_group_layout,
            culling_bind_group_layout,
        ],
        ..Default::default()
    });

    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("World geometry command generation pipeline"),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("main"),
        cache: None,
        compilation_options: Default::default(),
    })
}

fn create_draw_pipeline(
    device: &wgpu::Device,
    camera_bind_group_layout: &wgpu::BindGroupLayout,
    chunks_bind_group_layout: &wgpu::BindGroupLayout,
    textures_bind_group_layout: &wgpu::BindGroupLayout,
) -> RenderPipeline {
    let source = include_str!(concat!(env!("OUT_DIR"), "/world_geo_draw.wgsl"));
    let module = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("World geometry shader"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });

    let draw_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("World geometry pipeline layout"),
        bind_group_layouts: &[
            camera_bind_group_layout,
            chunks_bind_group_layout,
            textures_bind_group_layout,
        ],
        ..Default::default()
    });

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("World geometry pipeline"),
        layout: Some(&draw_pipeline_layout),
        vertex: VertexState {
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
        primitive: PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Cw,
            cull_mode: Some(wgpu::Face::Back),
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(DepthStencilState {
            format: DepthTexture::DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: CompareFunction::GreaterEqual,
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}
