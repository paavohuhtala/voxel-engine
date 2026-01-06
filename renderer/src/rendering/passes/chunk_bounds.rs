use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Vec3};
use wgpu::{
    CompareFunction, DepthStencilState, MultisampleState, PrimitiveState, RenderPipeline,
    ShaderModuleDescriptor, ShaderStages,
};

use crate::rendering::{
    memory::typed_buffer::GpuBuffer, render_camera::CameraUniform, texture::DepthTexture,
    util::bind_group_builder::BindGroupBuilder,
};

use engine::math::aabb::AABB8;

/// Vertex data for a single point in a chunk wireframe
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct ChunkBoundsVertex {
    /// Local position within the chunk (0-16 range)
    pub position: Vec3,
    /// Chunk position in chunk coordinates
    pub chunk_position: IVec3,
}

/// The 8 corners of a unit chunk (0-16 range)
const CUBE_CORNERS: [Vec3; 8] = [
    Vec3::new(0.0, 0.0, 0.0),    // 0: front-bottom-left
    Vec3::new(16.0, 0.0, 0.0),   // 1: front-bottom-right
    Vec3::new(16.0, 16.0, 0.0),  // 2: front-top-right
    Vec3::new(0.0, 16.0, 0.0),   // 3: front-top-left
    Vec3::new(0.0, 0.0, 16.0),   // 4: back-bottom-left
    Vec3::new(16.0, 0.0, 16.0),  // 5: back-bottom-right
    Vec3::new(16.0, 16.0, 16.0), // 6: back-top-right
    Vec3::new(0.0, 16.0, 16.0),  // 7: back-top-left
];

fn aabb_corners(min: Vec3, max: Vec3) -> [Vec3; 8] {
    [
        Vec3::new(min.x, min.y, min.z), // 0: front-bottom-left
        Vec3::new(max.x, min.y, min.z), // 1: front-bottom-right
        Vec3::new(max.x, max.y, min.z), // 2: front-top-right
        Vec3::new(min.x, max.y, min.z), // 3: front-top-left
        Vec3::new(min.x, min.y, max.z), // 4: back-bottom-left
        Vec3::new(max.x, min.y, max.z), // 5: back-bottom-right
        Vec3::new(max.x, max.y, max.z), // 6: back-top-right
        Vec3::new(min.x, max.y, max.z), // 7: back-top-left
    ]
}

/// Line indices for the 12 edges of a cube
const CUBE_LINE_INDICES: [(usize, usize); 12] = [
    // Front face
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 0),
    // Back face
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 4),
    // Connecting edges
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

/// Generate vertex data for a chunk wireframe
pub fn generate_chunk_wireframe_vertices(chunk_pos: IVec3) -> Vec<ChunkBoundsVertex> {
    let mut vertices = Vec::with_capacity(24);

    for (start, end) in CUBE_LINE_INDICES.iter() {
        vertices.push(ChunkBoundsVertex {
            position: CUBE_CORNERS[*start],
            chunk_position: chunk_pos,
        });
        vertices.push(ChunkBoundsVertex {
            position: CUBE_CORNERS[*end],
            chunk_position: chunk_pos,
        });
    }

    vertices
}

pub fn generate_aabb_wireframe_vertices(
    chunk_pos: IVec3,
    aabb_min: Vec3,
    aabb_max: Vec3,
) -> Vec<ChunkBoundsVertex> {
    let mut vertices = Vec::with_capacity(24);
    let corners = aabb_corners(aabb_min, aabb_max);

    for (start, end) in CUBE_LINE_INDICES.iter() {
        vertices.push(ChunkBoundsVertex {
            position: corners[*start],
            chunk_position: chunk_pos,
        });
        vertices.push(ChunkBoundsVertex {
            position: corners[*end],
            chunk_position: chunk_pos,
        });
    }

    vertices
}

pub struct ChunkBoundsPass {
    pipeline: RenderPipeline,
    bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
    max_chunks: usize,
}

impl ChunkBoundsPass {
    /// Maximum number of chunks that can be visualized at once
    const MAX_CHUNKS: usize = 4096;
    /// Vertices per chunk (24 = 12 edges * 2 vertices per line)
    const VERTICES_PER_CHUNK: usize = 24;

    pub fn new(device: &wgpu::Device, camera_uniform_buffer: &GpuBuffer<CameraUniform>) -> Self {
        let source = include_str!(concat!(env!("OUT_DIR"), "/chunk_bounds.wgsl"));
        let module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Chunk bounds shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });

        let (bind_group_layout, bind_group) =
            BindGroupBuilder::new("chunk_bounds", ShaderStages::VERTEX)
                .uniform(
                    0,
                    "Camera uniform buffer",
                    wgpu::BindingResource::Buffer(
                        camera_uniform_buffer.inner().as_entire_buffer_binding(),
                    ),
                )
                .build(device);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Chunk Bounds Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            ..Default::default()
        });

        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ChunkBoundsVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position: vec3<f32>
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // chunk_position: vec3<i32>
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<Vec3>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Sint32x3,
                },
            ],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Chunk Bounds Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[vertex_buffer_layout],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Bgra8UnormSrgb,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: DepthTexture::DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: CompareFunction::GreaterEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // Create vertex buffer with capacity for max chunks
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Chunk bounds vertex buffer"),
            size: (Self::MAX_CHUNKS
                * Self::VERTICES_PER_CHUNK
                * std::mem::size_of::<ChunkBoundsVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        ChunkBoundsPass {
            pipeline,
            bind_group,
            vertex_buffer,
            vertex_count: 0,
            max_chunks: Self::MAX_CHUNKS,
        }
    }

    /// Update the vertex buffer with wireframes for the given chunk positions
    pub fn update_chunks(&mut self, queue: &wgpu::Queue, chunk_positions: &[IVec3]) {
        let chunk_count = chunk_positions.len().min(self.max_chunks);

        if chunk_count == 0 {
            self.vertex_count = 0;
            return;
        }

        let mut vertices = Vec::with_capacity(chunk_count * Self::VERTICES_PER_CHUNK);
        for &pos in chunk_positions.iter().take(chunk_count) {
            vertices.extend(generate_chunk_wireframe_vertices(pos));
        }

        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        self.vertex_count = vertices.len() as u32;
    }

    /// Update the vertex buffer with wireframes for the given per-chunk AABBs.
    /// AABBs are chunk-local voxel coordinates; max is inclusive, so we expand by +1.
    pub fn update_chunk_aabbs(&mut self, queue: &wgpu::Queue, chunks: &[(IVec3, AABB8)]) {
        let chunk_count = chunks.len().min(self.max_chunks);

        if chunk_count == 0 {
            self.vertex_count = 0;
            return;
        }

        let mut vertices = Vec::with_capacity(chunk_count * Self::VERTICES_PER_CHUNK);
        for (pos, aabb8) in chunks.iter().take(chunk_count) {
            let min = aabb8.min.as_vec3();
            let mut max = aabb8.max.as_vec3() + Vec3::ONE;
            // Clamp in case of unexpected values.
            max = max.min(Vec3::splat(16.0));
            vertices.extend(generate_aabb_wireframe_vertices(*pos, min, max));
        }

        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        self.vertex_count = vertices.len() as u32;
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_texture: &DepthTexture,
    ) {
        if self.vertex_count == 0 {
            return;
        }

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Chunk bounds pass"),
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

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..self.vertex_count, 0..1);
    }
}
