use std::{sync::Arc, time::Duration};

use bytemuck::{Pod, Zeroable};
use dashmap::DashMap;
use glam::Vec3;
use log::log;
use wgpu::wgt::DrawIndexedIndirectArgs;

use crate::{
    camera::Camera,
    math::aabb::AABB,
    rendering::{
        chunk_mesh::{ChunkMesh, ChunkMeshData, ChunkVertex, GpuChunk},
        chunk_mesh_generator::generate_chunk_mesh_data,
        memory::{
            gpu_heap::GpuHeap,
            gpu_pool::{GpuPool, GpuPoolHandle},
            typed_buffer::{GpuBuffer, GpuBufferArray},
        },
        passes::{render_common::RenderCommon, world_geo::WorldGeometryPass},
        render_camera::RenderCamera,
        resolution::Resolution,
        texture::DepthTexture,
    },
    voxels::{
        chunk::{CHUNK_SIZE, Chunk},
        coord::ChunkPos,
        world::World,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkVisibility {
    Visible,
    NotVisible,
}

pub struct RenderChunk {
    pos: ChunkPos,
    aabb: AABB,
    visibility: ChunkVisibility,
    mesh: Option<ChunkMesh>,
    gpu_handle: GpuPoolHandle<GpuChunk>,
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct CullingParams {
    pub input_chunk_count: u32,
}

pub struct WorldBuffers {
    vertices: Arc<GpuHeap<ChunkVertex>>,
    indices: Arc<GpuHeap<u16>>,
    chunks: Arc<GpuPool<GpuChunk>>,
    culling_params: GpuBuffer<CullingParams>,
    input_chunk_ids: GpuBufferArray<u32>,
    draw_commands: GpuBufferArray<DrawIndexedIndirectArgs>,
    draw_count: GpuBuffer<u32>,
}

impl WorldBuffers {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertex_capacity: u64,
        index_capacity: u64,
    ) -> Self {
        let vertex_buffer = GpuHeap::new(
            device,
            queue.clone(),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            vertex_capacity,
            align_of::<ChunkVertex>() as u64,
            "World vertex buffer",
        );
        let index_buffer = GpuHeap::new(
            device,
            queue.clone(),
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            index_capacity,
            align_of::<u16>() as u64,
            "World index buffer",
        );
        let chunk_buffer = GpuPool::new(
            device,
            queue.clone(),
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            1024,
            "World chunk buffer",
        );

        let max_chunks = 1024;

        let culling_params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Culling params buffer"),
            size: size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let input_chunk_ids = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Input chunk IDs buffer"),
            size: (max_chunks * size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let draw_commands = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Draw commands buffer"),
            size: (max_chunks * size_of::<DrawIndexedIndirectArgs>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
            mapped_at_creation: false,
        });

        let draw_count = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Draw count buffer"),
            size: size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::INDIRECT,
            mapped_at_creation: false,
        });

        Self {
            vertices: Arc::new(vertex_buffer),
            indices: Arc::new(index_buffer),
            chunks: Arc::new(chunk_buffer),
            culling_params: GpuBuffer::from_buffer(culling_params),
            input_chunk_ids: GpuBufferArray::from_buffer(input_chunk_ids),
            draw_commands: GpuBufferArray::from_buffer(draw_commands),
            draw_count: GpuBuffer::from_buffer(draw_count),
        }
    }

    pub fn allocate_chunk_mesh(&self, mesh_data: &ChunkMeshData) -> ChunkMesh {
        log!(
            log::Level::Info,
            "Allocating chunk mesh: {} vertices, {} indices",
            mesh_data.vertices.len(),
            mesh_data.indices.len()
        );
        let vertex_allocation = self
            .vertices
            .clone()
            .allocate(mesh_data.vertices.len() as u64)
            .expect("Failed to allocate vertex buffer for chunk mesh");

        let index_allocation = self
            .indices
            .clone()
            .allocate(mesh_data.indices.len() as u64)
            .expect("Failed to allocate index buffer for chunk mesh");

        vertex_allocation.write_data(&mesh_data.vertices);
        index_allocation.write_data(&mesh_data.indices);

        ChunkMesh {
            position_and_y_range: mesh_data.position_and_y_range,
            vertices_handle: vertex_allocation,
            indices_handle: index_allocation,
            index_count: mesh_data.indices.len() as u32,
        }
    }
}

pub struct WorldRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    buffers: WorldBuffers,
    render_chunks: DashMap<ChunkPos, RenderChunk>,
    pass: WorldGeometryPass,
    // TODO: Lift this higher up
    render_common: RenderCommon,
    pub camera: RenderCamera,
    camera_angle: f32,
}

impl WorldRenderer {
    // TODO: Make configurable at runtime?
    pub const VERTEX_BUFFER_CAPACITY: u64 = 16 * 1024 * 1024; // 16 million vertices
    pub const INDEX_BUFFER_CAPACITY: u64 = 32 * 1024 * 1024; // 32 million indices

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32) -> Self {
        let render_common = RenderCommon::new(device);
        let buffers = WorldBuffers::new(device, queue, 1024 * 1024, 1024 * 1024);

        let camera = Camera {
            eye: Vec3::new(64.0, 32.0, -32.0),
            target: Vec3::new(0.0, 0.0, 0.0),
            up: Vec3::Y,
        };
        let resolution = Resolution { width, height };
        let render_camera = RenderCamera::new(device, camera, resolution);

        let pass = WorldGeometryPass::new(
            device,
            &render_camera.uniform_buffer,
            buffers.chunks.buffer(),
            &buffers.culling_params.inner(),
            &buffers.input_chunk_ids.inner(),
            &buffers.draw_commands.inner(),
            &buffers.draw_count.inner(),
            &buffers.vertices.buffer(),
            &buffers.indices.buffer(),
        );

        Self {
            device: device.clone(),
            queue: queue.clone(),
            buffers,
            render_chunks: DashMap::new(),
            pass,
            render_common,
            camera: render_camera,
            camera_angle: 0.0,
        }
    }

    fn create_render_chunk(&self, pos: ChunkPos, chunk: &Chunk) {
        log!(
            log::Level::Info,
            "Creating render chunk at position {:?}",
            pos
        );

        let mesh_data = generate_chunk_mesh_data(pos, chunk);
        let chunk_mesh = self.buffers.allocate_chunk_mesh(&mesh_data);
        // TODO: Proper AABB calculation
        let aabb = AABB::new(
            glam::Vec3::new(
                (pos.x() * CHUNK_SIZE as i32) as f32,
                (pos.y() * CHUNK_SIZE as i32) as f32,
                (pos.z() * CHUNK_SIZE as i32) as f32,
            ),
            glam::Vec3::new(
                ((pos.x() + 1) * CHUNK_SIZE as i32) as f32,
                ((pos.y() + 1) * CHUNK_SIZE as i32) as f32,
                ((pos.z() + 1) * CHUNK_SIZE as i32) as f32,
            ),
        );
        let gpu_chunk = self
            .buffers
            .chunks
            .allocate()
            .expect("Failed to allocate chunk");

        gpu_chunk.write_data(&GpuChunk {
            position_and_y_range: chunk_mesh.position_and_y_range,
            mesh_data_index_offset: chunk_mesh.indices_handle.offset() as u32,
            mesh_data_index_count: chunk_mesh.indices_handle.count() as u32,
            mesh_data_vertex_offset: chunk_mesh.vertices_handle.offset() as i32,
        });

        let render_chunk = RenderChunk {
            pos,
            aabb,
            visibility: ChunkVisibility::Visible,
            mesh: Some(chunk_mesh),
            gpu_handle: gpu_chunk,
        };
        self.render_chunks.insert(pos, render_chunk);
    }

    pub fn create_all_chunks(&self, world: &World) {
        log!(
            log::Level::Info,
            "Creating render chunks for all world chunks ({})",
            world.chunks.len()
        );

        for item in world.chunks.iter() {
            self.create_render_chunk(*item.key(), item.value());
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.camera.update_resolution(Resolution { width, height });
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_texture: &DepthTexture,
    ) {
        self.camera.update_uniform_buffer(&self.queue);

        // Add all chunk IDs to the input buffer for culling
        // TODO: Reuse allocation
        let chunk_ids: Vec<u32> = self
            .render_chunks
            .iter()
            .map(|chunk| chunk.gpu_handle.offset() as u32)
            .collect();

        // Reset draw count
        self.buffers.draw_count.write_data(&self.queue, &0u32);

        // Update culling params
        let culling_params = CullingParams {
            input_chunk_count: chunk_ids.len() as u32,
        };
        self.buffers
            .culling_params
            .write_data(&self.queue, &culling_params);
        // Copy chunk IDs to GPU
        self.buffers
            .input_chunk_ids
            .write_data(&self.queue, &chunk_ids);

        // Perform culling pass
        self.pass
            .cull_chunks(encoder, culling_params.input_chunk_count);

        // Render visible chunks
        self.pass.draw_chunks(
            encoder,
            view,
            depth_texture,
            culling_params.input_chunk_count,
        );
    }

    pub fn update(&mut self, delta_time: Duration) {
        // Rotate camera around the origin
        let rotation_speed = 0.5; // Radians per second
        self.camera_angle += rotation_speed * delta_time.as_secs_f32();
        let radius = 96.0;

        self.camera.update_camera(&Camera {
            eye: Vec3::new(
                radius * self.camera_angle.cos(),
                32.0,
                radius * self.camera_angle.sin(),
            ),
            target: Vec3::splat(CHUNK_SIZE as f32) / 2.0,
            up: Vec3::Y,
        });
    }
}
