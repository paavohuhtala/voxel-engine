use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use bytemuck::{Pod, Zeroable};
use offset_allocator::StorageReport;

use engine::{
    assets::blocks::BlockDatabase,
    camera::Camera,
    chunk_loader::ChunkLoaderEvent,
    game_loop::GameLoopTime,
    math::{aabb::PackedAABB, frustum::Frustum},
    mesh_generation::chunk_mesh::{ChunkMeshData, PackedVoxelFace},
    visibility::potentially_visible::PotentiallyVisibleChunks,
    voxels::{
        chunk::{IChunkRenderContext, IChunkRenderState},
        coord::ChunkPos,
    },
};
use wgpu::{CommandEncoder, wgt::CommandEncoderDescriptor};

use crate::{
    renderer_types::RenderWorld,
    rendering::{
        buffer_update_batcher::BufferUpdateBatcher,
        chunk_mesh::{ChunkMesh, GpuChunk},
        limits::{FACE_BUFFER_SIZE_BYTES, MAX_GPU_CHUNKS},
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
    },
};

pub struct ChunkRenderState {
    pub mesh: ChunkMesh,
    pub gpu_chunk: GpuPoolHandle<GpuChunk>,
}

#[derive(Clone)]
pub struct ChunkRenderContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub batcher: BufferUpdateBatcher,
    pub buffers: Arc<WorldBuffers>,
}

impl IChunkRenderContext for ChunkRenderContext {
    fn flush(&mut self, callback: Box<dyn FnOnce() + Send>) {
        log::info!("Flushing ChunkRenderContext batcher");
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("ChunkRenderContext flush encoder"),
            });
        let staging_buffer = self.batcher.flush(&mut encoder);
        self.queue.submit(std::iter::once(encoder.finish()));

        // Keep the staging buffer alive until the GPU work is done
        self.queue.on_submitted_work_done(move || {
            drop(staging_buffer);
            callback();
        });
    }
}

impl IChunkRenderState for ChunkRenderState {
    type Context = ChunkRenderContext;

    fn create_and_upload_mesh(context: &mut Self::Context, mesh_data: ChunkMeshData) -> Self {
        let mesh = context
            .buffers
            .initialize_chunk_mesh(&mut context.batcher, &mesh_data);
        let gpu_chunk = context
            .buffers
            .chunks
            .allocate()
            .expect("Failed to allocate chunk");
        let aabb = PackedAABB::try_from(mesh_data.aabb).expect("Failed to pack chunk AABB");

        gpu_chunk.write_data_batched(
            &mut context.batcher,
            &GpuChunk {
                position: mesh.position.0.extend(0),
                face_count: mesh_data.faces.len() as u32,
                face_byte_offset: mesh.faces_handle.byte_offset(),
                aabb,
                _padding: 0,
            },
        );

        ChunkRenderState { mesh, gpu_chunk }
    }

    fn chunk_gpu_id(&self) -> u64 {
        self.gpu_chunk.offset()
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
    queue: wgpu::Queue,
    buffers: Arc<WorldBuffers>,
    sky_pass: SkyPass,
    world_geo_pass: WorldGeometryPass,
    pub camera: RenderCamera,
    scene_texture: Texture,
    post_fx: PostFxRenderer,
    pub texture_manager: TextureManager,
    /// True if potentially visible chunks have changed since last frame
    /// Either the camera has moved to a new chunk, or new chunks have been generated
    chunks_changed: bool,
    chunk_ids: Vec<u32>,
    chunk_positions: Vec<ChunkPos>,
    id_to_index: HashMap<u32, usize>,
    pos_to_index: HashMap<ChunkPos, usize>,
    potentially_visible: PotentiallyVisibleChunks,
}

pub struct WorldRendererStatistics {
    pub chunk_buffer_capacity: u64,
    pub chunk_buffer_used: u64,
    pub face_buffer_capacity_bytes: u64,
    pub face_buffer_storage_report: StorageReport,
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
            .load_all_textures(&block_database.world_textures)
            .expect("Failed to load block materials");

        let world_geo_pass = WorldGeometryPass::new(device, queue, &buffers, &texture_manager);
        let buffers = Arc::new(buffers);

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
            queue: queue.clone(),
            buffers,
            sky_pass,
            world_geo_pass,
            camera: render_camera,
            scene_texture,
            post_fx,
            texture_manager,
            chunks_changed: true,
            chunk_ids: Vec::new(),
            chunk_positions: Vec::new(),
            // TODO: Reuse this with ChunkLoader
            potentially_visible: PotentiallyVisibleChunks::new(),
            id_to_index: HashMap::new(),
            pos_to_index: HashMap::new(),
        }
    }

    pub fn create_chunk_render_context(&self) -> ChunkRenderContext {
        ChunkRenderContext {
            device: self.device.clone(),
            queue: self.queue.clone(),
            batcher: BufferUpdateBatcher::new(self.device.clone(), 4 * 1024 * 1024),
            buffers: self.buffers.clone(),
        }
    }

    pub fn sync_with_world(&mut self, world: &mut RenderWorld) {
        for message in world.chunk_loader.event_receiver.try_iter() {
            match message {
                ChunkLoaderEvent::ChunkMeshesReady(chunk_mesh_updates) => {
                    for update in chunk_mesh_updates {
                        if let Some(mesh_id) = update.id {
                            log::info!(
                                "Received meshed chunk for position {:?} with mesh ID {}",
                                update.pos,
                                mesh_id
                            );

                            // If this position already has an entry, remove it first (re-mesh case)
                            if let Some(old_index) = self.pos_to_index.remove(&update.pos) {
                                let old_id = self.chunk_ids[old_index];
                                self.id_to_index.remove(&old_id);

                                let last_index = self.chunk_ids.len() - 1;
                                self.chunk_ids.swap_remove(old_index);
                                self.chunk_positions.swap_remove(old_index);

                                // Update moved element's index if we didn't remove the last one
                                if old_index != last_index {
                                    let moved_id = self.chunk_ids[old_index];
                                    let moved_pos = self.chunk_positions[old_index];
                                    self.id_to_index.insert(moved_id, old_index);
                                    self.pos_to_index.insert(moved_pos, old_index);
                                }
                            }

                            let index = self.chunk_ids.len();
                            self.chunk_ids.push(mesh_id as u32);
                            self.chunk_positions.push(update.pos);
                            self.id_to_index.insert(mesh_id as u32, index);
                            self.pos_to_index.insert(update.pos, index);
                        }
                    }
                }
                ChunkLoaderEvent::ChunksUnloaded(positions) => {
                    for pos in positions {
                        if let Some(index_to_remove) = self.pos_to_index.remove(&pos) {
                            let last_index = self.chunk_ids.len() - 1;

                            let removed_id = self.chunk_ids[index_to_remove];
                            self.id_to_index.remove(&removed_id);

                            self.chunk_ids.swap_remove(index_to_remove);
                            self.chunk_positions.swap_remove(index_to_remove);

                            if index_to_remove != last_index {
                                let moved_id = self.chunk_ids[index_to_remove];
                                let moved_pos = self.chunk_positions[index_to_remove];
                                self.id_to_index.insert(moved_id, index_to_remove);
                                self.pos_to_index.insert(moved_pos, index_to_remove);
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn update_visibility(&mut self, world: &RenderWorld) {
        /*if self.chunks_changed {
            let eye = self.camera.interpolated_camera.eye;
            let frustum = &self.camera.interpolated_camera.frustum;
            self.potentially_visible.update_and_sort(eye, frustum);
            self.chunk_ids.clear();

            for chunk_pos in &self.potentially_visible.chunks {
                if let Some(chunk) = world.chunks.get(chunk_pos)
                    && let Some(render_state) = &chunk.render_state
                {
                    self.chunk_ids.push(render_state.gpu_chunk.offset() as u32);
                }
            }

            self.chunks_changed = false;
        }*/
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
        self.post_fx.update(time);

        let frustum = self.camera.interpolated_camera.frustum;
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
    }

    /// Updates the camera for this frame. Call once per frame after all game updates.
    /// Internally manages interpolation between previous and current states.
    pub fn set_camera(&mut self, camera: &Camera, time: &GameLoopTime) {
        self.camera.set_camera(camera, time);
    }

    pub fn get_statistics(&self) -> WorldRendererStatistics {
        WorldRendererStatistics {
            chunk_buffer_capacity: self.buffers.chunks.capacity(),
            chunk_buffer_used: self.buffers.chunks.used(),
            face_buffer_capacity_bytes: self.buffers.faces.capacity_bytes() as u64,
            face_buffer_storage_report: self.buffers.faces.storage_report(),
        }
    }
}
