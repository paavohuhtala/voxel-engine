use crate::voxels::{chunk::ChunkData, coord::ChunkPos};

pub trait WorldGenerator: Send + Sync + 'static {
    fn new(seed: u32) -> Self
    where
        Self: Sized;
    fn generate_chunk(&self, chunk_pos: ChunkPos) -> ChunkData;
}
