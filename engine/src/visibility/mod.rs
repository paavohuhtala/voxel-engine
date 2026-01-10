use std::sync::LazyLock;

use glam::IVec3;

use crate::{limits::LOAD_DISTANCE, voxels::coord::ChunkPos};

pub mod potentially_visible;

pub fn generate_desired_chunk_offsets() -> Vec<IVec3> {
    let mut offsets = Vec::new();

    for x in -LOAD_DISTANCE..=LOAD_DISTANCE {
        for y in -LOAD_DISTANCE..=LOAD_DISTANCE {
            for z in -LOAD_DISTANCE..=LOAD_DISTANCE {
                offsets.push(IVec3::new(x, y, z));
            }
        }
    }

    offsets.sort_unstable_by_key(|offset| offset.length_squared());
    offsets
}

static OFFSETS: LazyLock<Vec<IVec3>> = LazyLock::new(generate_desired_chunk_offsets);

pub fn potentially_desired_chunks_iter(center: ChunkPos) -> impl Iterator<Item = ChunkPos> {
    OFFSETS.iter().map(move |offset| center + ChunkPos(*offset))
}
