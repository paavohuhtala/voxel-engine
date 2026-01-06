use glam::U8Vec3;

use crate::{
    math::aabb::AABB8,
    voxels::{
        chunk::{CHUNK_SIZE, CHUNK_VOLUME, Chunk, ChunkData, IChunkRenderState},
        coord::LocalPos,
        voxel::Voxel,
    },
};

pub struct UnpackedChunk {
    pub voxels: [Voxel; CHUNK_VOLUME],
}

pub enum UnpackedChunkResult {
    Data(Box<UnpackedChunk>),
    // This chunk consists entirely of air voxels
    Empty,
}

impl UnpackedChunk {
    pub fn new() -> Self {
        UnpackedChunk {
            voxels: [Voxel::AIR; CHUNK_VOLUME],
        }
    }

    pub fn try_from_chunk<T: IChunkRenderState>(chunk: &Chunk<T>) -> UnpackedChunkResult {
        let Some(chunk_data) = chunk.data.as_ref() else {
            panic!(
                "Tried to unpack chunk at position {:?} which has no data",
                chunk.position
            );
        };

        match chunk_data {
            ChunkData::Solid(voxel) if *voxel == Voxel::AIR => {
                return UnpackedChunkResult::Empty;
            }
            _ => {}
        }

        let mut unpacked_chunk = UnpackedChunk::new();

        match chunk_data {
            ChunkData::Solid(voxel) => {
                unpacked_chunk.voxels.fill(*voxel);
            }
            ChunkData::Packed(packed) => {
                packed.unpack(unpacked_chunk.voxels.as_mut_slice());
            }
        }

        UnpackedChunkResult::Data(Box::new(unpacked_chunk))
    }

    pub fn get_voxel(&self, pos: LocalPos) -> Option<Voxel> {
        let index = pos.to_chunk_data_index();
        self.voxels.get(index).copied()
    }

    pub fn set_voxel(&mut self, pos: LocalPos, voxel: Voxel) {
        let index = pos.to_chunk_data_index();
        self.voxels[index] = voxel;
    }

    pub fn compute_aabb(&self) -> AABB8 {
        let mut min = U8Vec3::splat(15);
        let mut max = U8Vec3::splat(0);

        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let index = (y as usize * CHUNK_SIZE as usize * CHUNK_SIZE as usize)
                        + (z as usize * CHUNK_SIZE as usize)
                        + x as usize;
                    let voxel = self.voxels[index];
                    if voxel != Voxel::AIR {
                        let pos = U8Vec3::new(x, y, z);
                        min = min.min(pos);
                        max = max.max(pos);
                    }
                }
            }
        }

        AABB8::new(min, max)
    }
}

impl Default for UnpackedChunk {
    fn default() -> Self {
        Self::new()
    }
}
