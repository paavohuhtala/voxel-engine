use engine::{
    limits::{VIEW_DISTANCE, VIEW_DISTANCE_Y},
    voxels::chunk::CHUNK_SIZE,
};

use crate::rendering::chunk_mesh::PackedVoxelFace;

// Given current view distance settings, this is the number of chunks needed to be stored in GPU memory
// to guarantee that all visible chunks can be rendered.
const REQUIRED_GPU_CHUNKS: u64 =
    (2 * VIEW_DISTANCE + 1).pow(2) as u64 * (2 * VIEW_DISTANCE_Y + 1) as u64;

// However, since we are streaming chunks in and out, we need some extra headroom to avoid stalls
pub const MAX_GPU_CHUNKS: u64 = (REQUIRED_GPU_CHUNKS as f32 * 1.5) as u64;

// Worst case is a chunk with a checkerboard pattern, with all 6 faces visible
// With 16*16*16 voxels, that means ((16*16*16) / 2) * 6) = 12288 faces per chunk
const MAX_FACES_PER_CHUNK: u64 = (CHUNK_SIZE as u64).pow(3) / 2 * 6;
const MAX_CHUNK_SIZE: u64 = size_of::<PackedVoxelFace>() as u64 * MAX_FACES_PER_CHUNK;

pub const FACE_BUFFER_SIZE_BYTES: u64 = (MAX_CHUNK_SIZE * MAX_GPU_CHUNKS).next_multiple_of(4);
