use std::{
    sync::{Arc, RwLock},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender, select};
use dashmap::DashMap;
use glam::IVec3;

use crate::{
    assets::blocks::BlockDatabaseSlim,
    camera::Camera,
    limits::LOAD_DISTANCE,
    mesh_generation::{
        chunk_mesh_generator_input::ChunkMeshGeneratorInput, greedy_mesher::GreedyMesher,
    },
    visibility::generate_desired_chunk_offsets,
    voxels::{
        chunk::{
            Chunk, ChunkData, ChunkHandle, ChunkState, IChunkRenderContext, IChunkRenderState,
        },
        coord::{ChunkPos, WorldPosF},
        face::Face,
    },
    worldgen::WorldGenerator,
};

#[derive(Debug)]
pub enum ChunkLoaderEvent<T: IChunkRenderState> {
    ChunkMeshesReady(
        Vec<ChunkMeshUpdate>,
        Option<<T::Context as IChunkRenderContext>::FlushResult>,
    ),
    ChunksUnloaded(Vec<ChunkPos>),
}

// Used by the main thread to communicate with the chunk loader thread
pub enum ChunkLoaderCommand {
    Shutdown,
}

// Used by chunk loader workers to communicate back to the chunk loader
pub enum ChunkWorkerEvent<T: IChunkRenderState> {
    ChunkLoaded(ChunkHandle),
    ChunkReadyForMeshing(ChunkHandle),
    MeshesGenerated(
        Vec<ChunkMeshUpdate>,
        Option<<T::Context as IChunkRenderContext>::FlushResult>,
    ),
}

pub type WorldChunks = Arc<DashMap<ChunkPos, Chunk>>;

pub trait WorldAccess<T: IChunkRenderState>: Send + Sync {
    fn get_handle(&self, pos: ChunkPos) -> Option<ChunkHandle>;
    fn insert_initial_chunk(&self, pos: ChunkPos) -> ChunkHandle;
    fn create_mesh_input(&self, pos: ChunkPos) -> anyhow::Result<Option<ChunkMeshGeneratorInput>>;
    fn is_ready_for_meshing(&self, pos: ChunkPos) -> bool;
    fn insert_chunk_data(
        &self,
        chunk: &ChunkHandle,
        data: ChunkData,
        sender: &Sender<ChunkWorkerEvent<T>>,
    );
    fn insert_render_state(&self, pos: ChunkPos, render_state: T);
    fn remove_chunk(&self, pos: ChunkPos);
    fn unload_chunks_outside_distance(&self, center: ChunkPos, distance: u32) -> Vec<ChunkPos>;
}

impl<T: IChunkRenderState> WorldAccess<T> for DashMap<ChunkPos, Chunk<T>> {
    fn get_handle(&self, pos: ChunkPos) -> Option<ChunkHandle> {
        self.get(&pos).map(|chunk| chunk.handle())
    }

    fn insert_initial_chunk(&self, pos: ChunkPos) -> ChunkHandle {
        let chunk = Chunk::new(pos);
        let handle = chunk.handle();
        self.insert(pos, chunk);
        handle
    }

    fn create_mesh_input(&self, pos: ChunkPos) -> anyhow::Result<Option<ChunkMeshGeneratorInput>> {
        ChunkMeshGeneratorInput::try_from_map(self, pos)
    }

    fn is_ready_for_meshing(&self, pos: ChunkPos) -> bool {
        if let Some(chunk) = self.get(&pos) {
            chunk.neighbor_state.is_ready_for_meshing()
        } else {
            false
        }
    }

    fn insert_chunk_data(
        &self,
        chunk: &ChunkHandle,
        data: ChunkData,
        sender: &Sender<ChunkWorkerEvent<T>>,
    ) {
        // Update the existing chunk's data in place, preserving state and neighbor_state Arcs
        if let Some(mut existing) = self.get_mut(&chunk.pos) {
            existing.data = Some(data);
        } else {
            // Chunk was removed (e.g., unloaded) while being generated, ignore
            return;
        }

        // Mark this chunk as generated for every neighboring chunk
        let mut neighbor_bits = 0u8;
        for direction in Face::all().iter().copied() {
            let neighbor_pos = chunk.pos.get_neighbor(direction);
            if let Some(neighbor) = self.get_handle(neighbor_pos) {
                let neighbor_state = neighbor.state();
                if neighbor_state < ChunkState::Loaded {
                    // Neighbor is not yet loaded, skip
                    continue;
                }

                let neighbor_ready_for_meshing = neighbor
                    .neighbor_state
                    .set_neighbor_ready(direction.opposite());

                if neighbor_ready_for_meshing {
                    sender
                        .send(ChunkWorkerEvent::ChunkReadyForMeshing(neighbor))
                        .unwrap();
                }

                neighbor_bits |= 1 << (direction as u8);
            }
        }

        let ready_for_meshing = chunk.neighbor_state.set_neighbor_bits(neighbor_bits);

        if ready_for_meshing {
            sender
                .send(ChunkWorkerEvent::ChunkReadyForMeshing(chunk.clone()))
                .unwrap();
        } else {
            // Just mark this as loaded
            chunk.set_state(ChunkState::Loaded);
        }
    }

    fn remove_chunk(&self, pos: ChunkPos) {
        self.remove(&pos);
    }

    fn insert_render_state(&self, pos: ChunkPos, render_state: T) {
        if let Some(mut chunk) = self.get_mut(&pos) {
            let state = chunk.state.load();
            if state == ChunkState::Unloaded {
                // Chunks is about to be unloaded, don't insert render state
                return;
            }
            chunk.render_state = Some(render_state);
        }
    }

    fn unload_chunks_outside_distance(&self, center: ChunkPos, distance: u32) -> Vec<ChunkPos> {
        let mut removed = Vec::new();
        self.retain(|pos, chunk| {
            let dist = pos.0.chebyshev_distance(center.0);
            let keep = dist <= distance;
            if !keep {
                chunk.before_unload();
                removed.push(*pos);
            }
            keep
        });
        removed
    }
}

pub struct ChunkLoaderHandle<T: IChunkRenderState> {
    pub command_sender: Sender<ChunkLoaderCommand>,
    pub event_receiver: Receiver<ChunkLoaderEvent<T>>,
    pub camera_moved_sender: Sender<()>,
    pub _thread_handle: JoinHandle<()>,
    pub camera: Arc<RwLock<Camera>>,
}

impl<T: IChunkRenderState> ChunkLoaderHandle<T> {
    pub fn notify_camera_moved(&self) {
        let _ = self.camera_moved_sender.try_send(());
    }
}

/// Manages loading and unloading of chunks in the world.
/// Loading can involve generating new chunks or loading from disk.
/// The chunk loader lives in its own thread and communicates with the main engine thread via channels.
pub struct ChunkLoader<T: IChunkRenderState> {
    command_receiver: Receiver<ChunkLoaderCommand>,
    worker_event_receiver: Receiver<ChunkWorkerEvent<T>>,
    event_sender: Sender<ChunkLoaderEvent<T>>,
    previous_chunk_pos: ChunkPos,
    world_access: Arc<dyn WorldAccess<T>>,
    camera_moved_receiver: Receiver<()>,
    worker_pool: ChunkLoaderWorkerPool,
    desired_generation_offsets: Vec<IVec3>,
    job_queue: Vec<ChunkLoaderJob>,
    camera: Arc<RwLock<Camera>>,
}

impl<T: IChunkRenderState> ChunkLoader<T> {
    pub fn start(
        world_generator: Box<dyn WorldGenerator>,
        block_database: Arc<BlockDatabaseSlim>,
        world_access: Arc<dyn WorldAccess<T>>,
        render_context: T::Context,
    ) -> ChunkLoaderHandle<T> {
        let (command_sender, command_receiver) = crossbeam_channel::unbounded();
        let (camera_moved_sender, camera_moved_receiver) = crossbeam_channel::bounded(1);
        let (worker_event_sender, worker_event_receiver) = crossbeam_channel::unbounded();
        let (event_sender, event_receiver) = crossbeam_channel::unbounded();

        let camera = Arc::new(RwLock::new(Camera::default()));
        let camera_clone = camera.clone();

        let thread = std::thread::Builder::new()
            .name("Chunk loader".to_string())
            .spawn(move || {
                let world_generator = Arc::from(world_generator);
                let offsets = generate_desired_chunk_offsets();

                let worker_pool = ChunkLoaderWorkerPool::new(
                    // TODO: use num_cpus crate
                    8,
                    worker_event_sender,
                    world_generator,
                    block_database,
                    world_access.clone(),
                    render_context,
                );

                let mut chunk_loader = ChunkLoader {
                    command_receiver,
                    worker_event_receiver,
                    event_sender,
                    previous_chunk_pos: ChunkPos::new(0, 0, 0),
                    world_access,
                    camera_moved_receiver,
                    desired_generation_offsets: offsets,
                    worker_pool,
                    job_queue: Vec::new(),
                    camera: camera_clone,
                };

                chunk_loader.run()
            })
            .unwrap();

        ChunkLoaderHandle {
            command_sender,
            event_receiver,
            camera_moved_sender,
            _thread_handle: thread,
            camera,
        }
    }

    fn run(&mut self) {
        loop {
            select! {
                recv(self.command_receiver) -> command => {
                    match command {
                        Err(_) | Ok(ChunkLoaderCommand::Shutdown) => {
                            break;
                        },
                    }
                }
                recv(self.worker_event_receiver) -> event => {
                    match event {
                        Ok(ChunkWorkerEvent::ChunkLoaded(_chunk)) => {
                        }
                        Ok(ChunkWorkerEvent::ChunkReadyForMeshing(chunk)) => {
                            chunk.set_state(ChunkState::InMeshingQueue);
                            self.job_queue.push(ChunkLoaderJob::GenerateMesh(chunk));
                        }
                        Ok(ChunkWorkerEvent::MeshesGenerated(updates, flush_results)) => {
                            self.event_sender.send(ChunkLoaderEvent::ChunkMeshesReady(updates, flush_results)).unwrap();
                        }
                        Err(_) => {
                            // Channel closed, should not happen
                            break;
                        }
                    }
                }
                recv(self.camera_moved_receiver) -> _ => {
                    self.on_camera_moved();
                }
                // Use a short timeout to ensure give_jobs_to_workers is called regularly
                // even when no events are pending
                default(std::time::Duration::from_millis(1)) => {}
            }
            self.give_jobs_to_workers();
        }
    }

    fn give_jobs_to_workers(&mut self) {
        // While there are jobs in the queue and the channel is not full, send jobs to workers
        while !self.job_queue.is_empty() {
            let job = self.job_queue.last().cloned().unwrap();
            match self.worker_pool.job_sender.try_send(job) {
                Ok(_) => {
                    // Worker accepted the job, remove it from the queue
                    self.job_queue.pop();
                }
                Err(_) => {
                    // The channel is full, stop trying to send more jobs
                    break;
                }
            }
        }
    }

    // When the camera moves (and on startup), we need to potentially load/unload chunks
    fn on_camera_moved(&mut self) {
        let current_chunk_pos = {
            let camera = self.camera.read().unwrap();
            WorldPosF(camera.eye).to_chunk_pos()
        };

        // Camera moved but chunk didn't change, nothing to do
        if current_chunk_pos == self.previous_chunk_pos {
            return;
        }

        self.previous_chunk_pos = current_chunk_pos;

        // Prune generation and meshing queues from chunks that are no longer desired
        self.job_queue.retain(|job| {
            let chunk = match job {
                ChunkLoaderJob::GenerateChunk(chunk) => chunk,
                ChunkLoaderJob::GenerateMesh(chunk) => chunk,
            };
            let distance = chunk.pos.0.chebyshev_distance(current_chunk_pos.0);
            let should_retain = distance <= (LOAD_DISTANCE as u32);

            if !should_retain {
                // Reset state so that unload_chunks_outside_distance will decrement the correct state.
                // Don't call before_unload here - the chunk will be properly unloaded below.
                match job {
                    ChunkLoaderJob::GenerateChunk(_) => {
                        chunk.set_state(ChunkState::Initial);
                    }
                    ChunkLoaderJob::GenerateMesh(_) => {
                        chunk.set_state(ChunkState::Loaded);
                    }
                }
            }

            should_retain
        });

        // Unload chunks that are now out of range
        // TODO: This might be slow
        let unloaded = self
            .world_access
            .unload_chunks_outside_distance(current_chunk_pos, LOAD_DISTANCE as u32);

        self.event_sender
            .send(ChunkLoaderEvent::ChunksUnloaded(unloaded))
            .unwrap();

        // Enqueue new chunks for generation
        for offset in &self.desired_generation_offsets {
            let chunk_pos = current_chunk_pos + ChunkPos(*offset);
            let chunk = self.world_access.get_handle(chunk_pos);

            let Some(chunk) = chunk else {
                // Chunk doesn't exist in the world, insert an initial chunk and enqueue for generation
                let chunk = self.world_access.insert_initial_chunk(chunk_pos);
                chunk.set_state(ChunkState::InGenerationQueue);
                self.job_queue.push(ChunkLoaderJob::GenerateChunk(chunk));
                continue;
            };

            match chunk.state() {
                ChunkState::Initial => {
                    // Chunk exists in the world but is not yet being generated, enqueue for generation
                    chunk.set_state(ChunkState::InGenerationQueue);
                    self.job_queue.push(ChunkLoaderJob::GenerateChunk(chunk));
                }
                ChunkState::InGenerationQueue | ChunkState::Generating => {
                    // Already in generation queue or being generated, do nothing
                }
                ChunkState::Loaded => {
                    // Chunk is loaded and not in meshing queue
                    // Check neighbor status, and move to meshing queue if ready
                    // TODO: But is this necessary? Chunks are moved to the meshing queue right after generation
                    // Maybe we should have special handling for these kinds of chunks, and not do this all the time
                    let can_mesh = chunk.neighbor_state.is_ready_for_meshing();

                    if can_mesh {
                        chunk.set_state(ChunkState::InMeshingQueue);
                        self.job_queue.push(ChunkLoaderJob::GenerateMesh(chunk));
                    }
                }
                ChunkState::InMeshingQueue
                | ChunkState::Meshing
                | ChunkState::WaitingForRendererFlush => {
                    // Already in meshing queue or being meshed, do nothing
                }
                ChunkState::Ready | ChunkState::ReadyEmpty => {
                    // Chunk is finished, nothing to do
                }
                ChunkState::Unloaded => {
                    // Chunk was unloaded, should not happen since we just got the handle from the map
                    log::warn!(
                        "Encountered Unloaded chunk in on_camera_moved at {:?}",
                        chunk.pos
                    );
                }
            }
        }
    }
}

struct ChunkLoaderWorkerPool {
    _worker_handles: Vec<JoinHandle<()>>,
    job_sender: Sender<ChunkLoaderJob>,
}

impl ChunkLoaderWorkerPool {
    pub fn new<T: IChunkRenderState>(
        num_workers: usize,
        worker_event_sender: Sender<ChunkWorkerEvent<T>>,
        world_generator: Arc<dyn WorldGenerator>,
        block_database: Arc<BlockDatabaseSlim>,
        chunk_access: Arc<dyn WorldAccess<T>>,
        render_context: T::Context,
    ) -> Self {
        let (command_sender, command_receiver) = crossbeam_channel::bounded(num_workers);

        let mut worker_handles = Vec::new();

        for _ in 0..num_workers {
            let world_generator = world_generator.clone();
            let block_database = block_database.clone();
            let chunk_access = chunk_access.clone();
            let render_context = render_context.clone();
            let command_receiver = command_receiver.clone();
            let worker_event_sender = worker_event_sender.clone();

            let handle = std::thread::Builder::new()
                .name("Chunk loader worker".to_string())
                .spawn(move || {
                    let mut worker = ChunkLoaderWorker::new(
                        world_generator,
                        block_database,
                        chunk_access,
                        render_context,
                        command_receiver,
                        worker_event_sender,
                    );

                    worker.process_jobs();
                });
            worker_handles.push(handle.unwrap());
        }

        ChunkLoaderWorkerPool {
            _worker_handles: worker_handles,
            job_sender: command_sender,
        }
    }
}

#[derive(Clone)]
enum ChunkLoaderJob {
    GenerateChunk(ChunkHandle),
    GenerateMesh(ChunkHandle),
}

#[derive(Debug)]
pub struct ChunkMeshUpdate {
    pub handle: ChunkHandle,
    pub id: Option<u64>,
}

struct ChunkLoaderWorker<T: IChunkRenderState> {
    world_generator: Arc<dyn WorldGenerator>,
    mesh_generator: Arc<GreedyMesher>,
    chunk_access: Arc<dyn WorldAccess<T>>,
    render_context: T::Context,
    receiver: Receiver<ChunkLoaderJob>,
    event_sender: Sender<ChunkWorkerEvent<T>>,
    pending_chunks: Vec<ChunkMeshUpdate>,
    last_flush: Instant,
}

impl<T: IChunkRenderState> ChunkLoaderWorker<T> {
    pub fn new(
        world_generator: Arc<dyn WorldGenerator>,
        block_database: Arc<BlockDatabaseSlim>,
        chunk_access: Arc<dyn WorldAccess<T>>,
        render_context: T::Context,
        receiver: Receiver<ChunkLoaderJob>,
        event_sender: Sender<ChunkWorkerEvent<T>>,
    ) -> Self {
        let mesh_generator = Arc::new(GreedyMesher::new(block_database));

        ChunkLoaderWorker {
            world_generator,
            mesh_generator,
            chunk_access,
            render_context,
            receiver,
            event_sender,
            pending_chunks: Vec::new(),
            last_flush: Instant::now(),
        }
    }

    fn generate_chunk(&mut self, chunk: ChunkHandle) {
        chunk.set_state(ChunkState::Generating);
        let data = self.world_generator.generate_chunk(chunk.pos);
        self.chunk_access
            .insert_chunk_data(&chunk, data, &self.event_sender);
        self.event_sender
            .send(ChunkWorkerEvent::ChunkLoaded(chunk))
            .unwrap();
    }

    fn generate_mesh(&mut self, chunk: ChunkHandle, input: ChunkMeshGeneratorInput) {
        let mesh_data = self.mesh_generator.generate_mesh(&input);
        let render_state = T::create_and_upload_mesh(&mut self.render_context, mesh_data);
        let id = render_state.chunk_gpu_id();
        self.chunk_access
            .insert_render_state(input.center_pos, render_state);
        chunk.set_state(ChunkState::WaitingForRendererFlush);
        self.pending_chunks.push(ChunkMeshUpdate {
            handle: chunk,
            id: Some(id),
        });

        if self.pending_chunks.len() >= 16 {
            self.try_flush();
        }
    }

    fn try_flush(&mut self) {
        if self.pending_chunks.is_empty() {
            return;
        }

        log::info!(
            "Flushing {} pending chunk meshes",
            self.pending_chunks.len()
        );

        let pending_meshes = std::mem::take(&mut self.pending_chunks);
        let sender = self.event_sender.clone();

        let results = self.render_context.flush();
        sender
            .send(ChunkWorkerEvent::MeshesGenerated(
                pending_meshes,
                Some(results),
            ))
            .unwrap();

        self.last_flush = Instant::now();
    }

    pub fn process_jobs(&mut self) {
        loop {
            if !self.pending_chunks.is_empty()
                && self.last_flush.elapsed() >= Duration::from_millis(30)
            {
                self.try_flush();
            }

            match self.receiver.recv_timeout(Duration::from_millis(5)) {
                Ok(ChunkLoaderJob::GenerateChunk(chunk)) => {
                    self.generate_chunk(chunk);
                }
                Ok(ChunkLoaderJob::GenerateMesh(chunk)) => {
                    chunk.set_state(ChunkState::Meshing);
                    match self.chunk_access.create_mesh_input(chunk.pos) {
                        Ok(Some(input)) => {
                            self.generate_mesh(chunk, input);
                        }
                        Ok(None) => {
                            chunk.set_state(ChunkState::WaitingForRendererFlush);
                            self.event_sender
                                .send(ChunkWorkerEvent::MeshesGenerated(
                                    vec![ChunkMeshUpdate {
                                        handle: chunk,
                                        id: None,
                                    }],
                                    None,
                                ))
                                .unwrap();
                        }
                        Err(e) => {
                            log::error!(
                                "Failed to create mesh input for chunk {:?}: {:?}",
                                chunk.pos,
                                e
                            );
                        }
                    }
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    // Just loop around to check flush time
                    continue;
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }
    }
}
