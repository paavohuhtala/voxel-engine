use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
    sync::Arc,
};

use anyhow::Context;
use bytemuck::{Pod, Zeroable};
use glam::IVec3;
use offset_allocator::StorageReport;

use engine::{
    assets::blocks::BlockDatabase,
    camera::Camera,
    chunk_loader::ChunkLoaderEvent,
    game_loop::GameLoopTime,
    math::{
        aabb::{AABB8, PackedAABB},
        frustum::Frustum,
    },
    mesh_generation::chunk_mesh::{ChunkMeshData, PackedVoxelFace},
    voxels::{
        chunk::{ChunkState, IChunkRenderContext, IChunkRenderState},
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
        passes::{chunk_bounds::ChunkBoundsPass, sky::SkyPass, world_geo::WorldGeometryPass},
        postfx::PostFxRenderer,
        render_camera::{CameraUniform, RenderCamera},
        resolution::Resolution,
        texture::{DepthTexture, Texture},
        texture_manager::TextureManager,
    },
};

#[derive(Copy, Clone, Eq, PartialEq)]
struct DebugChunkCandidate {
    dist: i32,
    pos: [i32; 3],
}

impl Ord for DebugChunkCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // Max-heap by distance; break ties by position for determinism.
        self.dist
            .cmp(&other.dist)
            .then_with(|| self.pos.cmp(&other.pos))
    }
}

impl PartialOrd for DebugChunkCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub struct ChunkRenderState {
    pub mesh: ChunkMesh,
    pub gpu_chunk: GpuPoolHandle<GpuChunk>,
}

#[derive(Clone)]
pub struct ChunkRenderContext {
    device: wgpu::Device,
    pub batcher: BufferUpdateBatcher,
    pub buffers: Arc<WorldBuffers>,
}

impl IChunkRenderContext for ChunkRenderContext {
    type FlushResult = (wgpu::CommandBuffer, Option<wgpu::Buffer>);

    fn flush(&mut self) -> Self::FlushResult {
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("ChunkRenderContext flush encoder"),
            });
        let staging_buffer = self.batcher.flush(&mut encoder);
        (encoder.finish(), staging_buffer)
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
    chunk_bounds_pass: ChunkBoundsPass,
    pub camera: RenderCamera,
    scene_texture: Texture,
    post_fx: PostFxRenderer,
    pub texture_manager: TextureManager,
    /// Maps chunk positions to their GPU chunk IDs for rendering
    rendered_chunks: HashMap<ChunkPos, u32>,
    /// Cached mesh AABBs for currently rendered chunks (chunk-local coordinates).
    rendered_chunk_aabbs: HashMap<ChunkPos, AABB8>,
    /// Whether to show chunk boundary wireframes for debugging
    pub show_chunk_bounds: bool,
    /// If true, the chunk bounds debug pass draws each chunk's mesh AABB instead of the full chunk.
    pub use_mesh_aabb_for_bounds: bool,
}

pub struct WorldRendererStatistics {
    pub chunk_buffer_capacity: u64,
    pub chunk_buffer_used: u64,
    pub face_buffer_capacity_bytes: u64,
    pub face_buffer_storage_report: StorageReport,
}

impl WorldRenderer {
    fn camera_chunk_pos(&self) -> IVec3 {
        let eye = self.camera.interpolated_camera.eye;

        // Match engine's chunking: floor division for negatives.
        fn floor_div_16(v: f32) -> i32 {
            (v / 16.0).floor() as i32
        }

        IVec3::new(
            floor_div_16(eye.x),
            floor_div_16(eye.y),
            floor_div_16(eye.z),
        )
    }

    fn collect_nearest_debug_chunk_positions(&self) -> Vec<IVec3> {
        let max = self.chunk_bounds_pass.max_chunks();
        let camera_chunk = self.camera_chunk_pos();
        let mut heap: BinaryHeap<DebugChunkCandidate> = BinaryHeap::new();

        for pos in self.rendered_chunks.keys() {
            let p = pos.0;
            let dist = (p - camera_chunk).abs().max_element();
            let candidate = DebugChunkCandidate {
                dist,
                pos: p.to_array(),
            };

            if heap.len() < max {
                heap.push(candidate);
            } else if let Some(worst) = heap.peek().copied()
                && dist < worst.dist
            {
                let _ = heap.pop();
                heap.push(candidate);
            }
        }

        let mut picked = heap.into_vec();
        picked.sort_unstable_by_key(|c| c.dist);
        picked
            .into_iter()
            .map(|c| IVec3::from_array(c.pos))
            .collect()
    }

    fn collect_nearest_debug_chunk_aabbs(&self) -> Vec<(IVec3, AABB8)> {
        let max = self.chunk_bounds_pass.max_chunks();
        let camera_chunk = self.camera_chunk_pos();
        let mut heap: BinaryHeap<DebugChunkCandidate> = BinaryHeap::new();

        for pos in self.rendered_chunk_aabbs.keys() {
            let p = pos.0;
            let dist = (p - camera_chunk).abs().max_element();
            let candidate = DebugChunkCandidate {
                dist,
                pos: p.to_array(),
            };

            if heap.len() < max {
                heap.push(candidate);
            } else if let Some(worst) = heap.peek().copied()
                && dist < worst.dist
            {
                let _ = heap.pop();
                heap.push(candidate);
            }
        }

        let mut picked = heap.into_vec();
        picked.sort_unstable_by_key(|c| c.dist);
        let mut out = Vec::with_capacity(picked.len());
        for c in picked {
            let pos = IVec3::from_array(c.pos);
            if let Some(aabb) = self.rendered_chunk_aabbs.get(&ChunkPos(pos)) {
                out.push((pos, *aabb));
            }
        }

        out
    }

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
        let chunk_bounds_pass = ChunkBoundsPass::new(device, &render_camera.uniform_buffer);
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
            chunk_bounds_pass,
            camera: render_camera,
            scene_texture,
            post_fx,
            texture_manager,
            rendered_chunks: HashMap::new(),
            rendered_chunk_aabbs: HashMap::new(),
            show_chunk_bounds: false,
            use_mesh_aabb_for_bounds: false,
        }
    }

    pub fn create_chunk_render_context(&self) -> ChunkRenderContext {
        ChunkRenderContext {
            device: self.device.clone(),
            batcher: BufferUpdateBatcher::new(self.device.clone(), 4 * 1024 * 1024),
            buffers: self.buffers.clone(),
        }
    }

    pub fn sync_with_world(&mut self, world: &RenderWorld) {
        for message in world.chunk_loader.event_receiver.try_iter() {
            match message {
                ChunkLoaderEvent::ChunkMeshesReady(chunk_mesh_updates, flush_result) => {
                    if let Some(result) = flush_result {
                        let (command_buffer, _staging_buffer) = result;
                        self.queue.submit(Some(command_buffer));
                    }

                    for update in chunk_mesh_updates {
                        // Skip chunks that were unloaded while waiting for flush
                        if update.handle.state() == ChunkState::Unloaded {
                            continue;
                        }

                        if let Some(mesh_id) = update.id {
                            // Insert or replace the chunk's GPU ID
                            self.rendered_chunks
                                .insert(update.handle.pos, mesh_id as u32);

                            // Cache the chunk's mesh AABB for debug rendering.
                            if let Some(chunk) = world.chunks.get(&update.handle.pos)
                                && let Some(render_state) = chunk.render_state.as_ref()
                            {
                                self.rendered_chunk_aabbs
                                    .insert(update.handle.pos, render_state.mesh.aabb);
                            }

                            update.handle.set_state(ChunkState::Ready);
                        } else {
                            // Empty chunk - remove from rendering if it was there
                            self.rendered_chunks.remove(&update.handle.pos);
                            self.rendered_chunk_aabbs.remove(&update.handle.pos);
                            update.handle.set_state(ChunkState::ReadyEmpty);
                        }
                    }
                }
                ChunkLoaderEvent::ChunksUnloaded(positions) => {
                    for pos in positions {
                        self.rendered_chunks.remove(&pos);
                        self.rendered_chunk_aabbs.remove(&pos);
                    }
                }
                ChunkLoaderEvent::WorldReset => {
                    self.rendered_chunks.clear();
                    self.rendered_chunk_aabbs.clear();
                }
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
        self.post_fx.update(time);

        // Collect chunk IDs for rendering
        let chunk_ids: Vec<u32> = self.rendered_chunks.values().copied().collect();

        let frustum = self.camera.interpolated_camera.frustum;
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

        // Render chunk bounds wireframes if enabled
        if self.show_chunk_bounds {
            if self.use_mesh_aabb_for_bounds {
                let chunks = self.collect_nearest_debug_chunk_aabbs();
                self.chunk_bounds_pass
                    .update_chunk_aabbs(&self.queue, &chunks);
            } else {
                let chunk_positions = self.collect_nearest_debug_chunk_positions();
                self.chunk_bounds_pass
                    .update_chunks(&self.queue, &chunk_positions);
            }
            self.chunk_bounds_pass
                .render(encoder, &self.scene_texture.view, depth_texture);
        }

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

    /// Toggle the visibility of chunk boundary wireframes for debugging
    pub fn toggle_chunk_bounds(&mut self) {
        self.show_chunk_bounds = !self.show_chunk_bounds;
    }
}
