use anyhow::Context;

use dashmap::DashMap;

use crate::voxels::{
    border::Border,
    chunk::{Chunk, IChunkRenderState},
    coord::{ChunkPos, WorldPos},
    face::Face,
    unpacked_chunk::{UnpackedChunk, UnpackedChunkResult},
    voxel::Voxel,
};

/// Contains everything from the world required to mesh a chunk,
/// including the bordering voxels from its 6 neighboring chunks.
pub struct ChunkMeshGeneratorInput {
    pub center_pos: ChunkPos,
    pub center: Box<UnpackedChunk>,
    // The neighbors of a chunk, in opposite face order (Bottom, Top, Right, Left, Back, Front)
    pub neighbors: Box<[Border; 6]>,
}

impl ChunkMeshGeneratorInput {
    pub fn new_empty(center_pos: ChunkPos) -> Self {
        let center = UnpackedChunk::default();
        let neighbors = Box::new([
            Border::new(Face::Bottom),
            Border::new(Face::Top),
            Border::new(Face::Right),
            Border::new(Face::Left),
            Border::new(Face::Back),
            Border::new(Face::Front),
        ]);
        ChunkMeshGeneratorInput {
            center: Box::new(center),
            center_pos,
            neighbors,
        }
    }

    pub fn try_from_map<T: IChunkRenderState>(
        chunks: &DashMap<ChunkPos, Chunk<T>>,
        center_pos: ChunkPos,
    ) -> anyhow::Result<Option<Self>> {
        let chunk = chunks.get(&center_pos).with_context(|| {
            format!(
                "Tried to create ChunkMeshGeneratorInput for non-existent chunk at position {:?}",
                center_pos
            )
        })?;

        let mut neighbors = Box::new([
            (Border::new(Face::Bottom)),
            (Border::new(Face::Top)),
            (Border::new(Face::Right)),
            (Border::new(Face::Left)),
            (Border::new(Face::Back)),
            (Border::new(Face::Front)),
        ]);

        let mut neighbors_occlude = true;

        for (i, face) in Face::all().iter().enumerate() {
            let neighbor_pos = center_pos.get_neighbor(*face);
            if let Some(neighbor_chunk) = chunks.get(&neighbor_pos) {
                neighbors[i].copy_from_chunk(&neighbor_chunk);
                if neighbors_occlude && !neighbors[i].occludes {
                    neighbors_occlude = false;
                }
            } else {
                // TODO: This causes panics for initial mesh generation, which doesn't ensure neighbor chunks are loaded yet
                // Does this mean initial chunk might have invalid AO and / or extraneous faces, because neighbors are missing?
                /*panic!(
                    "Tried to create ChunkMeshGeneratorInput for {:?} but neighbor chunk at position {:?} does not exist",
                    center_pos, neighbor_pos
                );*/
            }
        }

        if neighbors_occlude {
            // No need to mesh a chunk that is fully occluded
            // TODO: Handle edge case when the camera is inside a fully occluded chunk
            return Ok(None);
        }

        let center = UnpackedChunk::try_from_chunk(&chunk);

        let center = match center {
            UnpackedChunkResult::Data(data) => data,
            UnpackedChunkResult::Empty => {
                // No need to mesh an empty chunk
                return Ok(None);
            }
        };

        Ok(Some(ChunkMeshGeneratorInput {
            center,
            center_pos,
            neighbors,
        }))
    }

    pub fn get_voxel(&self, world_pos: WorldPos) -> Option<Voxel> {
        // Determine which chunk the world_pos belongs to
        let chunk_pos = world_pos.to_chunk_pos();
        let local_pos = world_pos.to_local_pos();

        if chunk_pos == self.center_pos {
            return self.center.get_voxel(local_pos);
        }

        // Compute which neighbor chunk to query
        let offset = chunk_pos - self.center_pos;
        let face = match (offset.0.x, offset.0.y, offset.0.z) {
            (1, 0, 0) => Face::Right,
            (-1, 0, 0) => Face::Left,
            (0, 1, 0) => Face::Top,
            (0, -1, 0) => Face::Bottom,
            (0, 0, 1) => Face::Front,
            (0, 0, -1) => Face::Back,
            _ => return None,
        };

        let neighbor_index = (face as i32) as usize;
        let neighbor_border = &self.neighbors[neighbor_index];
        neighbor_border.get_voxel(local_pos)
    }

    /// This should only be used in tests.
    pub fn set_voxel(&mut self, world_pos: WorldPos, voxel: Voxel) {
        // Determine which chunk the world_pos belongs to
        let chunk_pos = world_pos.to_chunk_pos();
        let local_pos = world_pos.to_local_pos();

        if chunk_pos == self.center_pos {
            let index = local_pos.to_chunk_data_index();
            self.center.voxels[index] = voxel;
            return;
        }

        // Compute which neighbor chunk to query
        let offset = chunk_pos - self.center_pos;
        let face = match (offset.0.x, offset.0.y, offset.0.z) {
            (1, 0, 0) => Face::Right,
            (-1, 0, 0) => Face::Left,
            (0, 1, 0) => Face::Top,
            (0, -1, 0) => Face::Bottom,
            (0, 0, 1) => Face::Front,
            (0, 0, -1) => Face::Back,
            _ => return,
        };

        let neighbor_index = (face as i32) as usize;
        let neighbor_border = &mut self.neighbors[neighbor_index];
        neighbor_border.set_voxel(local_pos, voxel);
    }
}
