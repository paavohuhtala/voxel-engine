use bytemuck::{Pod, Zeroable};
use glam::{IVec4, U8Vec2, U8Vec3, UVec2};

use engine::{
    math::aabb::{AABB8, PackedAABB},
    voxels::{coord::ChunkPos, face::Face},
};

use crate::rendering::memory::gpu_heap::GpuHeapHandle;

#[repr(transparent)]
#[derive(Clone, Copy, Pod, Zeroable)]
// A face of a voxel packed into 64 bits
// Layout:
// First u32:
//   0-3: x (0-15)
//   4-7: y (0-15)
//   8-11: z (0-15)
//   12-14: face id (0-5)
//   15: flip diagonal
//   16-19: width in voxels - 1 (0-15)
//   20-23: height in voxels - 1 (0-15)
//   24-25: ambient occlusion, bottom-left (0-3)
//   26-27: ambient occlusion, bottom-right (0-3)
//   28-29: ambient occlusion, top-right (0-3)
//   30-31: ambient occlusion, top-left (0-3)
// Second u32:
//   0-15: texture index (0-0xFFFF)
//   16-31: unused
pub struct PackedVoxelFace(UVec2);

pub struct VoxelFace {
    pub position: U8Vec3,
    pub face_direction: Face,
    pub size: U8Vec2,
    /// Ambient occlusion values for each vertex (0-3)
    /// Order: bottom-left, bottom-right, top-right, top-left
    pub ambient_occlusion: [u8; 4],
    /// By default, the diagonal goes from bottom-left to top-right
    /// Set to true to flip the diagonal.
    /// This is used to get better looking ambient occlusion.
    pub flip_diagonal: bool,
    pub texture_index: u16,
}

impl From<VoxelFace> for PackedVoxelFace {
    fn from(value: VoxelFace) -> Self {
        let mut first = 0u32;
        first |= (value.position.x as u32) & 0xF;
        first |= ((value.position.y as u32) & 0xF) << 4;
        first |= ((value.position.z as u32) & 0xF) << 8;
        first |= ((value.face_direction as u32) & 0x7) << 12;
        if value.flip_diagonal {
            first |= 1 << 15;
        }
        first |= ((value.size.x.saturating_sub(1) as u32) & 0xF) << 16;
        first |= ((value.size.y.saturating_sub(1) as u32) & 0xF) << 20;
        first |= ((value.ambient_occlusion[0] as u32) & 0x3) << 24;
        first |= ((value.ambient_occlusion[1] as u32) & 0x3) << 26;
        first |= ((value.ambient_occlusion[2] as u32) & 0x3) << 28;
        first |= ((value.ambient_occlusion[3] as u32) & 0x3) << 30;

        let mut second = 0u32;
        second |= (value.texture_index as u32) & 0xFFFF;

        PackedVoxelFace(UVec2::new(first, second))
    }
}

impl PackedVoxelFace {
    /// Should only be used for debugging and tests.
    pub fn unpack(&self) -> VoxelFace {
        let first = self.0.x;
        let second = self.0.y;

        let x = (first & 0xF) as u8;
        let y = ((first >> 4) & 0xF) as u8;
        let z = ((first >> 8) & 0xF) as u8;
        let position = U8Vec3::new(x, y, z);

        let face_id = ((first >> 12) & 0x7) as u8;
        let flip_diagonal = ((first >> 15) & 0x1) != 0;

        let width = ((first >> 16) & 0xF) as u8 + 1;
        let height = ((first >> 20) & 0xF) as u8 + 1;
        let size = U8Vec2::new(width, height);

        let ao0 = ((first >> 24) & 0x3) as u8;
        let ao1 = ((first >> 26) & 0x3) as u8;
        let ao2 = ((first >> 28) & 0x3) as u8;
        let ao3 = ((first >> 30) & 0x3) as u8;
        let ambient_occlusion = [ao0, ao1, ao2, ao3];

        let texture_index = (second & 0xFFFF) as u16;

        VoxelFace {
            position,
            face_direction: Face::try_from(face_id).unwrap(),
            size,
            ambient_occlusion,
            flip_diagonal,
            texture_index,
        }
    }
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct GpuChunk {
    // World position of the chunk (x, y, z, w unused)
    pub position: IVec4,
    pub face_data_offset: u32,
    pub face_count: u32,
    pub aabb: PackedAABB,
    pub _padding: u32,
}

#[derive(Clone, Default)]
pub struct ChunkMeshData {
    pub position: ChunkPos,
    pub aabb: AABB8,
    pub faces: Vec<PackedVoxelFace>,
}

impl ChunkMeshData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_position(position: ChunkPos) -> Self {
        ChunkMeshData {
            position,
            aabb: AABB8::new(U8Vec3::splat(0), U8Vec3::splat(15)),
            faces: Vec::new(),
        }
    }
}

pub struct ChunkMesh {
    pub position: ChunkPos,
    pub aabb: AABB8,
    pub faces_handle: GpuHeapHandle<PackedVoxelFace>,
}
