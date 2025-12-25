use crate::voxels::{
    chunk::Chunk,
    coord::{ChunkPos, WorldPos},
    voxel::Voxel,
};

use dashmap::{DashMap, DashSet};

pub struct World {
    pub chunks: DashMap<ChunkPos, Chunk>,
    modified_chunks: DashSet<ChunkPos>,
}

impl World {
    pub fn new() -> Self {
        World {
            chunks: DashMap::new(),
            modified_chunks: DashSet::new(),
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
        self.modified_chunks.insert(chunk_id);
    }

    pub fn get_voxel(&self, position: WorldPos) -> Option<Voxel> {
        let chunk_id = position.to_chunk_pos();
        let chunk = self.chunks.get(&chunk_id)?;
        let local_pos = position.to_local_pos();
        chunk.get_voxel(local_pos)
    }
}
