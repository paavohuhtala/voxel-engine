use std::hint::unreachable_unchecked;

use crate::voxels::{
    chunk::{CHUNK_SIZE, Chunk, ChunkData, IChunkRenderState},
    coord::LocalPos,
    face::Face,
    voxel::Voxel,
};

const BORDER_VOLUME: usize = CHUNK_SIZE as usize * CHUNK_SIZE as usize;

// Represents the border voxels of a chunk. Used for meshing with neighboring chunks.
pub struct Border {
    orientation: Face,
    /// Set to true if all voxels in this border are non-transparent
    pub occludes: bool,
    voxels: [Voxel; BORDER_VOLUME],
}

impl Border {
    pub fn new(orientation: Face) -> Self {
        Border {
            orientation,
            occludes: false,
            voxels: [Voxel::AIR; BORDER_VOLUME],
        }
    }

    pub fn copy_from_chunk<T: IChunkRenderState>(&mut self, chunk: &Chunk<T>) {
        let Some(chunk_data) = chunk.data.as_ref() else {
            panic!(
                "Tried to copy border from chunk at position {:?} which has no data",
                chunk.position
            );
        };

        match chunk_data {
            ChunkData::Solid(voxel) => {
                self.voxels.fill(*voxel);
                self.occludes = !voxel.is_transparent();
            }
            ChunkData::Packed(packed) => {
                let mut occludes = true;
                // Use the orientation to determine which border to copy
                for x in 0..CHUNK_SIZE {
                    for y in 0..CHUNK_SIZE {
                        let local_pos = match self.orientation {
                            Face::Top => LocalPos::new(x, CHUNK_SIZE - 1, y),
                            Face::Bottom => LocalPos::new(x, 0, y),
                            Face::Left => LocalPos::new(0, x, y),
                            Face::Right => LocalPos::new(CHUNK_SIZE - 1, x, y),
                            Face::Front => LocalPos::new(x, y, CHUNK_SIZE - 1),
                            Face::Back => LocalPos::new(x, y, 0),
                        };

                        if let Some(voxel) = packed.get_voxel(local_pos) {
                            let target_index = (y as usize) * (CHUNK_SIZE as usize) + (x as usize);
                            self.voxels[target_index] = voxel;
                            if occludes && voxel.is_transparent() {
                                occludes = false;
                            }
                        } else {
                            unsafe { unreachable_unchecked() }
                        }
                    }
                }
                self.occludes = occludes;
            }
        }
    }

    pub fn get_voxel(&self, pos: LocalPos) -> Option<Voxel> {
        // Check if the position is on the border plane
        let is_on_border = match self.orientation {
            Face::Top => pos.y() == CHUNK_SIZE - 1,
            Face::Bottom => pos.y() == 0,
            Face::Left => pos.x() == 0,
            Face::Right => pos.x() == CHUNK_SIZE - 1,
            Face::Front => pos.z() == CHUNK_SIZE - 1,
            Face::Back => pos.z() == 0,
        };

        if !is_on_border {
            return None;
        }

        // Convert pos to border-local coordinates
        let (x, y) = match self.orientation {
            Face::Top | Face::Bottom => (pos.x(), pos.z()),
            Face::Left | Face::Right => (pos.y(), pos.z()),
            Face::Front | Face::Back => (pos.x(), pos.y()),
        };

        let index = (y as usize) * (CHUNK_SIZE as usize) + (x as usize);
        Some(self.voxels[index])
    }

    // This only exists for testing purposes
    pub fn set_voxel(&mut self, pos: LocalPos, voxel: Voxel) {
        // Check if the position is on the border plane
        let is_on_border = match self.orientation {
            Face::Top => pos.y() == CHUNK_SIZE - 1,
            Face::Bottom => pos.y() == 0,
            Face::Left => pos.x() == 0,
            Face::Right => pos.x() == CHUNK_SIZE - 1,
            Face::Front => pos.z() == CHUNK_SIZE - 1,
            Face::Back => pos.z() == 0,
        };

        assert!(
            is_on_border,
            "Attempted to set voxel at non-border position {:?} for border {:?}",
            pos, self.orientation
        );

        // Convert pos to border-local coordinates
        let (x, y) = match self.orientation {
            Face::Top | Face::Bottom => (pos.x(), pos.z()),
            Face::Left | Face::Right => (pos.y(), pos.z()),
            Face::Front | Face::Back => (pos.x(), pos.y()),
        };

        let index = (y as usize) * (CHUNK_SIZE as usize) + (x as usize);
        self.voxels[index] = voxel;
    }
}
