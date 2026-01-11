use std::sync::Arc;

use glam::{DVec2, Vec3Swizzles};
use noise::{NoiseFn, SuperSimplex};
use rayon::prelude::*;

use crate::{
    assets::blocks::BlockDatabaseSlim,
    voxels::{
        chunk::{CHUNK_SIZE, ChunkData, IChunkRenderState},
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

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn fbm(noise: &SuperSimplex, mut p: DVec2, octaves: usize, lacunarity: f64, gain: f64) -> f64 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut norm = 0.0;

    for _ in 0..octaves {
        sum += noise.get(p.to_array()) * amp;
        norm += amp;
        amp *= gain;
        p *= lacunarity;
    }

    if norm > 0.0 { sum / norm } else { 0.0 }
}

impl WorldGenerator for NoiseWorldGenerator {
    fn new(seed: u32) -> Self {
        Self {
            noise: SuperSimplex::new(seed),
        }
    }

    fn generate_chunk(&self, chunk_pos: ChunkPos) -> ChunkData {
        let mut chunk = UnpackedChunk::new();
        let origin_pos = chunk_pos.origin();
        let origin_2d = origin_pos.0.xz().as_dvec2();

        // Tuning knobs (world-space is in voxels).
        const SEA_LEVEL: f64 = 28.0;

        // Larger values -> smaller features (higher frequency). These are in 1/voxel.
        const CONTINENT_FREQ: f64 = 1.0 / 2048.0;
        const PLAINS_MASK_FREQ: f64 = 1.0 / 4096.0;
        const HILLS_FREQ: f64 = 1.0 / 768.0;
        const DETAIL_FREQ: f64 = 1.0 / 128.0;

        const CONTINENT_AMP: f64 = 32.0;
        const HILLS_AMP: f64 = 64.0;
        const PLAINS_DETAIL_AMP: f64 = 2.0;
        const HILL_DETAIL_AMP: f64 = 4.0;

        // Data is stored in YZX order
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let world_xz = origin_2d + DVec2::new(x as f64, z as f64);

                // Broad-scale base height.
                let continent = fbm(&self.noise, world_xz * CONTINENT_FREQ, 4, 2.0, 8.0);
                let mut height_f = SEA_LEVEL + continent * CONTINENT_AMP;

                // Low-frequency mask that decides where plains tend to appear.
                // 1.0 = plains, 0.0 = hilly.
                let plains_raw =
                    self.noise.get((world_xz * PLAINS_MASK_FREQ).to_array()) * 0.5 + 0.5;
                let plains = smoothstep(0.55, 0.75, plains_raw);
                let hilliness = 1.0 - plains;

                // Hills: only add positive noise so hills appear in patches.
                let hills_n = fbm(&self.noise, world_xz * HILLS_FREQ, 5, 2.0, 0.5);
                let hills = hills_n.max(0.0).powf(2.0) * HILLS_AMP;
                height_f += hills * hilliness;

                // Detail: very small in plains, a bit more in hills.
                let detail_n = self.noise.get((world_xz * DETAIL_FREQ).to_array());
                let detail_amp = lerp(PLAINS_DETAIL_AMP, HILL_DETAIL_AMP, hilliness);
                height_f += detail_n * detail_amp;

                let height = height_f.round() as i32;

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

        ChunkData::from(chunk)
    }
}

#[allow(unused)]
pub fn generate_noise_world<T: IChunkRenderState>(
    initial_size: i32,
    db: Arc<BlockDatabaseSlim>,
    render_context: T::Context,
) -> World<T> {
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

    World::from_chunks(generator, db, chunks, render_context)
}
