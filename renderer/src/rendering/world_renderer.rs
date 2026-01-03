use std::{collections::HashMap, sync::Arc};

use bytemuck::{Pod, Zeroable};
use crossbeam_channel::Receiver;
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
    voxels::coord::ChunkPos,
    world::World,
};
use wgpu::CommandEncoder;

use crate::rendering::{
    buffer_update_batcher::BufferUpdateBatcher,
    chunk_mesh::{ChunkMesh, ChunkMeshData, ChunkVertex, GpuChunk},
    limits::{INDEX_BUFFER_CAPACITY, MAX_GPU_CHUNKS, VERTEX_BUFFER_CAPACITY},
    memory::{
        gpu_heap::GpuHeap,
        gpu_pool::{GpuPool, GpuPoolHandle},
        typed_buffer::GpuBuffer,
    },
    mesh_generation::chunk_mesh_generator::{ChunkMeshGenerator, ChunkMeshGeneratorEvent},
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

        let chunk_buffer = GpuPool::new(
            device,
            queue.clone(),
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            MAX_GPU_CHUNKS,
            "World chunk buffer",
        );

        Self {
            vertices: Arc::new(vertex_buffer),
            indices: Arc::new(index_buffer),
            chunks: Arc::new(chunk_buffer),
            camera,
        }
    }

    pub fn initialize_chunk_mesh(
        &self,
        batcher: &mut BufferUpdateBatcher,
        mesh_data: &ChunkMeshData,
    ) -> ChunkMesh {
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

        vertex_allocation.wite_data_batched(batcher, &mesh_data.vertices);
        index_allocation.wite_data_batched(batcher, &mesh_data.indices);

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
    scene_texture: Texture,
    post_fx: PostFxRenderer,
    pub texture_manager: TextureManager,
    pub mesh_generator: Arc<ChunkMeshGenerator>,
    mesh_receiver: Receiver<ChunkMeshGeneratorEvent>,
    batcher: BufferUpdateBatcher,
    previous_chunk: Option<ChunkPos>,
    /// True if potentially visible chunks have changed since last frame
    /// Either the camera has moved to a new chunk, or new chunks have been generated
    chunks_changed: bool,
    chunk_ids: Vec<u32>,
}

impl WorldRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: Resolution,
        block_database: Arc<BlockDatabase>,
    ) -> Self {
        let render_camera = RenderCamera::new(device, queue, size);

        let buffers = WorldBuffers::new(
            device,
            queue,
            VERTEX_BUFFER_CAPACITY,
            INDEX_BUFFER_CAPACITY,
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

        let (mesh_generator, mesh_receiver) = ChunkMeshGenerator::new(block_database.clone());
        let mesh_generator = Arc::new(mesh_generator);

        let batcher = BufferUpdateBatcher::new(device.clone(), 64 * 1024 * 1024);

        Self {
            device: device.clone(),
            buffers,
            render_chunks: HashMap::new(),
            sky_pass,
            world_geo_pass,
            camera: render_camera,
            scene_texture,
            post_fx,
            texture_manager,
            mesh_generator,
            mesh_receiver,
            batcher,
            previous_chunk: None,
            chunks_changed: true,
            chunk_ids: Vec::new(),
        }
    }

    fn create_render_chunk_from_mesh(
        batcher: &mut BufferUpdateBatcher,
        buffers: &WorldBuffers,
        pos: ChunkPos,
        mesh_data: &ChunkMeshData,
    ) -> RenderChunk {
        let chunk_mesh = buffers.initialize_chunk_mesh(batcher, mesh_data);
        let gpu_chunk = buffers.chunks.allocate().expect("Failed to allocate chunk");
        let aabb = PackedAABB::try_from(mesh_data.aabb).expect("Failed to pack chunk AABB");

        gpu_chunk.write_data_batched(
            batcher,
            &GpuChunk {
                position: chunk_mesh.position.0.extend(0),
                mesh_data_index_offset: chunk_mesh.indices_handle.index() as u32,
                mesh_data_index_count: chunk_mesh.indices_handle.count() as u32,
                mesh_data_vertex_offset: chunk_mesh.vertices_handle.index() as i32,
                aabb,
            },
        );

        RenderChunk {
            _pos: pos,
            aabb: mesh_data.aabb,
            _mesh: Some(chunk_mesh),
            gpu_handle: gpu_chunk,
        }
    }

    fn update_changed_chunks(&mut self) {
        let batcher = &mut self.batcher;
        let buffers = &self.buffers;

        while let Ok(event) = self.mesh_receiver.try_recv() {
            match event {
                ChunkMeshGeneratorEvent::Generated { pos, mesh } => {
                    let render_chunk =
                        Self::create_render_chunk_from_mesh(batcher, buffers, pos, &mesh);
                    self.render_chunks.insert(pos, render_chunk);
                    self.chunks_changed = true;
                }
            }
        }
    }

    pub fn remove_chunk(&mut self, pos: ChunkPos) {
        self.render_chunks.remove(&pos);
    }

    pub fn create_all_chunks(&mut self, world: &World) {
        log::debug!(
            "Creating render chunks for all world chunks ({})",
            world.chunks.len()
        );

        let batcher = &mut self.batcher;
        let chunk_positions: Vec<ChunkPos> = world.chunks.iter().map(|item| *item.key()).collect();
        let buffers = &self.buffers;

        let chunk_mesh_generator = self.mesh_generator.clone();

        let chunk_meshes = chunk_positions
            .par_iter()
            .map(|&pos| {
                let generator = chunk_mesh_generator.clone();
                (pos, generator.generate_chunk_mesh(world, pos))
            })
            .collect_vec_list();

        for vec in chunk_meshes {
            for (pos, mesh) in vec {
                let render_chunk =
                    Self::create_render_chunk_from_mesh(batcher, buffers, pos, &mesh);
                self.render_chunks.insert(pos, render_chunk);
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
        encoder: &mut CommandEncoder,
        view: &wgpu::TextureView,
        depth_texture: &DepthTexture,
        time: &GameLoopTime,
    ) {
        self.update_changed_chunks();
        self.batcher.flush(encoder);

        self.camera
            .update_camera_matrices(time.blending_factor as f32);
        self.post_fx.update(time);

        let eye = self.camera.eye(time.blending_factor as f32);

        if self.chunks_changed {
            let mut render_chunks: Vec<&RenderChunk> = self.render_chunks.values().collect();

            // Sort by distance to camera
            render_chunks.par_sort_by_cached_key(|chunk| {
                let chunk_center = chunk.aabb.center();
                let distance_sq = eye.distance_squared(chunk_center);
                OrderedFloat(distance_sq)
            });

            self.chunk_ids.clear();

            for chunk in &render_chunks {
                self.chunk_ids.push(chunk.gpu_handle.offset() as u32);
            }

            self.chunks_changed = false;
        }

        let frustum = Frustum::from_inverse_view_projection(self.camera.inverse_view_projection());
        // Update culling params
        let culling_params = CullingParams {
            frustum,
            input_chunk_count: self.chunk_ids.len() as u32,
            _padding: [0; 3],
        };
        // Perform culling pass
        self.world_geo_pass
            .cull_chunks(encoder, &culling_params, &self.chunk_ids);

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

    pub fn update_camera(&mut self, camera: &Camera, immediate: bool) {
        self.camera.update_camera(camera, immediate);

        let current_chunk = camera.get_current_chunk();
        if self.previous_chunk != Some(current_chunk) {
            self.chunks_changed = true;
            self.previous_chunk = Some(current_chunk);
        }
    }

    #[profiling::function]
    pub fn update(&mut self, _time: &GameLoopTime) {}
}
