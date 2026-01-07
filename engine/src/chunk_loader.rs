use std::{
    collections::HashSet,
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
        chunk::{Chunk, ChunkData, ChunkState, IChunkRenderContext, IChunkRenderState},
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
    ChunkLoaded {
        pos: ChunkPos,
    },
    ChunkReadyForMeshing {
        pos: ChunkPos,
    },
    MeshesGenerated(
        Vec<ChunkMeshUpdate>,
        Option<<T::Context as IChunkRenderContext>::FlushResult>,
    ),
}

pub trait WorldAccess<T: IChunkRenderState>: Send + Sync {
    fn is_chunk_ready_for_meshing(&self, pos: ChunkPos) -> bool;
    fn create_mesh_input(&self, pos: ChunkPos) -> anyhow::Result<Option<ChunkMeshGeneratorInput>>;
    fn insert_chunk_data(
        &self,
        pos: ChunkPos,
        data: ChunkData,
        sender: &Sender<ChunkWorkerEvent<T>>,
    );
    fn insert_render_state(&self, pos: ChunkPos, render_state: T);
    fn remove_chunk(&self, pos: ChunkPos);
    fn unload_chunks_outside_distance(&self, center: ChunkPos, distance: u32) -> Vec<ChunkPos>;
    fn get_chunk_state(&self, pos: ChunkPos) -> Option<ChunkState>;
    fn set_chunk_state(&self, pos: ChunkPos, state: ChunkState);
}

impl<T: IChunkRenderState> WorldAccess<T> for DashMap<ChunkPos, Chunk<T>> {
    fn is_chunk_ready_for_meshing(&self, pos: ChunkPos) -> bool {
        let Some(chunk) = self.get(&pos) else {
            return false;
        };

        if chunk.state != ChunkState::Loaded {
            return false;
        }

        let neighbors = pos.get_neighbors();

        for neighbor_pos in neighbors {
            let Some(neighbor_chunk) = self.get(&neighbor_pos) else {
                return false;
            };

            if !neighbor_chunk.is_suitable_neighbor_for_meshing() {
                return false;
            }
        }

        true
    }

    fn create_mesh_input(&self, pos: ChunkPos) -> anyhow::Result<Option<ChunkMeshGeneratorInput>> {
        ChunkMeshGeneratorInput::try_from_map(self, pos)
    }

    fn insert_chunk_data(
        &self,
        pos: ChunkPos,
        data: ChunkData,
        sender: &Sender<ChunkWorkerEvent<T>>,
    ) {
        self.insert(pos, Chunk::from_data(pos, data));

        // Mark this chunk as generated for every neighboring chunk
        let mut neighbor_bits = 0u8;
        for direction in Face::all().iter().copied() {
            let neighbor_pos = pos.get_neighbor(direction);
            if let Some(neighbor) = self.get(&neighbor_pos) {
                let neighbor_ready_for_meshing = neighbor
                    .neighbor_status
                    .set_neighbor_ready(direction.opposite());

                if neighbor_ready_for_meshing {
                    sender
                        .send(ChunkWorkerEvent::ChunkReadyForMeshing { pos: neighbor_pos })
                        .unwrap();
                }

                neighbor_bits |= 1 << (direction as u8);
            }
        }

        // Update neighbor status of this chunk
        let ready_for_meshing = if let Some(chunk) = self.get(&pos) {
            chunk.neighbor_status.set_neighbor_bits(neighbor_bits)
        } else {
            false
        };

        if ready_for_meshing {
            sender
                .send(ChunkWorkerEvent::ChunkReadyForMeshing { pos })
                .unwrap();
        }
    }

    fn remove_chunk(&self, pos: ChunkPos) {
        self.remove(&pos);
    }

    fn insert_render_state(&self, pos: ChunkPos, render_state: T) {
        if let Some(mut chunk) = self.get_mut(&pos) {
            chunk.render_state = Some(render_state);
            chunk.state = ChunkState::Ready;
        }
    }

    fn unload_chunks_outside_distance(&self, center: ChunkPos, distance: u32) -> Vec<ChunkPos> {
        let mut removed = Vec::new();
        self.retain(|pos, _chunk| {
            let dist = pos.0.chebyshev_distance(center.0);
            let keep = dist <= distance;
            if !keep {
                removed.push(*pos);
            }
            keep
        });
        removed
    }

    fn get_chunk_state(&self, pos: ChunkPos) -> Option<ChunkState> {
        self.get(&pos).map(|chunk| chunk.state)
    }

    fn set_chunk_state(&self, pos: ChunkPos, state: ChunkState) {
        if let Some(mut chunk) = self.get_mut(&pos) {
            chunk.state = state;
        }
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
    previous_chunk: ChunkPos,
    world_access: Arc<dyn WorldAccess<T>>,
    camera_moved_receiver: Receiver<()>,
    worker_pool: ChunkLoaderWorkerPool,
    desired_generation_offsets: Vec<IVec3>,
    job_queue: Vec<ChunkLoaderJob>,
    pending_chunks: HashSet<ChunkPos>,
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
                    previous_chunk: ChunkPos::new(0, 0, 0),
                    world_access,
                    camera_moved_receiver,
                    desired_generation_offsets: offsets,
                    worker_pool,
                    job_queue: Vec::new(),
                    camera: camera_clone,
                    pending_chunks: HashSet::new(),
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
                        Ok(ChunkWorkerEvent::ChunkLoaded { pos }) => {
                            self.pending_chunks.remove(&pos);
                        }
                        Ok(ChunkWorkerEvent::ChunkReadyForMeshing { pos }) => {
                            self.job_queue.push(ChunkLoaderJob::GenerateMesh(pos));
                        }
                        Ok(ChunkWorkerEvent::MeshesGenerated(updates, flush_results)) => {
                            for update in &updates {
                                self.pending_chunks.remove(&update.pos);
                            }
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
                default => {
                    self.give_jobs_to_workers();
                }
            }
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
        let camera = self.camera.read().unwrap();
        let current_chunk = WorldPosF(camera.eye).to_chunk_pos();

        // Camera moved but chunk didn't change, nothing to do
        if current_chunk == self.previous_chunk {
            return;
        }

        self.previous_chunk = current_chunk;

        // Prune generation and meshing queues from chunks that are no longer desired
        self.job_queue.retain(|jobs| {
            let pos = match jobs {
                ChunkLoaderJob::GenerateChunk(chunk_pos) => *chunk_pos,
                ChunkLoaderJob::GenerateMesh(chunk_pos) => *chunk_pos,
            };
            let distance = pos.0.chebyshev_distance(current_chunk.0);
            distance <= (LOAD_DISTANCE as u32)
        });

        // Unload chunks that are now out of range
        // TODO: This might be slow
        let unloaded = self
            .world_access
            .unload_chunks_outside_distance(current_chunk, LOAD_DISTANCE as u32);

        self.event_sender
            .send(ChunkLoaderEvent::ChunksUnloaded(unloaded))
            .unwrap();

        // Enqueue new chunks for generation
        for offset in &self.desired_generation_offsets {
            let chunk_pos = current_chunk + ChunkPos(*offset);
            let chunk_state = self.world_access.get_chunk_state(chunk_pos);

            match chunk_state {
                Some(ChunkState::Ready)
                | Some(ChunkState::Meshing)
                | Some(ChunkState::Generating)
                | Some(ChunkState::Loaded) => {
                    // Chunk is already loaded or being processed, do nothing
                }
                Some(ChunkState::WaitingForLoading) | None => {
                    if !self.pending_chunks.contains(&chunk_pos) {
                        self.job_queue
                            .push(ChunkLoaderJob::GenerateChunk(chunk_pos));
                        self.pending_chunks.insert(chunk_pos);
                    }
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
        let (command_sender, command_receiver) = crossbeam_channel::bounded(num_workers * 2);

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
    GenerateChunk(ChunkPos),
    GenerateMesh(ChunkPos),
}

#[derive(Debug)]
pub struct ChunkMeshUpdate {
    pub pos: ChunkPos,
    pub id: Option<u64>,
}

struct ChunkLoaderWorker<T: IChunkRenderState> {
    world_generator: Arc<dyn WorldGenerator>,
    mesh_generator: Arc<GreedyMesher>,
    chunk_access: Arc<dyn WorldAccess<T>>,
    render_context: T::Context,
    receiver: Receiver<ChunkLoaderJob>,
    event_sender: Sender<ChunkWorkerEvent<T>>,
    pending_meshes: Vec<ChunkMeshUpdate>,
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
            pending_meshes: Vec::new(),
            last_flush: Instant::now(),
        }
    }

    fn try_flush(&mut self) {
        if self.pending_meshes.is_empty() {
            return;
        }

        log::info!(
            "Flushing {} pending chunk meshes",
            self.pending_meshes.len()
        );

        let pending_meshes = std::mem::take(&mut self.pending_meshes);
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

    fn generate_mesh(&mut self, input: ChunkMeshGeneratorInput) {
        let mesh_data = self.mesh_generator.generate_mesh(&input);
        let render_state = T::create_and_upload_mesh(&mut self.render_context, mesh_data);
        let id = render_state.chunk_gpu_id();
        self.chunk_access
            .insert_render_state(input.center_pos, render_state);

        self.pending_meshes.push(ChunkMeshUpdate {
            pos: input.center_pos,
            id: Some(id),
        });

        if self.pending_meshes.len() >= 16 {
            self.try_flush();
        }
    }

    fn generate_chunk(&mut self, pos: ChunkPos) {
        let data = self.world_generator.generate_chunk(pos);
        self.chunk_access
            .insert_chunk_data(pos, data, &self.event_sender);
        self.event_sender
            .send(ChunkWorkerEvent::ChunkLoaded { pos })
            .unwrap();
    }

    pub fn process_jobs(&mut self) {
        loop {
            if !self.pending_meshes.is_empty()
                && self.last_flush.elapsed() >= Duration::from_millis(30)
            {
                self.try_flush();
            }

            match self.receiver.recv_timeout(Duration::from_millis(5)) {
                Ok(ChunkLoaderJob::GenerateChunk(pos)) => {
                    self.generate_chunk(pos);
                }
                Ok(ChunkLoaderJob::GenerateMesh(pos)) => {
                    match self.chunk_access.create_mesh_input(pos) {
                        Ok(Some(input)) => {
                            self.generate_mesh(input);
                        }
                        Ok(None) => {
                            self.chunk_access.set_chunk_state(pos, ChunkState::Ready);
                            self.event_sender
                                .send(ChunkWorkerEvent::MeshesGenerated(
                                    vec![ChunkMeshUpdate { pos, id: None }],
                                    None,
                                ))
                                .unwrap();
                        }
                        Err(e) => {
                            log::error!("Failed to create mesh input for chunk {:?}: {:?}", pos, e);
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
