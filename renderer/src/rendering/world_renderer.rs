use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use bytemuck::{Pod, Zeroable};
use crossbeam_channel::Receiver;
use glam::Vec3;
use ordered_float::OrderedFloat;
use rayon::prelude::*;

use engine::{
    assets::blocks::{BlockDatabase, BlockDatabaseSlim},
    camera::Camera,
    game_loop::GameLoopTime,
    math::{aabb::PackedAABB, frustum::Frustum},
    voxels::{chunk::CHUNK_SIZE, coord::ChunkPos},
    world::World,
};
use wgpu::CommandEncoder;

use crate::rendering::{
    buffer_update_batcher::BufferUpdateBatcher,
    chunk_mesh::PackedVoxelFace,
    chunk_mesh::{ChunkMesh, ChunkMeshData, GpuChunk},
    limits::{FACE_BUFFER_SIZE_BYTES, MAX_GPU_CHUNKS},
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
    pos: ChunkPos,
    _mesh: Option<ChunkMesh>,
    gpu_handle: GpuPoolHandle<GpuChunk>,
}

impl RenderChunk {
    pub fn center(&self) -> Vec3 {
        self.pos.origin().0.as_vec3() + Vec3::splat(CHUNK_SIZE as f32 * 0.5)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct CullingParams {
    pub frustum: Frustum,
    pub input_chunk_count: u32,
    _padding: [u32; 3],
}

pub struct WorldBuffers {
    pub faces: Arc<GpuHeap<PackedVoxelFace>>,
    pub chunks: Arc<GpuPool<GpuChunk>>,
    pub camera: GpuBuffer<CameraUniform>,
}

impl WorldBuffers {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: GpuBuffer<CameraUniform>,
    ) -> Self {
        let faces = GpuHeap::new(
            device,
            queue,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            FACE_BUFFER_SIZE_BYTES as u32,
            (MAX_GPU_CHUNKS * 4) as u32,
            "World face buffer",
        );

        let chunk_buffer = GpuPool::new(
            device,
            queue,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            MAX_GPU_CHUNKS,
            "World chunk buffer",
        );

        Self {
            faces: Arc::new(faces),
            chunks: Arc::new(chunk_buffer),
            camera,
        }
    }

    pub fn initialize_chunk_mesh(
        &self,
        batcher: &mut BufferUpdateBatcher,
        mesh_data: &ChunkMeshData,
    ) -> ChunkMesh {
        let face_allocation = self
            .faces
            .clone()
            .allocate(mesh_data.faces.len() as u32)
            .with_context(|| {
                format!(
                    "Failed to allocate {} faces for chunk mesh",
                    mesh_data.faces.len()
                )
            })
            .expect("Failed to allocate face buffer for chunk mesh");

        face_allocation.write_data_batched(batcher, &mesh_data.faces);

        ChunkMesh {
            position: mesh_data.position,
            aabb: mesh_data.aabb,
            faces_handle: face_allocation,
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

        let buffers = WorldBuffers::new(device, queue, render_camera.uniform_buffer.clone());

        let sky_pass = SkyPass::new(device, &render_camera.uniform_buffer);

        let mut texture_manager = TextureManager::new(device, queue);
        texture_manager
            .load_all_textures(&block_database)
            .expect("Failed to load block materials");

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

        // TODO: Let's hope the block database never changes
        let block_database = BlockDatabaseSlim::from_block_database(&block_database);
        let (mesh_generator, mesh_receiver) = ChunkMeshGenerator::new(Arc::new(block_database));
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
                face_count: mesh_data.faces.len() as u32,
                face_byte_offset: chunk_mesh.faces_handle.byte_offset(),
                aabb,
                _padding: 0,
            },
        );

        RenderChunk {
            pos,
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
            render_chunks.sort_unstable_by_key(|chunk| {
                let chunk_center = chunk.center();
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

    /// Sets both previous and current camera to the same value (no interpolation).
    /// Use when teleporting or initializing.
    pub fn set_camera_immediate(&mut self, camera: &Camera) {
        self.camera.set_camera_immediate(camera);
        self.update_chunk_tracking(camera);
    }

    /// Updates the camera for this frame. Call once per frame after all game updates.
    /// Internally manages interpolation between previous and current states.
    pub fn set_camera(&mut self, camera: &Camera) {
        self.camera.set_camera(camera);
        self.update_chunk_tracking(camera);
    }

    fn update_chunk_tracking(&mut self, camera: &Camera) {
        let current_chunk = camera.get_current_chunk();
        if self.previous_chunk != Some(current_chunk) {
            self.chunks_changed = true;
            self.previous_chunk = Some(current_chunk);
        }
    }

    #[profiling::function]
    pub fn update(&mut self, _time: &GameLoopTime) {}
}
