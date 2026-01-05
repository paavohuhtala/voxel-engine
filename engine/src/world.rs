use std::collections::HashSet;

use crate::{
    chunk_loader::{ChunkLoader, ChunkLoaderEvent},
    voxels::{
        chunk::Chunk,
        coord::{ChunkPos, WorldPos},
        voxel::Voxel,
    },
    worldgen::WorldGenerator,
};

use crossbeam_channel::Receiver;
use dashmap::DashMap;

pub struct World {
    pub chunk_loader: ChunkLoader,
    pub chunks: DashMap<ChunkPos, Chunk>,
    chunk_loader_receiver: Receiver<ChunkLoaderEvent>,

    ready_for_meshing: HashSet<ChunkPos>,
    ready_to_unload: HashSet<ChunkPos>,
}

impl World {
    pub fn from_generator(generator: impl WorldGenerator) -> Self {
        let chunk_loader = ChunkLoader::new(Box::new(generator), Vec::new());
        let chunk_loader_receiver = chunk_loader.receiver();
        World {
            chunk_loader,
            chunks: DashMap::new(),
            chunk_loader_receiver,
            ready_for_meshing: HashSet::new(),
            ready_to_unload: HashSet::new(),
        }
    }

    pub fn from_chunks(generator: impl WorldGenerator, chunks: Vec<(ChunkPos, Chunk)>) -> Self {
        let chunks = chunks.into_iter().collect::<DashMap<ChunkPos, Chunk>>();
        let initial_loaded_chunks = chunks.iter().map(|entry| *entry.key()).collect();
        let chunk_loader = ChunkLoader::new(Box::new(generator), initial_loaded_chunks);
        let chunk_loader_receiver = chunk_loader.receiver();
        World {
            chunk_loader,
            chunks,
            chunk_loader_receiver,
            ready_for_meshing: HashSet::new(),
            ready_to_unload: HashSet::new(),
        }
    }

    pub fn set_voxel(&self, position: WorldPos, voxel: Voxel) {
        let chunk_id = position.to_chunk_pos();
        let mut chunk = self
            .chunks
            .entry(chunk_id)
            .or_insert_with(|| Chunk::Solid(Voxel::AIR));
        let local_pos = position.to_local_pos();
        chunk.set_voxel(local_pos, voxel);
    }

    pub fn get_voxel(&self, position: WorldPos) -> Option<Voxel> {
        let chunk_id = position.to_chunk_pos();
        let chunk = self.chunks.get(&chunk_id)?;
        let local_pos = position.to_local_pos();
        chunk.get_voxel(local_pos)
    }

    pub fn update(&mut self) {
        self.add_generated_chunks();
    }

    fn is_chunk_ready_for_meshing(&self, pos: ChunkPos) -> bool {
        // A chunk is ready for meshing if it exists and all its neighbors exist
        if !self.chunks.contains_key(&pos) {
            return false;
        }

        let neighbors = pos.get_neighbors();

        for neighbor_pos in neighbors {
            if !self.chunks.contains_key(&neighbor_pos) {
                return false;
            }
        }

        true
    }

    fn add_generated_chunks(&mut self) {
        let mut added = 0;
        let mut removed = 0;

        while let Ok(event) = self.chunk_loader_receiver.try_recv() {
            match event {
                ChunkLoaderEvent::Loaded { pos, chunk } => {
                    added += 1;
                    self.chunks.insert(pos, *chunk);

                    if self.is_chunk_ready_for_meshing(pos) {
                        self.ready_for_meshing.insert(pos);
                    }

                    // Also check if any neighbors are now ready for meshing because of this new chunk
                    for neighbor in pos.get_neighbors() {
                        if self.is_chunk_ready_for_meshing(neighbor) {
                            self.ready_for_meshing.insert(neighbor);
                        }
                    }
                }
                ChunkLoaderEvent::ShouldUnload(items) => {
                    removed += items.len();
                    for pos in items {
                        self.chunks.remove(&pos);
                        self.ready_for_meshing.remove(&pos);
                        self.ready_to_unload.insert(pos);
                    }
                }
            }
        }

        if added > 0 || removed > 0 {
            log::debug!("World update: +{} -{} chunks", added, removed);
        }
    }

    pub fn get_chunks_ready_for_meshing(&mut self, chunk_positions: &mut Vec<ChunkPos>) {
        chunk_positions.extend(self.ready_for_meshing.drain());
    }

    pub fn get_chunks_ready_to_unload(&mut self, chunk_positions: &mut Vec<ChunkPos>) {
        chunk_positions.extend(self.ready_to_unload.drain());
    }
}
