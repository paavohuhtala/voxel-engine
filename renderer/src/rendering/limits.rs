use engine::voxels::chunk::CHUNK_SIZE;

use crate::rendering::chunk_mesh::PackedVoxelFace;

pub const MAX_GPU_CHUNKS: u64 = 32 * 32 * 16;

// Worst case is a chunk with a checkerboard pattern, with all 6 faces visible
// With 16*16*16 voxels, that means ((16*16*16) / 2) * 6) = 12288 faces per chunk
const MAX_FACES_PER_CHUNK: u64 = (CHUNK_SIZE as u64).pow(3) / 2 * 6;
const MAX_CHUNK_SIZE: u64 = size_of::<PackedVoxelFace>() as u64 * MAX_FACES_PER_CHUNK;

pub const FACE_BUFFER_SIZE_BYTES: u64 = (MAX_CHUNK_SIZE * MAX_GPU_CHUNKS).next_multiple_of(4);
