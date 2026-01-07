use std::{collections::HashMap, sync::Arc};

use crate::{
    assets::blocks::BlockDatabaseSlim,
    chunk_loader::{ChunkLoader, ChunkLoaderHandle},
    voxels::{
        chunk::{Chunk, ChunkData, ChunkState, IChunkRenderState},
        coord::{ChunkPos, WorldPos},
        voxel::Voxel,
    },
    worldgen::WorldGenerator,
};

use dashmap::DashMap;

pub struct World<T: IChunkRenderState = ()> {
    pub chunk_loader: ChunkLoaderHandle<T>,
    pub chunks: Arc<DashMap<ChunkPos, Chunk<T>>>,
    last_statistics: WorldStatistics,
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
        chunks_vec: Vec<(ChunkPos, ChunkData)>,
        render_context: T::Context,
    ) -> Self {
        let chunks_map = chunks_vec
            .into_iter()
            .map(|(pos, data)| (pos, Chunk::from_data(pos, data)))
            .collect::<DashMap<ChunkPos, Chunk<T>>>();

        //let initial_loaded_chunks = chunks_map.iter().map(|entry| *entry.key()).collect();

        let chunks = Arc::new(chunks_map);
        let chunk_access = chunks.clone();

        let chunk_loader = ChunkLoader::start(
            Box::new(generator),
            block_database,
            chunk_access,
            render_context,
        );
        World {
            chunk_loader,
            chunks,
            last_statistics: WorldStatistics {
                total_loaded_chunks: 0,
                approximate_memory_usage_bytes: 0,
                chunks_by_state: HashMap::new(),
            },
        }
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
        &self.last_statistics
    }
}

pub struct WorldStatistics {
    pub total_loaded_chunks: usize,
    pub approximate_memory_usage_bytes: usize,
    pub chunks_by_state: HashMap<ChunkState, usize>,
}
