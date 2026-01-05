use glam::{DVec2, Vec3Swizzles};
use noise::{NoiseFn, SuperSimplex};
use rayon::prelude::*;

use crate::{
    voxels::{
        chunk::{CHUNK_SIZE, Chunk},
        coord::{ChunkPos, LocalPos},
        unpacked_chunk::UnpackedChunk,
        voxel::Voxel,
    },
    world::World,
    worldgen::world_generator::WorldGenerator,
};

pub struct NoiseWorldGenerator {
    noise: SuperSimplex,
}

impl WorldGenerator for NoiseWorldGenerator {
    fn new(seed: u32) -> Self {
        Self {
            noise: SuperSimplex::new(seed),
        }
    }

    fn generate_chunk(&self, chunk_pos: ChunkPos) -> Chunk {
        let mut chunk = UnpackedChunk::new();
        let origin_pos = chunk_pos.origin();
        let origin_2d = origin_pos.0.xz().as_dvec2();

        // Data is stored in YZX order
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let pos = (origin_2d + DVec2::new(x as f64, z as f64)) * 0.01;
                let height = (self.noise.get(pos.to_array()) * 32.0) as i32;

                for y in 0..CHUNK_SIZE {
                    let world_y = origin_pos.0.y + y as i32;

                    let voxel = if world_y < height {
                        Voxel::DIRT
                    } else if world_y == height {
                        Voxel::GRASS
                    } else {
                        Voxel::AIR
                    };

                    if voxel != Voxel::AIR {
                        chunk.set_voxel(LocalPos::new(x, y, z), voxel);
                    }
                }
            }
        }

        Chunk::from(chunk)
    }
}

#[allow(unused)]
pub fn generate_noise_world(initial_size: i32) -> World {
    let generator = NoiseWorldGenerator::new(123_456);

    let chunk_range = -(initial_size / 2)..(initial_size / 2);
    let range_width = chunk_range.end - chunk_range.start;

    let chunks = (0..(range_width * range_width))
        .into_par_iter()
        .map(|i| {
            let x = chunk_range.start + (i / range_width);
            let z = chunk_range.start + (i % range_width);
            let pos = ChunkPos::new(x, 0, z);
            (pos, generator.generate_chunk(pos))
        })
        .collect::<Vec<_>>();

    World::from_chunks(generator, chunks)
}
