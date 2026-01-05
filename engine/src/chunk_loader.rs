use std::{
    collections::HashSet,
    sync::{Arc, RwLock},
    thread::JoinHandle,
};

use crossbeam_channel::{Receiver, Sender, select};
use glam::IVec3;

use crate::{
    voxels::{chunk::Chunk, coord::ChunkPos},
    worldgen::WorldGenerator,
};

pub enum ChunkLoaderCommand {
    Shutdown,
    CurrentChunkChanged,
    ChunkLoaded(ChunkPos),
}

pub enum ChunkLoaderEvent {
    Loaded { pos: ChunkPos, chunk: Box<Chunk> },
    ShouldUnload(Vec<ChunkPos>),
}

/// Manages loading and unloading of chunks in the world.
/// Loading can involve generating new chunks or loading from disk.
/// The chunk loader lives in its own thread and communicates with the main engine thread via channels.
pub struct ChunkLoader {
    chunk_event_receiver: Receiver<ChunkLoaderEvent>,
    command_sender: Sender<ChunkLoaderCommand>,
    current_chunk: Arc<RwLock<ChunkPos>>,
    _worker_thread: JoinHandle<()>,
}

impl ChunkLoader {
    pub fn new(
        world_generator: Box<dyn WorldGenerator>,
        initial_loaded_chunks: Vec<ChunkPos>,
    ) -> Self {
        let (chunk_event_sender, chunk_event_receiver) = crossbeam_channel::unbounded();

        let current_chunk = Arc::new(RwLock::new(ChunkPos(IVec3::ZERO)));
        let current_chunk_clone = current_chunk.clone();

        let (worker, command_sender) = ChunkLoaderWorker::new(
            world_generator,
            chunk_event_sender,
            initial_loaded_chunks,
            current_chunk_clone,
        );

        let worker_thread = std::thread::Builder::new()
            .name("Chunk loader worker".to_string())
            .spawn(move || {
                let mut worker = worker;
                worker.handle_events();
            })
            .unwrap();

        ChunkLoader {
            current_chunk,
            chunk_event_receiver,
            command_sender,
            _worker_thread: worker_thread,
        }
    }

    pub fn receiver(&self) -> Receiver<ChunkLoaderEvent> {
        self.chunk_event_receiver.clone()
    }

    pub fn update_camera_position(&mut self, new_position: ChunkPos) {
        let current_pos = *self.current_chunk.read().unwrap();

        if current_pos == new_position {
            return;
        }

        {
            let mut pos_lock = self.current_chunk.write().unwrap();
            *pos_lock = new_position;
        }

        self.command_sender
            .send(ChunkLoaderCommand::CurrentChunkChanged)
            .unwrap();
    }
}

struct ChunkLoaderWorker {
    world_generator: Arc<dyn WorldGenerator>,
    current_chunk: Arc<RwLock<ChunkPos>>,
    command_sender: Sender<ChunkLoaderCommand>,
    command_receiver: Receiver<ChunkLoaderCommand>,
    pending_chunks: HashSet<ChunkPos>,
    loaded_chunks: HashSet<ChunkPos>,
    chunk_event_sender: Sender<ChunkLoaderEvent>,
    offsets: Vec<ChunkPos>,
}

impl ChunkLoaderWorker {
    // TODO: Make configurable
    const VIEW_DISTANCE: i32 = 12;
    const VIEW_DISTANCE_Y: i32 = 6;

    pub fn new(
        world_generator: Box<dyn WorldGenerator>,
        chunk_event_sender: Sender<ChunkLoaderEvent>,
        initial_loaded_chunks: Vec<ChunkPos>,
        current_chunk: Arc<RwLock<ChunkPos>>,
    ) -> (Self, Sender<ChunkLoaderCommand>) {
        let (command_sender, command_receiver) = crossbeam_channel::unbounded();

        // TODO: Recompute this if VIEW_DISTANCE changes
        let offsets = (-Self::VIEW_DISTANCE_Y..=Self::VIEW_DISTANCE_Y)
            .flat_map(move |y| {
                (-Self::VIEW_DISTANCE..=Self::VIEW_DISTANCE).flat_map(move |z| {
                    (-Self::VIEW_DISTANCE..=Self::VIEW_DISTANCE)
                        .map(move |x| ChunkPos(IVec3::new(x, y, z)))
                })
            })
            .collect::<Vec<_>>();

        let loaded_chunks = initial_loaded_chunks.into_iter().collect::<HashSet<_>>();

        (
            ChunkLoaderWorker {
                world_generator: Arc::from(world_generator),
                current_chunk,
                command_sender: command_sender.clone(),
                command_receiver,
                pending_chunks: HashSet::new(),
                loaded_chunks,
                chunk_event_sender,
                offsets,
            },
            command_sender,
        )
    }

    pub fn handle_events(&mut self) {
        loop {
            select! {
                recv(self.command_receiver) -> command => {
                    match command {
                        Ok(ChunkLoaderCommand::CurrentChunkChanged) => {
                            // Current chunk changed, empty channel of pending updates and update loaded chunks
                            while let Ok(message) = self.command_receiver.try_recv() {
                                match message {
                                    ChunkLoaderCommand::CurrentChunkChanged => {
                                        // Ignore, already handling
                                    }
                                    ChunkLoaderCommand::Shutdown => {
                                        return;
                                    }
                                    ChunkLoaderCommand::ChunkLoaded(pos) => {
                                        self.process_chunk_loaded(pos);
                                    }
                                }
                            }

                            let new_pos = *self.current_chunk.read().unwrap();
                            self.update_loaded_chunks(new_pos);
                        }
                        Ok(ChunkLoaderCommand::Shutdown) | Err(_) => {
                            return;
                        }
                        Ok(ChunkLoaderCommand::ChunkLoaded(pos)) => {
                            self.process_chunk_loaded(pos);
                        }
                    }
                }
            }
        }
    }

    fn process_chunk_loaded(&mut self, pos: ChunkPos) {
        self.pending_chunks.remove(&pos);
        self.loaded_chunks.insert(pos);
    }

    fn update_loaded_chunks(&mut self, new_position: ChunkPos) {
        let mut chunks_to_load = Vec::new();

        let desired_chunks = self
            .offsets
            .iter()
            .map(|offset| ChunkPos(new_position.0 + offset.0))
            .collect::<HashSet<_>>();

        for chunk in &desired_chunks {
            // Already loading or loaded, do nothing
            if self.pending_chunks.contains(chunk) || self.loaded_chunks.contains(chunk) {
                continue;
            }

            // Not loaded yet, mark for loading
            chunks_to_load.push(*chunk);
            self.pending_chunks.insert(*chunk);
        }

        let chunks_to_unload = self
            .loaded_chunks
            .difference(&desired_chunks)
            .cloned()
            .collect::<Vec<_>>();

        if !chunks_to_load.is_empty() {
            log::debug!("Loading {} chunks", chunks_to_load.len());
        }

        // For chunks to load, start Rayon jobs to generate them
        for chunk_pos in chunks_to_load {
            let sender = self.chunk_event_sender.clone();
            let job_sender = self.command_sender.clone();
            self.pending_chunks.insert(chunk_pos);
            let generator = self.world_generator.clone();
            rayon::spawn(move || {
                let chunk = generator.generate_chunk(chunk_pos);
                sender
                    .send(ChunkLoaderEvent::Loaded {
                        pos: chunk_pos,
                        chunk: Box::new(chunk),
                    })
                    .unwrap();
                job_sender
                    .send(ChunkLoaderCommand::ChunkLoaded(chunk_pos))
                    .unwrap();
            });
        }

        // For chunks to unload, just send an event and let other systems handle it
        if !chunks_to_unload.is_empty() {
            self.chunk_event_sender
                .send(ChunkLoaderEvent::ShouldUnload(chunks_to_unload))
                .unwrap();
        }

        self.loaded_chunks
            .retain(|pos| desired_chunks.contains(pos));
    }
}
