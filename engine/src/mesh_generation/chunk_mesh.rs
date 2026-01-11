use bytemuck::{Pod, Zeroable};
use glam::{U8Vec2, U8Vec3};

use crate::{
    math::aabb::AABB8,
    voxels::{coord::ChunkPos, face::Face},
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
/// A voxel face packed into 6 bytes (48 bits).
/// Stored as a byte array to avoid padding & alignment issues.
///
/// Layout:
/// - Bytes 0-3: Geometry data (32 bits, little-endian)
///   - bits 0-3:   position.x (0-15)
///   - bits 4-7:   position.y (0-15)
///   - bits 8-11:  position.z (0-15)
///   - bits 12-14: face_id (0-5)
///   - bit 15:     flip_diagonal
///   - bits 16-19: width - 1 (0-15)
///   - bits 20-23: height - 1 (0-15)
///   - bits 24-25: AO bottom-left (0-3)
///   - bits 26-27: AO bottom-right (0-3)
///   - bits 28-29: AO top-right (0-3)
///   - bits 30-31: AO top-left (0-3)
/// - Bytes 4-5: Texture index (16 bits, little-endian)
pub struct PackedVoxelFace {
    bytes: [u8; 6],
}

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
        let mut geometry = 0u32;
        geometry |= (value.position.x as u32) & 0xF;
        geometry |= ((value.position.y as u32) & 0xF) << 4;
        geometry |= ((value.position.z as u32) & 0xF) << 8;
        geometry |= ((value.face_direction as u32) & 0x7) << 12;
        if value.flip_diagonal {
            geometry |= 1 << 15;
        }
        geometry |= ((value.size.x.saturating_sub(1) as u32) & 0xF) << 16;
        geometry |= ((value.size.y.saturating_sub(1) as u32) & 0xF) << 20;
        geometry |= ((value.ambient_occlusion[0] as u32) & 0x3) << 24;
        geometry |= ((value.ambient_occlusion[1] as u32) & 0x3) << 26;
        geometry |= ((value.ambient_occlusion[2] as u32) & 0x3) << 28;
        geometry |= ((value.ambient_occlusion[3] as u32) & 0x3) << 30;

        let texture_index = value.texture_index;

        let mut bytes = [0u8; 6];
        bytes[0..4].copy_from_slice(&geometry.to_le_bytes());
        bytes[4..6].copy_from_slice(&texture_index.to_le_bytes());
        PackedVoxelFace { bytes }
    }
}

impl PackedVoxelFace {
    /// Should only be used for debugging and tests.
    pub fn unpack(&self) -> VoxelFace {
        let geometry =
            u32::from_le_bytes([self.bytes[0], self.bytes[1], self.bytes[2], self.bytes[3]]);
        let texture_index = u16::from_le_bytes([self.bytes[4], self.bytes[5]]);

        let x = (geometry & 0xF) as u8;
        let y = ((geometry >> 4) & 0xF) as u8;
        let z = ((geometry >> 8) & 0xF) as u8;
        let position = U8Vec3::new(x, y, z);

        let face_id = ((geometry >> 12) & 0x7) as u8;
        let flip_diagonal = ((geometry >> 15) & 0x1) != 0;

        let width = ((geometry >> 16) & 0xF) as u8 + 1;
        let height = ((geometry >> 20) & 0xF) as u8 + 1;
        let size = U8Vec2::new(width, height);

        let ao0 = ((geometry >> 24) & 0x3) as u8;
        let ao1 = ((geometry >> 26) & 0x3) as u8;
        let ao2 = ((geometry >> 28) & 0x3) as u8;
        let ao3 = ((geometry >> 30) & 0x3) as u8;
        let ambient_occlusion = [ao0, ao1, ao2, ao3];

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

#[derive(Clone, Default)]
pub struct ChunkMeshData {
    pub position: ChunkPos,
    pub aabb: AABB8,
    pub opaque_faces: Vec<PackedVoxelFace>,
    pub alpha_cutout_faces: Vec<PackedVoxelFace>,
}

impl ChunkMeshData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_position(position: ChunkPos) -> Self {
        ChunkMeshData {
            position,
            aabb: AABB8::new(U8Vec3::splat(0), U8Vec3::splat(15)),
            opaque_faces: Vec::new(),
            alpha_cutout_faces: Vec::new(),
        }
    }

    pub fn total_faces(&self) -> usize {
        self.opaque_faces.len() + self.alpha_cutout_faces.len()
    }
}
