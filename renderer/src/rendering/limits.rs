use crate::rendering::chunk_mesh::ChunkVertex;

pub const MAX_GPU_CHUNKS: u64 = 32 * 32 * 12;

// TODO: What is the maximum number of vertices we might need per chunk?
pub const VERTEX_BUFFER_CAPACITY: u64 =
    (size_of::<ChunkVertex>() as u64 * MAX_GPU_CHUNKS * 512).next_power_of_two();
pub const INDEX_BUFFER_CAPACITY: u64 =
    (size_of::<u16>() as u64 * MAX_GPU_CHUNKS * 1024).next_power_of_two();
