use crate::{
    limits::VIEW_DISTANCE,
    math::frustum::Frustum,
    voxels::coord::{ChunkPos, WorldPosF},
};
use glam::Vec3;
use ordered_float::OrderedFloat;

pub struct PotentiallyVisibleChunks {
    pub chunks: Vec<ChunkPos>,
}

impl PotentiallyVisibleChunks {
    pub fn new() -> Self {
        PotentiallyVisibleChunks { chunks: Vec::new() }
    }

    pub fn update_and_sort(&mut self, eye: Vec3, frustum: &Frustum) {
        self.chunks.clear();
        get_potentially_visible_chunks(eye, frustum, &mut self.chunks);
    }
}

impl Default for PotentiallyVisibleChunks {
    fn default() -> Self {
        Self::new()
    }
}

/// Gets potentially visible chunks within the view distance that intersect the given frustum.
/// Chunks are sorted by distance to the eye position.
fn get_potentially_visible_chunks(eye: Vec3, frustum: &Frustum, chunks: &mut Vec<ChunkPos>) {
    let view_distance = VIEW_DISTANCE;
    let diameter = view_distance * 2 + 1;

    let current_chunk = WorldPosF(eye).to_chunk_pos();

    for x in 0..diameter {
        for y in 0..diameter {
            for z in 0..diameter {
                let chunk_pos = current_chunk
                    + ChunkPos::new(x - view_distance, y - view_distance, z - view_distance);
                let aabb = chunk_pos.get_aabb();
                if frustum.intersects_aabb(&aabb) {
                    chunks.push(chunk_pos);
                }
            }
        }
    }

    chunks.sort_unstable_by_key(|chunk_pos| OrderedFloat(chunk_pos.center().distance_squared(eye)));
}
