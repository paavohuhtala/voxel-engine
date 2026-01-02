use std::{collections::HashMap, sync::Arc};

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use ordered_float::OrderedFloat;
use rayon::prelude::*;

use engine::{
    assets::blocks::BlockDatabase,
    camera::Camera,
    game_loop::GameLoopTime,
    math::{
        aabb::{AABB8, PackedAABB},
        frustum::Frustum,
    },
    voxels::{
        chunk::{CHUNK_SIZE, Chunk},
        coord::ChunkPos,
        world::World,
    },
};

use crate::rendering::{
    chunk_mesh::{ChunkMesh, ChunkMeshData, ChunkVertex, GpuChunk},
    chunk_mesh_generator::generate_chunk_mesh_data,
    memory::{
        gpu_heap::GpuHeap,
        gpu_pool::{GpuPool, GpuPoolHandle},
        typed_buffer::GpuBuffer,
    },
    passes::{sky::SkyPass, world_geo::WorldGeometryPass},
    postfx::PostFxRenderer,
    render_camera::{CameraUniform, RenderCamera},
    resolution::Resolution,
    texture::{DepthTexture, Texture},
    texture_manager::TextureManager,
};

pub struct RenderChunk {
    aabb: AABB8,
    _pos: ChunkPos,
    _mesh: Option<ChunkMesh>,
    gpu_handle: GpuPoolHandle<GpuChunk>,
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct CullingParams {
    pub frustum: Frustum,
    pub input_chunk_count: u32,
    _padding: [u32; 3],
}

pub struct WorldBuffers {
    pub vertices: Arc<GpuHeap<ChunkVertex>>,
    pub indices: Arc<GpuHeap<u16>>,
    pub chunks: Arc<GpuPool<GpuChunk>>,
    pub camera: GpuBuffer<CameraUniform>,
}

impl WorldBuffers {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertex_capacity_bytes: u64,
        index_capacity_bytes: u64,
        camera: GpuBuffer<CameraUniform>,
    ) -> Self {
        let vertex_buffer = GpuHeap::new(
            device,
            queue.clone(),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            vertex_capacity_bytes,
            align_of::<ChunkVertex>() as u64,
            "World vertex buffer",
        );
        let index_buffer = GpuHeap::new(
            device,
            queue.clone(),
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            index_capacity_bytes,
            align_of::<u16>() as u64,
            "World index buffer",
        );

        let max_chunks = 32 * 32 * 32;

        let chunk_buffer = GpuPool::new(
            device,
            queue.clone(),
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            max_chunks,
            "World chunk buffer",
        );

        Self {
            vertices: Arc::new(vertex_buffer),
            indices: Arc::new(index_buffer),
            chunks: Arc::new(chunk_buffer),
            camera,
        }
    }

    pub fn initialize_chunk_mesh(&self, mesh_data: &ChunkMeshData) -> ChunkMesh {
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

        log::debug!(
            "Allocated chunk mesh: {} vertices, {} indices. Vertex offset: {}, Index offset: {}",
            mesh_data.vertices.len(),
            mesh_data.indices.len(),
            vertex_allocation.byte_offset(),
            index_allocation.byte_offset()
        );

        vertex_allocation.write_data(&mesh_data.vertices);
        index_allocation.write_data(&mesh_data.indices);

        ChunkMesh {
            position: mesh_data.position,
            aabb: mesh_data.aabb,
            vertices_handle: vertex_allocation,
            indices_handle: index_allocation,
            index_count: mesh_data.indices.len() as u32,
        }
    }
}

pub struct WorldRenderer {
    device: wgpu::Device,
    buffers: WorldBuffers,
    render_chunks: HashMap<ChunkPos, RenderChunk>,
    sky_pass: SkyPass,
    world_geo_pass: WorldGeometryPass,
    pub camera: RenderCamera,
    camera_angle: f32,
    scene_texture: Texture,
    post_fx: PostFxRenderer,
    time: f32,
    block_database: Arc<BlockDatabase>,
    pub texture_manager: TextureManager,
}

impl WorldRenderer {
    // TODO: Make configurable at runtime?
    pub const VERTEX_BUFFER_CAPACITY: u64 = size_of::<ChunkVertex>() as u64 * 16 * 1024 * 1024;
    pub const INDEX_BUFFER_CAPACITY: u64 = size_of::<u16>() as u64 * 1024 * 1024 * 16;

    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: Resolution,
        block_database: Arc<BlockDatabase>,
    ) -> Self {
        let camera = Camera {
            eye: Vec3::new(64.0, 32.0, -32.0),
            target: Vec3::new(0.0, 0.0, 0.0),
            up: Vec3::Y,
        };
        let render_camera = RenderCamera::new(device, queue, camera, size);

        let buffers = WorldBuffers::new(
            device,
            queue,
            Self::VERTEX_BUFFER_CAPACITY,
            Self::INDEX_BUFFER_CAPACITY,
            render_camera.uniform_buffer.clone(),
        );

        let sky_pass = SkyPass::new(device, &render_camera.uniform_buffer);

        let texture_manager = TextureManager::new(device, queue);

        let world_geo_pass = WorldGeometryPass::new(device, queue, &buffers, &texture_manager);

        let scene_texture = Texture::from_descriptor(
            device,
            wgpu::TextureDescriptor {
                label: Some("Scene texture"),
                size: wgpu::Extent3d {
                    width: size.width,
                    height: size.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            None,
        );

        let post_fx = PostFxRenderer::new(
            device,
            queue,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            &scene_texture.view,
            size,
        );

        Self {
            device: device.clone(),
            buffers,
            render_chunks: HashMap::new(),
            sky_pass,
            world_geo_pass,
            camera: render_camera,
            camera_angle: 0.0,
            scene_texture,
            post_fx,
            time: 0.0,
            texture_manager,
            block_database: block_database.clone(),
        }
    }

    fn create_render_chunk(
        block_database: &BlockDatabase,
        buffers: &WorldBuffers,
        pos: ChunkPos,
        chunk: &Chunk,
        world: &World,
    ) -> (ChunkPos, RenderChunk) {
        log::debug!("Creating render chunk at position {:?}", pos);

        let mesh_data = generate_chunk_mesh_data(block_database, pos, chunk, world);
        let chunk_mesh = buffers.initialize_chunk_mesh(&mesh_data);
        let gpu_chunk = buffers.chunks.allocate().expect("Failed to allocate chunk");
        let aabb = PackedAABB::try_from(mesh_data.aabb).expect("Failed to pack chunk AABB");

        gpu_chunk.write_data(&GpuChunk {
            position: chunk_mesh.position.extend(0),
            mesh_data_index_offset: chunk_mesh.indices_handle.index() as u32,
            mesh_data_index_count: chunk_mesh.indices_handle.count() as u32,
            mesh_data_vertex_offset: chunk_mesh.vertices_handle.index() as i32,
            aabb,
        });

        let render_chunk = RenderChunk {
            _pos: pos,
            aabb: mesh_data.aabb,
            _mesh: Some(chunk_mesh),
            gpu_handle: gpu_chunk,
        };
        (pos, render_chunk)
    }

    pub fn create_all_chunks(&mut self, world: &World) {
        log::debug!(
            "Creating render chunks for all world chunks ({})",
            world.chunks.len()
        );

        let chunk_positions: Vec<ChunkPos> = world.chunks.iter().map(|item| *item.key()).collect();
        let buffers = &self.buffers;

        let block_database = self.block_database.clone();

        let render_chunks = chunk_positions
            .par_iter()
            .filter_map(|&pos| {
                // Because chunks are stored in a DashMap, they could technically disapper while we're iterating
                world.chunks.get(&pos).map(|chunk| {
                    Self::create_render_chunk(block_database.as_ref(), buffers, pos, &chunk, world)
                })
            })
            .collect_vec_list();

        for vec in render_chunks {
            for (pos, chunk) in vec {
                self.render_chunks.insert(pos, chunk);
            }
        }

        let vertex_buffer_stats = self.buffers.vertices.get_stats();
        let index_buffer_stats = self.buffers.indices.get_stats();

        log::info!(
            "Created {} render chunks. Vertex buffer stats: {:?}, Index buffer stats: {:?}",
            self.render_chunks.len(),
            vertex_buffer_stats,
            index_buffer_stats
        );
    }

    pub fn resize(&mut self, size: Resolution) {
        self.camera.resize(size);
        self.scene_texture.resize(&self.device, size);
        self.post_fx.resize(size, &self.scene_texture.view);
    }

    #[profiling::function]
    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_texture: &DepthTexture,
        time: &GameLoopTime,
    ) {
        self.camera
            .update_camera_matrices(time.blending_factor as f32);
        self.post_fx.update(self.time);

        let eye = self.camera.eye(time.blending_factor as f32);
        let mut render_chunks: Vec<&RenderChunk> = self.render_chunks.values().collect();

        // Sort by distance to camera
        render_chunks.par_sort_by_cached_key(|chunk| {
            let chunk_center = chunk.aabb.center();
            let distance_sq = eye.distance_squared(chunk_center);
            OrderedFloat(distance_sq)
        });

        // Add all chunk IDs to the input buffer for culling
        // TODO: Reuse allocation
        let chunk_ids: Vec<u32> = render_chunks
            .iter()
            .map(|chunk| chunk.gpu_handle.offset() as u32)
            .collect();

        let frustum = Frustum::from_inverse_view_projection(self.camera.inverse_view_projection());
        // Update culling params
        let culling_params = CullingParams {
            frustum,
            input_chunk_count: chunk_ids.len() as u32,
            _padding: [0; 3],
        };
        // Perform culling pass
        self.world_geo_pass
            .cull_chunks(encoder, &culling_params, &chunk_ids);

        // Render sky
        self.sky_pass.render(encoder, &self.scene_texture.view);

        // Render visible chunks
        self.world_geo_pass.draw_chunks(
            encoder,
            &self.scene_texture.view,
            depth_texture,
            culling_params.input_chunk_count,
        );

        // Render PostFX
        self.post_fx.render(encoder, view);
    }

    #[profiling::function]
    pub fn update(&mut self, time: &GameLoopTime) {
        // Rotate camera around the origin
        let rotation_speed = -0.02; // Radians per second
        self.camera_angle += rotation_speed * time.delta_time_s as f32;
        let radius = 45.0;

        self.camera.update_camera(
            &Camera {
                eye: Vec3::new(
                    radius * self.camera_angle.cos(),
                    16.0,
                    radius * self.camera_angle.sin(),
                ),
                target: Vec3::splat(CHUNK_SIZE as f32 / 2.0),
                up: Vec3::Y,
            },
            false,
        );
    }
}
