use std::{
    collections::HashSet,
    sync::{Arc, RwLock},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender, select};
use glam::IVec3;

use crate::{
    assets::blocks::BlockDatabaseSlim,
    camera::Camera,
    limits::{LOAD_DISTANCE, UNLOAD_DISTANCE},
    loader_job_queue::{JobPriority, JobType, LoaderJobQueue},
    mesh_generation::{
        chunk_mesh_generator_input::{
            ChunkMeshGeneratorInput, MeshGeneratorInputError, MeshGeneratorWarning,
        },
        greedy_mesher::GreedyMesher,
    },
    visibility::generate_desired_chunk_offsets,
    voxels::{
        chunk::{
            Chunk, ChunkData, ChunkHandle, ChunkState, IChunkRenderContext, IChunkRenderState,
        },
        coord::{ChunkPos, WorldPosF},
        face::Face,
    },
    world::WorldChunks,
    worldgen::WorldGenerator,
};

#[derive(Debug)]
pub enum ChunkLoaderEvent<T: IChunkRenderState> {
    ChunkMeshesReady(
        Vec<ChunkMeshUpdate>,
        Option<<T::Context as IChunkRenderContext>::FlushResult>,
    ),
    ChunksUnloaded(Vec<ChunkPos>),
    /// Fast-path for large teleports: the world was cleared and should be treated as rebuilt.
    /// Renderers should drop any cached per-chunk state.
    WorldReset,
}

// Used by the main thread to communicate with the chunk loader thread
pub enum ChunkLoaderCommand {
    Shutdown,
}

// Used by chunk loader workers to communicate back to the chunk loader
pub enum ChunkWorkerEvent {
    /// The chunk is ready to be meshed (all neighbors are present). No further checks are needed.
    ReadyForMeshing(ChunkHandle),
    /// The chunk _might_ be ready for meshing, but the loader should verify its neighbors first.
    /// This is used to retry meshing when a neighbor was missing or in an invalid state when meshing was first attempted.
    PotentiallyReadyForMeshing(ChunkHandle),
}

pub trait WorldAccess<T: IChunkRenderState>: Send + Sync {
    fn exists(&self, pos: ChunkPos) -> bool;
    fn insert_initial_chunk(&self, pos: ChunkPos) -> ChunkHandle;
    /// Computes the neighbor-ready bitmask for `pos` by checking the current world map.
    /// A bit is set if the neighbor exists and is suitable for meshing.
    fn compute_neighbor_mask(&self, pos: ChunkPos) -> u8;
    fn create_mesh_input(
        &self,
        pos: ChunkPos,
    ) -> Result<Option<ChunkMeshGeneratorInput>, MeshGeneratorInputError>;
    /// Inserts voxel data to the chunk and updates its state and neighbor mask.
    /// Also propagates neighbor-ready bits to neighboring chunks.
    /// Return true if the chunk the data was inserted successfully, false if the chunk was not found (likely unloaded).
    fn insert_chunk_data_and_update_neighbor_masks(
        &self,
        chunk: &ChunkHandle,
        data: ChunkData,
        sender: &Sender<ChunkWorkerEvent>,
    ) -> bool;
    fn insert_render_state(&self, pos: ChunkPos, render_state: T);
    /// Unloads and removes the given chunk positions from the world map.
    /// Returns the positions that were actually removed.
    fn unload_chunks(&self, positions: &[ChunkPos]) -> Vec<ChunkPos>;
    fn unload_chunks_outside_distance(&self, center: ChunkPos, distance: u32) -> Vec<ChunkPos>;
    /// Unloads all chunks and clears the world map. Intended for large teleports.
    fn clear_all_chunks(&self);
}

impl<T: IChunkRenderState> WorldAccess<T> for WorldChunks<T> {
    fn exists(&self, pos: ChunkPos) -> bool {
        self.contains_key(&pos)
    }

    fn insert_initial_chunk(&self, pos: ChunkPos) -> ChunkHandle {
        let chunk = Chunk::new(pos);
        let handle = chunk.handle();
        self.insert(pos, chunk);
        handle
    }

    fn compute_neighbor_mask(&self, pos: ChunkPos) -> u8 {
        let mut neighbor_mask = 0u8;

        for face in Face::all().iter().copied() {
            let neighbor_pos = pos.get_neighbor(face);
            let neighbor_is_loaded = self
                .get(&neighbor_pos)
                .map(|c| c.is_suitable_neighbor_for_meshing())
                .unwrap_or(false);

            if neighbor_is_loaded {
                neighbor_mask |= 1 << (face as u8);
            }
        }

        neighbor_mask
    }

    fn create_mesh_input(
        &self,
        pos: ChunkPos,
    ) -> Result<Option<ChunkMeshGeneratorInput>, MeshGeneratorInputError> {
        ChunkMeshGeneratorInput::try_from_map(self, pos)
    }

    fn insert_chunk_data_and_update_neighbor_masks(
        &self,
        chunk: &ChunkHandle,
        data: ChunkData,
        sender: &Sender<ChunkWorkerEvent>,
    ) -> bool {
        if let Some(mut existing) = self.get_mut(&chunk.pos) {
            existing.data = Some(data);
        } else {
            // Chunk was unloaded before data could be inserted, ignore
            return false;
        }

        // Mark this chunk as generated for every neighboring chunk
        let mut neighbor_bits = 0u8;
        for direction in Face::all().iter().copied() {
            let neighbor_pos = chunk.pos.get_neighbor(direction);

            let Some(neighbor_chunk) = self.get(&neighbor_pos) else {
                continue;
            };

            // For each not-unloaded neighbor, mark that neighbor as having this chunk present
            let neighbor_handle = neighbor_chunk.handle();
            if neighbor_handle.state() != ChunkState::Unloaded {
                let neighbor_ready_for_meshing = neighbor_handle
                    .neighbor_state
                    .set_neighbor_ready(direction.opposite());

                // Enqueue the neighbor for meshing if it's now ready
                if neighbor_ready_for_meshing && neighbor_handle.state() == ChunkState::Loaded {
                    sender
                        .send(ChunkWorkerEvent::ReadyForMeshing(neighbor_handle))
                        .unwrap();
                }

                // Track which neighbors are already suitable so we can set our own neighbor mask.
                if neighbor_chunk.is_suitable_neighbor_for_meshing() {
                    neighbor_bits |= 1 << (direction as u8);
                }
            }
        }

        if chunk.try_transition(ChunkState::Generating, ChunkState::Loaded) {
            let ready_for_meshing = chunk.neighbor_state.set_neighbor_bits(neighbor_bits);
            if ready_for_meshing {
                sender
                    .send(ChunkWorkerEvent::ReadyForMeshing(chunk.clone()))
                    .unwrap();
            }
        }

        true
    }

    fn insert_render_state(&self, pos: ChunkPos, render_state: T) {
        if let Some(mut chunk) = self.get_mut(&pos) {
            let state = chunk.state.load();
            if state == ChunkState::Unloaded {
                // Chunk is about to be unloaded, don't insert render state
                return;
            }
            chunk.render_state = Some(render_state);
        }
    }

    fn unload_chunks(&self, positions: &[ChunkPos]) -> Vec<ChunkPos> {
        let mut removed = Vec::new();

        for pos in positions {
            if let Some((pos, chunk)) = self.remove(pos) {
                chunk.unload();
                removed.push(pos);
            }
        }

        if removed.is_empty() {
            return Vec::new();
        }

        // Update neighbor states of remaining chunks
        for removed_pos in &removed {
            for face in Face::all().iter().copied() {
                let neighbor_pos = removed_pos.get_neighbor(face);
                if let Some(neighbor) = self.get(&neighbor_pos) {
                    neighbor
                        .neighbor_state
                        .clear_neighbor_ready(face.opposite());
                }
            }
        }

        removed
    }

    fn unload_chunks_outside_distance(&self, center: ChunkPos, distance: u32) -> Vec<ChunkPos> {
        let mut removed = Vec::new();

        self.retain(|pos, chunk| {
            let dist = pos.0.chebyshev_distance(center.0);
            let should_keep = dist <= distance;
            if !should_keep {
                chunk.unload();
                removed.push(*pos);
            }
            should_keep
        });

        if removed.is_empty() {
            return Vec::new();
        }

        // Update neighbor states of remaining chunks
        for removed_pos in &removed {
            for face in Face::all().iter().copied() {
                let neighbor_pos = removed_pos.get_neighbor(face);
                if let Some(neighbor) = self.get(&neighbor_pos) {
                    neighbor
                        .neighbor_state
                        .clear_neighbor_ready(face.opposite());
                }
            }
        }

        removed
    }

    fn clear_all_chunks(&self) {
        // Mark all chunks as unloaded first so any outstanding handles quickly stop doing work.
        for entry in self.iter() {
            entry.value().unload();
        }

        // Then drop everything from the map. No neighbor updates needed.
        self.clear();
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

/// Manages coordination for chunk loading/meshing.
/// Heavy camera-driven load/unload work runs on a dedicated thread so this coordinator
/// can stay responsive to worker events (notably ReadyForMeshing).
pub struct ChunkLoader {
    command_receiver: Receiver<ChunkLoaderCommand>,
    worker_event_receiver: Receiver<ChunkWorkerEvent>,
    job_queue: Arc<LoaderJobQueue>,
    camera: Arc<RwLock<Camera>>,
}

impl ChunkLoader {
    pub fn start<T: IChunkRenderState>(
        world_generator: Box<dyn WorldGenerator>,
        block_database: Arc<BlockDatabaseSlim>,
        world_access: Arc<dyn WorldAccess<T>>,
        render_context: T::Context,
    ) -> ChunkLoaderHandle<T> {
        let job_queue = Arc::new(LoaderJobQueue::new(UNLOAD_DISTANCE as u32));
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

                let (camera_shutdown_sender, camera_shutdown_receiver) =
                    crossbeam_channel::bounded::<()>(1);

                let worker_pool = ChunkLoaderWorkerPool::new(
                    // TODO: use num_cpus crate
                    16,
                    worker_event_sender,
                    event_sender.clone(),
                    job_queue.clone(),
                    world_generator,
                    block_database,
                    world_access.clone(),
                    render_context,
                );

                // Dedicated thread for heavy camera-driven load/unload and enqueue operations.
                // Keeps the coordinator responsive to worker events (notably ReadyForMeshing).
                let camera_thread = {
                    let job_queue = job_queue.clone();
                    let camera = camera_clone.clone();
                    let world_access = world_access.clone();
                    let event_sender = event_sender.clone();
                    std::thread::Builder::new()
                        .name("Chunk loader camera".to_string())
                        .spawn(move || {
                            let mut camera_worker = ChunkLoaderCameraWorker {
                                event_sender,
                                world_access,
                                camera_moved_receiver,
                                desired_generation_offsets: offsets,
                                job_queue,
                                camera,

                                previous_chunk_pos: ChunkPos::new(0, 0, 0),
                                pending_chunk_pos: None,
                                pending_chunk_pos_since: Instant::now(),

                                boundary_slab_scratch: HashSet::new(),
                                boundary_slab_batch: Vec::new(),
                                generation_job_batch: Vec::new(),
                            };

                            // Do an initial pass so we enqueue around spawn without requiring a move.
                            camera_worker.on_camera_moved();
                            camera_worker.run(camera_shutdown_receiver);
                        })
                        .unwrap()
                };

                let mut chunk_loader = ChunkLoader {
                    command_receiver,
                    worker_event_receiver,
                    job_queue,
                    camera: camera_clone,
                };

                drop(worker_pool);
                drop(camera_thread);

                chunk_loader.run(camera_shutdown_sender)
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

    fn get_priority_for_job(&self, pos: ChunkPos, job_type: JobType) -> JobPriority {
        let camera_chunk_pos = {
            let camera = self.camera.read().unwrap();
            WorldPosF(camera.eye).to_chunk_pos()
        };
        let distance_in_chunks = pos.0.chebyshev_distance(camera_chunk_pos.0);
        JobPriority {
            distance_in_chunks,
            job_type,
        }
    }

    fn run(&mut self, camera_shutdown_sender: Sender<()>) {
        loop {
            select! {
                recv(self.command_receiver) -> command => {
                    match command {
                        Err(_) | Ok(ChunkLoaderCommand::Shutdown) => {
                            let _ = camera_shutdown_sender.try_send(());
                            break;
                        },
                    }
                }
                recv(self.worker_event_receiver) -> event => {
                    match event {
                        Ok(ChunkWorkerEvent::ReadyForMeshing(chunk)) => {
                            if chunk.try_transition(ChunkState::Loaded, ChunkState::InMeshingQueue) {
                                let priority = self.get_priority_for_job(chunk.pos, JobType::Meshing);
                                self.job_queue.push(ChunkLoaderJob::GenerateMesh(chunk), priority);
                            }
                        }
                        Ok(ChunkWorkerEvent::PotentiallyReadyForMeshing(chunk)) => {
                            if chunk.neighbor_state.is_ready_for_meshing()
                                && chunk.try_transition(ChunkState::Loaded, ChunkState::InMeshingQueue)
                            {
                                let priority = self.get_priority_for_job(chunk.pos, JobType::Meshing);
                                self.job_queue.push(ChunkLoaderJob::GenerateMesh(chunk), priority);
                            }
                        }
                        Err(_) => {
                            // Channel closed, should not happen
                            break;
                        }
                    }
                }
            }
        }
    }
}

struct ChunkLoaderCameraWorker<T: IChunkRenderState> {
    event_sender: Sender<ChunkLoaderEvent<T>>,
    world_access: Arc<dyn WorldAccess<T>>,
    camera_moved_receiver: Receiver<()>,
    desired_generation_offsets: Vec<IVec3>,
    job_queue: Arc<LoaderJobQueue>,
    camera: Arc<RwLock<Camera>>,

    previous_chunk_pos: ChunkPos,
    pending_chunk_pos: Option<ChunkPos>,
    pending_chunk_pos_since: Instant,

    boundary_slab_scratch: HashSet<ChunkPos>,
    boundary_slab_batch: Vec<ChunkPos>,
    generation_job_batch: Vec<(ChunkLoaderJob, JobPriority)>,
}

impl<T: IChunkRenderState> ChunkLoaderCameraWorker<T> {
    const MAX_GENERATION_JOB_BATCH: usize = 16 * 1024;

    fn flush_generation_jobs(&mut self, total_enqueued: &mut usize, batches: &mut usize) {
        if self.generation_job_batch.is_empty() {
            return;
        }

        let count = self.generation_job_batch.len();
        *total_enqueued += count;
        *batches += 1;

        self.job_queue
            .push_batch(self.generation_job_batch.drain(..));
    }

    fn run(&mut self, shutdown_receiver: Receiver<()>) {
        loop {
            select! {
                recv(shutdown_receiver) -> _ => {
                    break;
                }
                recv(self.camera_moved_receiver) -> msg => {
                    if msg.is_err() {
                        break;
                    }
                    self.on_camera_moved();
                }
            }
        }
    }

    fn for_each_boundary_slab_position(
        &mut self,
        center: IVec3,
        delta: IVec3,
        radius: i32,
        direction_multiplier: i32,
        mut f: impl FnMut(&mut Self, ChunkPos),
    ) {
        debug_assert!(direction_multiplier == 1 || direction_multiplier == -1);

        let span = (2 * radius + 1) as usize;
        let mut positions = std::mem::take(&mut self.boundary_slab_scratch);
        positions.clear();
        positions.reserve(span * span * 3);

        if delta.x != 0 {
            let x_plane = center.x + direction_multiplier * radius * delta.x.signum();
            for y in -radius..=radius {
                for z in -radius..=radius {
                    positions.insert(ChunkPos::new(x_plane, center.y + y, center.z + z));
                }
            }
        }

        if delta.y != 0 {
            let y_plane = center.y + direction_multiplier * radius * delta.y.signum();
            for x in -radius..=radius {
                for z in -radius..=radius {
                    positions.insert(ChunkPos::new(center.x + x, y_plane, center.z + z));
                }
            }
        }

        if delta.z != 0 {
            let z_plane = center.z + direction_multiplier * radius * delta.z.signum();
            for x in -radius..=radius {
                for y in -radius..=radius {
                    positions.insert(ChunkPos::new(center.x + x, center.y + y, z_plane));
                }
            }
        }

        for pos in positions.drain() {
            f(self, pos);
        }

        self.boundary_slab_scratch = positions;
    }

    // When the camera moves (and on startup), we need to potentially load/unload chunks
    fn on_camera_moved(&mut self) {
        let current_chunk_pos = {
            let camera = self.camera.read().unwrap();
            WorldPosF(camera.eye).to_chunk_pos()
        };

        // Apply debounce to avoid rapid chunk loading/unloading when the camera is near chunk boundaries
        const CHUNK_POS_DEBOUNCE: Duration = Duration::from_millis(16);
        if current_chunk_pos != self.previous_chunk_pos {
            match self.pending_chunk_pos {
                Some(pending) if pending == current_chunk_pos => {
                    if self.pending_chunk_pos_since.elapsed() < CHUNK_POS_DEBOUNCE {
                        return;
                    }
                }
                _ => {
                    self.pending_chunk_pos = Some(current_chunk_pos);
                    self.pending_chunk_pos_since = Instant::now();
                    return;
                }
            }
        }

        // Either stable, or the debounce period elapsed
        self.pending_chunk_pos = None;

        // Camera moved but chunk didn't change, nothing to do
        if current_chunk_pos == self.previous_chunk_pos {
            return;
        }

        let previous_chunk_pos = self.previous_chunk_pos;
        let delta = current_chunk_pos.0 - previous_chunk_pos.0;
        let small_step = delta.x.abs() <= 1 && delta.y.abs() <= 1 && delta.z.abs() <= 1;
        let jump_distance = delta.abs().max_element();
        // If the camera moved more than twice the unload distance, the kept regions don't overlap.
        // In that case it's faster to clear everything and rebuild.
        let no_overlap_teleport = !small_step && jump_distance > 2 * UNLOAD_DISTANCE;

        self.previous_chunk_pos = current_chunk_pos;

        // Unload chunks that are now out of range.
        if no_overlap_teleport {
            let removed_jobs = self.job_queue.clear();
            self.world_access.clear_all_chunks();

            log::info!(
                "Large teleport detected ({}) chunks: cleared world and {} queued jobs",
                jump_distance,
                removed_jobs
            );

            self.event_sender
                .send(ChunkLoaderEvent::WorldReset)
                .unwrap();
        } else {
            let unloaded = if small_step {
                self.boundary_slab_batch.clear();
                self.for_each_boundary_slab_position(
                    previous_chunk_pos.0,
                    delta,
                    UNLOAD_DISTANCE,
                    -1,
                    |this, pos| this.boundary_slab_batch.push(pos),
                );
                self.world_access.unload_chunks(&self.boundary_slab_batch)
            } else {
                // Fallback for large jumps/teleports: scan the whole map.
                self.world_access
                    .unload_chunks_outside_distance(current_chunk_pos, UNLOAD_DISTANCE as u32)
            };

            self.event_sender
                .send(ChunkLoaderEvent::ChunksUnloaded(unloaded))
                .unwrap();
        }

        // Enqueue new chunks for generation
        self.generation_job_batch.clear();
        let mut total_enqueued = 0usize;
        let mut batches = 0usize;

        if small_step {
            self.for_each_boundary_slab_position(
                current_chunk_pos.0,
                delta,
                LOAD_DISTANCE,
                1,
                |this, chunk_pos| {
                    if !this.world_access.exists(chunk_pos) {
                        let distance_in_chunks =
                            chunk_pos.0.chebyshev_distance(current_chunk_pos.0);
                        let priority = JobPriority {
                            distance_in_chunks,
                            job_type: JobType::Generation,
                        };
                        let chunk = this.world_access.insert_initial_chunk(chunk_pos);
                        chunk.set_state(ChunkState::InGenerationQueue);
                        this.generation_job_batch
                            .push((ChunkLoaderJob::GenerateChunk(chunk), priority));

                        if this.generation_job_batch.len() >= Self::MAX_GENERATION_JOB_BATCH {
                            this.flush_generation_jobs(&mut total_enqueued, &mut batches);
                        }
                    }
                },
            );
        } else {
            // Fallback for large jumps/teleports: scan the whole cube.
            let offsets_len = self.desired_generation_offsets.len();
            for i in 0..offsets_len {
                let offset = self.desired_generation_offsets[i];
                let chunk_pos = current_chunk_pos + ChunkPos(offset);
                if !self.world_access.exists(chunk_pos) {
                    let chunk = self.world_access.insert_initial_chunk(chunk_pos);
                    chunk.set_state(ChunkState::InGenerationQueue);
                    let distance_in_chunks = chunk_pos.0.chebyshev_distance(current_chunk_pos.0);
                    let priority = JobPriority {
                        distance_in_chunks,
                        job_type: JobType::Generation,
                    };
                    self.generation_job_batch
                        .push((ChunkLoaderJob::GenerateChunk(chunk), priority));

                    if self.generation_job_batch.len() >= Self::MAX_GENERATION_JOB_BATCH {
                        self.flush_generation_jobs(&mut total_enqueued, &mut batches);
                    }
                }
            }
        }

        self.flush_generation_jobs(&mut total_enqueued, &mut batches);

        if total_enqueued > 0 {
            log::debug!(
                "Enqueued {} chunk generation jobs in {} batches",
                total_enqueued,
                batches
            );
        }
    }
}

struct ChunkLoaderWorkerPool {
    _worker_handles: Vec<JoinHandle<()>>,
}

impl ChunkLoaderWorkerPool {
    #[allow(clippy::too_many_arguments)]
    pub fn new<T: IChunkRenderState>(
        num_workers: usize,
        worker_event_sender: Sender<ChunkWorkerEvent>,
        loader_event_sender: Sender<ChunkLoaderEvent<T>>,
        job_queue: Arc<LoaderJobQueue>,
        world_generator: Arc<dyn WorldGenerator>,
        block_database: Arc<BlockDatabaseSlim>,
        chunk_access: Arc<dyn WorldAccess<T>>,
        render_context: T::Context,
    ) -> Self {
        let mut worker_handles = Vec::new();

        for _ in 0..num_workers {
            let world_generator = world_generator.clone();
            let block_database = block_database.clone();
            let chunk_access = chunk_access.clone();
            let render_context = render_context.clone();
            let worker_event_sender = worker_event_sender.clone();
            let loader_event_sender = loader_event_sender.clone();
            let job_queue = job_queue.clone();

            let handle = std::thread::Builder::new()
                .name("Chunk loader worker".to_string())
                .spawn(move || {
                    let mut worker = ChunkLoaderWorker::new(
                        world_generator,
                        block_database,
                        chunk_access,
                        render_context,
                        job_queue,
                        worker_event_sender,
                        loader_event_sender,
                    );

                    worker.process_jobs();
                });
            worker_handles.push(handle.unwrap());
        }

        ChunkLoaderWorkerPool {
            _worker_handles: worker_handles,
        }
    }
}

#[derive(Clone)]
pub enum ChunkLoaderJob {
    GenerateChunk(ChunkHandle),
    GenerateMesh(ChunkHandle),
}

impl ChunkLoaderJob {
    pub fn chunk_handle(&self) -> &ChunkHandle {
        match self {
            ChunkLoaderJob::GenerateChunk(chunk) => chunk,
            ChunkLoaderJob::GenerateMesh(chunk) => chunk,
        }
    }
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
    job_queue: Arc<LoaderJobQueue>,
    job_notifications: Receiver<()>,
    event_sender: Sender<ChunkWorkerEvent>,
    loader_event_sender: Sender<ChunkLoaderEvent<T>>,
    pending_chunks: Vec<ChunkMeshUpdate>,
    last_flush: Instant,
}

impl<T: IChunkRenderState> ChunkLoaderWorker<T> {
    pub fn new(
        world_generator: Arc<dyn WorldGenerator>,
        block_database: Arc<BlockDatabaseSlim>,
        chunk_access: Arc<dyn WorldAccess<T>>,
        render_context: T::Context,
        job_queue: Arc<LoaderJobQueue>,
        event_sender: Sender<ChunkWorkerEvent>,
        loader_event_sender: Sender<ChunkLoaderEvent<T>>,
    ) -> Self {
        let mesh_generator = Arc::new(GreedyMesher::new(block_database));
        let job_notifications = job_queue.subscribe();

        ChunkLoaderWorker {
            world_generator,
            mesh_generator,
            chunk_access,
            render_context,
            job_queue,
            job_notifications,
            event_sender,
            loader_event_sender,
            pending_chunks: Vec::new(),
            last_flush: Instant::now(),
        }
    }

    fn generate_chunk(&mut self, chunk: ChunkHandle) {
        if !chunk.try_transition(ChunkState::InGenerationQueue, ChunkState::Generating) {
            // Chunk has likely been unloaded while in the generation queue, ignore
            return;
        }

        let data = self.world_generator.generate_chunk(chunk.pos);
        let _ = self
            .chunk_access
            .insert_chunk_data_and_update_neighbor_masks(&chunk, data, &self.event_sender);
    }

    fn generate_mesh(&mut self, chunk: ChunkHandle) {
        if !chunk.try_transition(ChunkState::InMeshingQueue, ChunkState::Meshing) {
            // Chunk has likely been unloaded while in the meshing queue, ignore
            return;
        }

        let input = match self.chunk_access.create_mesh_input(chunk.pos) {
            Ok(input) => input,
            Err(err) => {
                // We failed to create mesh input, handle the error
                match err {
                    MeshGeneratorInputError::Warning(warning) => {
                        match warning {
                            MeshGeneratorWarning::ChunkMissing { .. } => {
                                // The chunk was likely unloaded while waiting for meshing
                                // Nothing to do here
                            }
                            MeshGeneratorWarning::NeighborMissing { .. }
                            | MeshGeneratorWarning::NeighborInInvalidState { .. } => {
                                // Since we tried to mesh but a neighbor is missing or invalid, the most likely explanation is that
                                // the neighbor was unloaded while waiting for meshing.
                                // Roll back the chunk to Loaded state and re-enqueue it for meshing
                                if chunk.try_transition(ChunkState::Meshing, ChunkState::Loaded) {
                                    self.event_sender
                                        .send(ChunkWorkerEvent::PotentiallyReadyForMeshing(chunk))
                                        .unwrap();
                                }
                            }
                        }
                    }
                    MeshGeneratorInputError::Fatal(err) => {
                        log::error!(
                            "Failed to create mesh input for chunk {:?}: {:?}",
                            chunk.pos,
                            err
                        );
                    }
                }

                return;
            }
        };

        let Some(input) = input else {
            // The chunk doesn't need a mesh, since it's either empty or fully occluded.
            // Mark it as ready-empty and enqueue for renderer flush.
            if chunk.try_transition(ChunkState::Meshing, ChunkState::WaitingForRendererFlush) {
                self.loader_event_sender
                    .send(ChunkLoaderEvent::ChunkMeshesReady(
                        vec![ChunkMeshUpdate {
                            handle: chunk,
                            id: None,
                        }],
                        None,
                    ))
                    .unwrap();
            }
            return;
        };

        let mesh_data = self.mesh_generator.generate_mesh(&input);
        let render_state = T::create_and_upload_mesh(&mut self.render_context, mesh_data);
        let id = render_state.chunk_gpu_id();

        if chunk.try_transition(ChunkState::Meshing, ChunkState::WaitingForRendererFlush) {
            self.chunk_access
                .insert_render_state(input.center_pos, render_state);
            self.pending_chunks.push(ChunkMeshUpdate {
                handle: chunk,
                id: Some(id),
            });
        }
    }

    fn flush_pending(&mut self) {
        log::debug!(
            "Flushing {} pending chunk meshes",
            self.pending_chunks.len()
        );

        let pending_meshes = std::mem::take(&mut self.pending_chunks);
        let results = self.render_context.flush();
        self.loader_event_sender
            .send(ChunkLoaderEvent::ChunkMeshesReady(
                pending_meshes,
                Some(results),
            ))
            .unwrap();

        self.last_flush = Instant::now();
    }

    pub fn process_jobs(&mut self) {
        loop {
            if let Some(job) = self.job_queue.pop() {
                let position = match &job {
                    ChunkLoaderJob::GenerateChunk(chunk) => chunk.pos,
                    ChunkLoaderJob::GenerateMesh(chunk) => chunk.pos,
                };

                // Verify the chunk still exists
                if !self.chunk_access.exists(position) {
                    continue;
                }

                match job {
                    ChunkLoaderJob::GenerateChunk(chunk) => {
                        self.generate_chunk(chunk);
                    }
                    ChunkLoaderJob::GenerateMesh(chunk) => {
                        self.generate_mesh(chunk);
                    }
                }

                let should_flush_time = self.last_flush.elapsed() >= Duration::from_millis(30);
                let should_flush_count = self.pending_chunks.len() >= 512;

                if !self.pending_chunks.is_empty() && (should_flush_time || should_flush_count) {
                    self.flush_pending();
                }
            } else {
                if !self.pending_chunks.is_empty() {
                    // We have time, so flush any pending meshes
                    self.flush_pending();
                }

                // Block until notified of new jobs
                let _ = self
                    .job_notifications
                    .recv_timeout(Duration::from_millis(5));
            }
        }
    }
}
