use std::ops::{Add, Sub};

use glam::{IVec3, U8Vec3, Vec3};

use crate::{
    math::axis::Axis,
    voxels::{chunk::CHUNK_SIZE, face::Face},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// A position of a voxel within a chunk
pub struct LocalPos(pub U8Vec3);

impl LocalPos {
    #[inline(always)]
    pub fn new(x: u8, y: u8, z: u8) -> Self {
        if x >= CHUNK_SIZE || y >= CHUNK_SIZE || z >= CHUNK_SIZE {
            panic!("ChunkSpaceCoord out of bounds: ({}, {}, {})", x, y, z);
        }
        LocalPos(U8Vec3 { x, y, z })
    }

    #[inline(always)]
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

    #[inline(always)]
    pub fn x(&self) -> u8 {
        self.0.x
    }

    #[inline(always)]
    pub fn y(&self) -> u8 {
        self.0.y
    }

    #[inline(always)]
    pub fn z(&self) -> u8 {
        self.0.z
    }

    #[inline(always)]
    pub fn to_chunk_data_index(&self) -> usize {
        let chunk_size = CHUNK_SIZE as usize;
        // Y->Z->X ordering
        self.0.y as usize * chunk_size * chunk_size
            + self.0.z as usize * chunk_size
            + self.0.x as usize
    }
}

impl From<U8Vec3> for LocalPos {
    fn from(value: U8Vec3) -> Self {
        LocalPos::new(value.x, value.y, value.z)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
/// Coordinates identifying a chunk in chunk space (world coordinates divided by chunk size and floored)
pub struct ChunkPos(pub IVec3);

impl ChunkPos {
    #[inline(always)]
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        ChunkPos(IVec3 { x, y, z })
    }

    #[inline(always)]
    pub fn x(&self) -> i32 {
        self.0.x
    }

    #[inline(always)]
    pub fn y(&self) -> i32 {
        self.0.y
    }

    #[inline(always)]
    pub fn z(&self) -> i32 {
        self.0.z
    }

    #[inline(always)]
    pub fn origin(&self) -> WorldPos {
        WorldPos(self.0 * IVec3::splat(CHUNK_SIZE as i32))
    }

    #[inline(always)]
    pub fn get_neighbor(&self, face: Face) -> ChunkPos {
        let offset = face.to_ivec3();
        ChunkPos(self.0 + offset)
    }

    pub fn get_neighbors(&self) -> [ChunkPos; 6] {
        [
            self.get_neighbor(Face::Top),
            self.get_neighbor(Face::Bottom),
            self.get_neighbor(Face::Left),
            self.get_neighbor(Face::Right),
            self.get_neighbor(Face::Front),
            self.get_neighbor(Face::Back),
        ]
    }
}

impl Sub for ChunkPos {
    type Output = ChunkPos;

    #[inline(always)]
    fn sub(self, other: ChunkPos) -> ChunkPos {
        ChunkPos(self.0 - other.0)
    }
}

impl Add for ChunkPos {
    type Output = ChunkPos;

    #[inline(always)]
    fn add(self, other: ChunkPos) -> ChunkPos {
        ChunkPos(self.0 + other.0)
    }
}

/// A position of a voxel in world space
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldPos(pub IVec3);

impl WorldPos {
    #[inline(always)]
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        WorldPos(IVec3 { x, y, z })
    }

    #[inline(always)]
    pub fn to_chunk_pos(&self) -> ChunkPos {
        let converted_pos = self.0.div_euclid(IVec3::splat(CHUNK_SIZE as i32));
        ChunkPos(converted_pos)
    }

    #[inline(always)]
    pub fn to_local_pos(&self) -> LocalPos {
        let converted_pos = self.0.rem_euclid(IVec3::splat(CHUNK_SIZE as i32));
        LocalPos(converted_pos.as_u8vec3())
    }

    #[inline(always)]
    pub fn from_chunk_and_voxel(chunk_coord: ChunkPos, voxel_coord: LocalPos) -> Self {
        chunk_coord.origin() + WorldPos(voxel_coord.0.as_ivec3())
    }

    #[inline(always)]
    pub const fn get_axis(&self, axis: Axis) -> i32 {
        match axis {
            Axis::X => self.0.x,
            Axis::Y => self.0.y,
            Axis::Z => self.0.z,
        }
    }
}

impl From<IVec3> for WorldPos {
    fn from(value: IVec3) -> Self {
        WorldPos(value)
    }
}

impl From<[i32; 3]> for WorldPos {
    fn from(value: [i32; 3]) -> Self {
        WorldPos(IVec3::from(value))
    }
}

impl From<WorldPos> for [i32; 3] {
    fn from(value: WorldPos) -> Self {
        [value.0.x, value.0.y, value.0.z]
    }
}

impl Add for WorldPos {
    type Output = WorldPos;

    #[inline(always)]
    fn add(self, other: WorldPos) -> WorldPos {
        WorldPos(self.0 + other.0)
    }
}

impl Sub for WorldPos {
    type Output = WorldPos;

    #[inline(always)]
    fn sub(self, other: WorldPos) -> WorldPos {
        WorldPos(self.0 - other.0)
    }
}

impl Add<LocalPos> for WorldPos {
    type Output = WorldPos;

    #[inline(always)]
    fn add(self, other: LocalPos) -> WorldPos {
        WorldPos(self.0 + other.0.as_ivec3())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldPosF(pub Vec3);

impl From<WorldPosF> for Vec3 {
    fn from(value: WorldPosF) -> Self {
        value.0
    }
}

impl From<Vec3> for WorldPosF {
    fn from(value: Vec3) -> Self {
        WorldPosF(value)
    }
}

impl WorldPosF {
    #[inline(always)]
    pub fn to_chunk_pos(&self) -> ChunkPos {
        let converted_pos = self.0 / Vec3::splat(CHUNK_SIZE as f32).floor();
        ChunkPos(converted_pos.as_ivec3())
    }

    #[inline(always)]
    pub fn to_local_pos(&self) -> LocalPos {
        let converted_pos = self.0.rem_euclid(Vec3::splat(CHUNK_SIZE as f32)).floor();
        LocalPos(converted_pos.as_u8vec3())
    }
}
