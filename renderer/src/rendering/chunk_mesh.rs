use bytemuck::{Pod, Zeroable};
use glam::IVec4;

use engine::{
    math::aabb::{AABB8, PackedAABB},
    mesh_generation::chunk_mesh::PackedVoxelFace,
    voxels::coord::ChunkPos,
};

use crate::rendering::memory::gpu_heap::GpuHeapHandle;

#[repr(C, align(16))]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct GpuChunk {
    // World position of the chunk (x, y, z, w unused)
    pub position: IVec4,
    pub face_byte_offset: u32,
    pub total_face_count: u32,
    pub opaque_face_count: u32,
    pub aabb: PackedAABB,
}

pub struct ChunkMesh {
    pub position: ChunkPos,
    pub aabb: AABB8,
    pub faces_handle: GpuHeapHandle<PackedVoxelFace>,
}
