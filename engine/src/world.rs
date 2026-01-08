use std::sync::Arc;

use crate::{
    assets::blocks::BlockDatabaseSlim,
    chunk_loader::{ChunkLoader, ChunkLoaderHandle},
    voxels::{
        chunk::{Chunk, ChunkData, ChunkState, IChunkRenderState},
        coord::{ChunkPos, WorldPos},
        voxel::Voxel,
    },
    world_stats::{CHUNKS_BY_STATE, WorldStatistics},
    worldgen::WorldGenerator,
};

use dashmap::DashMap;

pub struct World<T: IChunkRenderState = ()> {
    pub chunk_loader: ChunkLoaderHandle<T>,
    pub chunks: Arc<DashMap<ChunkPos, Chunk<T>>>,
    statistics: WorldStatistics,
}

pub enum ChunkEvent {
    Loaded,
    Unloaded,
    FinishedMeshing,
}

impl<T: IChunkRenderState + Send + Sync + 'static> World<T> {
    pub fn from_generator(
        generator: impl WorldGenerator,
        block_database: Arc<BlockDatabaseSlim>,
        render_context: T::Context,
    ) -> Self {
        Self::from_chunks(generator, block_database, Vec::new(), render_context)
    }

    pub fn from_chunks(
        generator: impl WorldGenerator,
        block_database: Arc<BlockDatabaseSlim>,
        initial_chunks: Vec<(ChunkPos, ChunkData)>,
        render_context: T::Context,
    ) -> Self {
        let statistics = WorldStatistics::new();
        CHUNKS_BY_STATE.increment_by(ChunkState::Loaded, initial_chunks.len() as u32);

        let initial_chunk_positions = initial_chunks
            .iter()
            .map(|(pos, _)| *pos)
            .collect::<Vec<_>>();

        let chunks_map = initial_chunks
            .into_iter()
            .map(|(pos, data)| (pos, Chunk::from_data(pos, data)))
            .collect::<DashMap<ChunkPos, Chunk<T>>>();

        let chunks = Arc::new(chunks_map);
        let chunk_access = chunks.clone();

        let chunk_loader = ChunkLoader::start(
            Box::new(generator),
            block_database,
            chunk_access,
            render_context,
        );
        let world = World {
            chunk_loader,
            chunks,
            statistics,
        };
        world.update_neighbors_for_chunks(initial_chunk_positions.into_iter());
        world
    }

    pub fn set_voxel(&self, position: WorldPos, voxel: Voxel) {
        let chunk_id = position.to_chunk_pos();
        self.chunks.entry(chunk_id).and_modify(|chunk| {
            // TODO: Changes to non-existent chunks are silently ignored, is that good?
            // If the chunk hasn't been generated yet, it will be overwritten when generation finishes
            let local_pos = position.to_local_pos();
            chunk.set_voxel(local_pos, voxel);
        });
    }

    pub fn get_voxel(&self, position: WorldPos) -> Option<Voxel> {
        let chunk_id = position.to_chunk_pos();
        let chunk = self.chunks.get(&chunk_id)?;
        let local_pos = position.to_local_pos();
        chunk.get_voxel(local_pos)
    }

    pub fn get_statistics(&self) -> &WorldStatistics {
        &self.statistics
    }

    pub fn update(&mut self) {
        self.statistics.total_chunks = self.chunks.len();
    }

    /// Sets neighbor bits for each provided chunk position.
    /// Slow, only run when world loads / is generated for the first time.
    fn update_neighbors_for_chunks(&self, positions: impl Iterator<Item = ChunkPos>) {
        for pos in positions {
            let mut neighbor_mask = 0u8;
            // Neighbors are iterated in Face order, so we can use the index as the bit position
            for (i, neighbor_pos) in pos.get_neighbors().iter().enumerate() {
                let neighbor_is_loaded = self
                    .chunks
                    .get(neighbor_pos)
                    .map(|c| c.is_suitable_neighbor_for_meshing())
                    .unwrap_or(false);
                if neighbor_is_loaded {
                    neighbor_mask |= 1 << i;
                }
            }
            if let Some(chunk) = self.chunks.get(&pos) {
                chunk.neighbor_state.set_neighbor_bits(neighbor_mask);
            }
        }
    }
}
