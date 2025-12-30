use glam::{DVec2, Vec3Swizzles};
use noise::{NoiseFn, SuperSimplex};
use rayon::prelude::*;

pub mod text_generator;

use crate::voxels::{
    chunk::{CHUNK_SIZE, CHUNK_VOLUME, Chunk},
    coord::{ChunkPos, WorldPos},
    voxel::Voxel,
    world::World,
};

pub fn generate_basic_world() -> World {
    // Generates a flat world sized 64^3 with stone between y=[-32,-1], grass on y=0 and air above (y>0)
    let world = World::new();
    let stone = Voxel::from_type(1);
    let grass = Voxel::from_type(2);

    for x in -32..32 {
        for y in -32..32 {
            for z in -32..32 {
                let pos = WorldPos::new(x, y, z);
                if y < 0 {
                    world.set_voxel(pos, stone);
                } else if y == 0 {
                    world.set_voxel(pos, grass);
                }
            }
        }
    }
    world
}

pub fn generate_noise_world(size: i32) -> World {
    let noise = SuperSimplex::new(123_456);

    fn generate_chunk(noise: &SuperSimplex, chunk_pos: ChunkPos) -> Chunk {
        // Generate chunk data from noise, then pack it into a PackedChunk
        let mut voxels = [Voxel::AIR; CHUNK_VOLUME];

        let origin_pos = chunk_pos.origin().0.xz().as_dvec2();

        // Data is stored in YZX order
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let pos = (origin_pos + DVec2::new(x as f64, z as f64)) * 0.01;
                let height = (noise.get(pos.to_array()) + 1.0) / 2.0;
                let height = (height * 15.0) as i32;

                let mut last_index = None;

                for y in 0..CHUNK_SIZE {
                    let voxel = if y as i32 <= height {
                        Voxel::DIRT
                    } else {
                        // Air from now on
                        break;
                    };

                    let index = (y as usize * CHUNK_SIZE as usize * CHUNK_SIZE as usize)
                        + (z as usize * CHUNK_SIZE as usize)
                        + x as usize;
                    voxels[index] = voxel;
                    last_index = Some(index);
                }

                // Change the top voxel to grass if there was at least one voxel placed
                if let Some(index) = last_index {
                    voxels[index] = Voxel::GRASS;
                }
            }
        }

        Chunk::from_voxels(&voxels)
    }

    let chunk_range = -(size / 2)..(size / 2);
    let range_width = chunk_range.end - chunk_range.start;

    let chunks = (0..(range_width * range_width))
        .into_par_iter()
        .map(|i| {
            let x = chunk_range.start + (i / range_width);
            let z = chunk_range.start + (i % range_width);
            let pos = ChunkPos::new(x, 0, z);
            (pos, generate_chunk(&noise, pos))
        })
        .collect::<Vec<_>>();

    World::from_chunks(chunks)
}
