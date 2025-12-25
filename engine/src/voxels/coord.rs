use std::ops::{Add, Sub};

use glam::{IVec3, U8Vec3};

use crate::voxels::{chunk::CHUNK_SIZE, face::Face};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// A position of a voxel within a chunk
pub struct LocalPos(pub U8Vec3);

impl LocalPos {
    pub fn new(x: u8, y: u8, z: u8) -> Self {
        if x >= CHUNK_SIZE || y >= CHUNK_SIZE || z >= CHUNK_SIZE {
            panic!("ChunkSpaceCoord out of bounds: ({}, {}, {})", x, y, z);
        }
        LocalPos(U8Vec3 { x, y, z })
    }

    pub fn offset(&self, face: Face) -> Option<LocalPos> {
        let offset = face.to_ivec3();
        let new_pos = self.0.as_ivec3() + offset;

        if new_pos.x < 0
            || new_pos.y < 0
            || new_pos.z < 0
            || new_pos.x >= CHUNK_SIZE as i32
            || new_pos.y >= CHUNK_SIZE as i32
            || new_pos.z >= CHUNK_SIZE as i32
        {
            None
        } else {
            Some(LocalPos(new_pos.as_u8vec3()))
        }
    }

    pub fn x(&self) -> u8 {
        self.0.x
    }

    pub fn y(&self) -> u8 {
        self.0.y
    }

    pub fn z(&self) -> u8 {
        self.0.z
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Coordinates identifying a chunk in chunk space (world coordinates divided by chunk size and floored)
pub struct ChunkPos(pub IVec3);

impl ChunkPos {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        ChunkPos(IVec3 { x, y, z })
    }

    pub fn x(&self) -> i32 {
        self.0.x
    }

    pub fn y(&self) -> i32 {
        self.0.y
    }

    pub fn z(&self) -> i32 {
        self.0.z
    }

    pub fn origin(&self) -> WorldPos {
        WorldPos(self.0 * IVec3::splat(CHUNK_SIZE as i32))
    }
}

/// A position of a voxel in world space
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldPos(pub IVec3);

impl WorldPos {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        WorldPos(IVec3 { x, y, z })
    }

    pub fn to_chunk_pos(&self) -> ChunkPos {
        let converted_pos = self.0.div_euclid(IVec3::splat(CHUNK_SIZE as i32));
        ChunkPos(converted_pos)
    }

    pub fn to_local_pos(&self) -> LocalPos {
        let converted_pos = self.0.rem_euclid(IVec3::splat(CHUNK_SIZE as i32));
        LocalPos(converted_pos.as_u8vec3())
    }

    pub fn from_chunk_and_voxel(chunk_coord: ChunkPos, voxel_coord: LocalPos) -> Self {
        chunk_coord.origin() + WorldPos(voxel_coord.0.as_ivec3())
    }
}

impl Add for WorldPos {
    type Output = WorldPos;

    fn add(self, other: WorldPos) -> WorldPos {
        WorldPos(self.0 + other.0)
    }
}

impl Sub for WorldPos {
    type Output = WorldPos;

    fn sub(self, other: WorldPos) -> WorldPos {
        WorldPos(self.0 - other.0)
    }
}
