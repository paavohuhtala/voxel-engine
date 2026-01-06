use std::sync::Arc;

use crate::{
    assets::blocks::BlockDatabaseSlim,
    voxels::{
        chunk::{CHUNK_SIZE, ChunkData, IChunkRenderState},
        coord::{ChunkPos, LocalPos},
        unpacked_chunk::UnpackedChunk,
        voxel::Voxel,
    },
    world::World,
    worldgen::WorldGenerator,
};

pub struct TortureTestWorldGenerator;

impl WorldGenerator for TortureTestWorldGenerator {
    fn new(_seed: u32) -> Self {
        TortureTestWorldGenerator
    }

    fn generate_chunk(&self, chunk_pos: ChunkPos) -> ChunkData {
        // Generates a chunk filled with a test pattern, where every other voxel is filled
        let mut chunk = UnpackedChunk::new();
        let grass = Voxel::from_type(1);

        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let world_x = chunk_pos.origin().0.x + x as i32;
                    let world_y = chunk_pos.origin().0.y + y as i32;
                    let world_z = chunk_pos.origin().0.z + z as i32;

                    // Simple pattern: fill voxel if the sum of coordinates is even
                    if (world_x + world_y + world_z) % 2 == 0 {
                        chunk.set_voxel(LocalPos::new(x, y, z), grass);
                    }
                }
            }
        }

        ChunkData::from(chunk)
    }
}

#[allow(unused)]
pub fn generate_torture_test_world<T: IChunkRenderState>(
    db: Arc<BlockDatabaseSlim>,
    render_context: T::Context,
) -> World<T> {
    let generator = TortureTestWorldGenerator::new(0);
    World::from_generator(generator, db, render_context)
}
