use std::sync::Arc;

use engine::{
    assets::blocks::BlockDatabase,
    math::{
        axis::Axis,
        basis::Basis,
        local_vec::{ConstructLocalVec3, LocalVec3},
    },
    voxels::{
        chunk::CHUNK_SIZE,
        coord::WorldPos,
        face::{Face, FaceDiagonal},
        voxel::Voxel,
    },
};
use glam::{IVec3, U8Vec2, U8Vec3};

use crate::rendering::{
    chunk_mesh::{ChunkMeshData, ChunkVertex},
    mesh_generation::chunk_mesh_generator_input::ChunkMeshGeneratorInput,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FaceDirection {
    Positive,
    Negative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MaskEntry {
    VoxelFace {
        voxel: Voxel,
        direction: FaceDirection,
        ao: [u8; 4],
    },
    Empty,
}

pub struct GreedyMesher {
    block_database: Arc<BlockDatabase>,
    mask: Vec<MaskEntry>,
}

impl GreedyMesher {
    pub fn new(block_database: Arc<BlockDatabase>) -> Self {
        const CHUNK_USIZE: usize = CHUNK_SIZE as usize;
        GreedyMesher {
            block_database,
            mask: vec![MaskEntry::Empty; CHUNK_USIZE.pow(2)],
        }
    }

    pub fn generate_mesh(&mut self, input: &ChunkMeshGeneratorInput) -> ChunkMeshData {
        let mut chunk_mesh_data = ChunkMeshData::from_position(input.center_pos);

        const AXES: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];
        // For each principal axis, construct a 2D mask and generate quads for exposed faces
        for d_axis in AXES {
            self.create_faces_for_axis(input, &mut chunk_mesh_data, d_axis);
        }
        chunk_mesh_data.aabb = input.center.compute_aabb();
        chunk_mesh_data
    }

    /// Gets the voxel at the given position. The position is provided as chunk-relative coordinates.
    /// If the position is out of bounds for the current chunk, it queries the world for the voxel instead.
    fn get_voxel(&self, input: &ChunkMeshGeneratorInput, offset: IVec3) -> Option<Voxel> {
        let world_pos = input.center_pos.origin() + WorldPos::from(offset);
        let voxel = input.get_voxel(world_pos);

        // Treat AIR as None
        match voxel {
            None | Some(Voxel::AIR) => None,
            Some(voxel) => Some(voxel),
        }
    }

    fn create_faces_for_axis(
        &mut self,
        input: &ChunkMeshGeneratorInput,
        mesh_data: &mut ChunkMeshData,
        d_axis: Axis,
    ) {
        let basis = Basis::new(d_axis.u_axis(), d_axis.v_axis(), d_axis);

        // Iterate across the depth of the chunk along the current axis
        // Note that this is an inclusive range to handle the back faces of the last layer
        for depth in 0..=(CHUNK_SIZE as i32) {
            self.create_mask_for_slice(input, basis, depth);
            self.create_faces_at_depth(mesh_data, basis, depth);
        }
    }

    fn create_mask_for_slice(&mut self, input: &ChunkMeshGeneratorInput, basis: Basis, depth: i32) {
        const N: i32 = CHUNK_SIZE as i32;

        // Clear the mask completely
        self.mask.fill(MaskEntry::Empty);

        for v in 0..N {
            for u in 0..N {
                let pos = LocalVec3::from_uvd(u, v, depth, basis);
                let front = pos.offset(0, 0, -1).to_world();
                let back = pos.to_world();
                let voxel_front = self.get_voxel(input, front);
                let voxel_back = self.get_voxel(input, back);

                let entry = match (voxel_front, voxel_back) {
                    // Voxel's front face is exposed
                    (Some(voxel), None) => {
                        // Only generate if the voxel belongs to the current chunk
                        if depth - 1 < 0 {
                            MaskEntry::Empty
                        } else {
                            let ao = self.calculate_face_ao(input, pos, FaceDirection::Positive);
                            MaskEntry::VoxelFace {
                                voxel,
                                direction: FaceDirection::Positive,
                                ao,
                            }
                        }
                    }
                    // Voxel's back face is exposed
                    (None, Some(voxel)) => {
                        // Only generate if the voxel belongs to the current chunk
                        if depth >= N {
                            MaskEntry::Empty
                        } else {
                            let ao = self.calculate_face_ao(input, pos, FaceDirection::Negative);
                            MaskEntry::VoxelFace {
                                voxel,
                                direction: FaceDirection::Negative,
                                ao,
                            }
                        }
                    }
                    // Both faces of the voxel are either exposed or hidden, skip
                    _ => MaskEntry::Empty,
                };
                self.mask[(v * N + u) as usize] = entry;
            }
        }
    }

    fn create_faces_at_depth(&mut self, mesh_data: &mut ChunkMeshData, basis: Basis, depth: i32) {
        const N: usize = CHUNK_SIZE as usize;

        // Iterate over the mask for this slice and generate quads
        let mut index = 0;
        for v in 0..N {
            let mut u = 0;
            while u < N {
                let entry @ MaskEntry::VoxelFace {
                    voxel,
                    direction,
                    ao,
                } = self.mask[index + u]
                else {
                    // The mask is empty here, skip
                    u += 1;
                    continue;
                };

                // There's a face here, try to expand a quad

                // Expand width
                let mut width = 1;
                while u + width < N {
                    if self.mask[index + u + width] == entry {
                        width += 1;
                    } else {
                        break;
                    }
                }

                let mut height = 1;
                // Expand height
                while v + height < N {
                    // Find the next row below the current quad
                    // Since they're laid out linearly in memory, we can create a slice for it
                    let row_start = index + (height * N) + u;
                    let next_row_slice = &self.mask[row_start..(row_start + width)];
                    if next_row_slice.iter().all(|&e| e == entry) {
                        height += 1;
                    } else {
                        break;
                    }
                }

                let origin = LocalVec3::from_uvd(u as u8, v as u8, depth as u8, basis);

                self.add_quad(
                    mesh_data,
                    origin,
                    (width as u8, height as u8).into(),
                    voxel,
                    direction,
                    ao,
                );

                // Zero out the mask entries we just consumed
                for h in 0..height {
                    let row_start = index + (h * N) + u;
                    self.mask[row_start..(row_start + width)].fill(MaskEntry::Empty);
                }

                // Skip past the quad we just generated
                u += width;
            }
            // Move to the next row
            index += N;
        }
    }

    fn add_quad(
        &self,
        chunk_mesh_data: &mut ChunkMeshData,
        origin: LocalVec3<U8Vec3>,
        size: U8Vec2,
        voxel: Voxel,
        direction: FaceDirection,
        ao: [u8; 4],
    ) {
        let start_index = chunk_mesh_data.vertices.len() as u16;
        let uv = origin.uv();

        // Despite the name, these are not _texture_ UVs, but rather local quad vertex coordinates in the U and V axes
        let uv_coords = [
            uv,
            uv.offset(size.x, 0),
            uv.offset(size.x, size.y),
            uv.offset(0, size.y),
        ];

        let face = match (origin.basis.d, direction) {
            (Axis::Y, FaceDirection::Positive) => Face::Top,
            (Axis::Y, FaceDirection::Negative) => Face::Bottom,
            (Axis::X, FaceDirection::Negative) => Face::Left,
            (Axis::X, FaceDirection::Positive) => Face::Right,
            (Axis::Z, FaceDirection::Positive) => Face::Front,
            (Axis::Z, FaceDirection::Negative) => Face::Back,
        };

        let texture_index = self
            .block_database
            .get_by_id(voxel.block_type_id())
            .expect("Expected to find block definition")
            .get_texture_indices()
            .expect("Expected block to have texture indices")
            .get_face_index(face);

        for (i, pos) in uv_coords.iter().enumerate() {
            let pos = pos.extend(origin.d(), origin.basis.d).to_world();
            chunk_mesh_data.vertices.push(ChunkVertex {
                position: pos.extend(face as u8),
                texture_index,
                ambient_occlusion: ao[i] as u16,
            });
        }

        let diagonal = if (ao[0] + ao[2]) < (ao[1] + ao[3]) {
            FaceDiagonal::BottomLeftToTopRight
        } else {
            FaceDiagonal::TopLeftToBottomRight
        };

        let quad_indices = match direction {
            FaceDirection::Positive => face.indices_ccw(start_index, diagonal),
            FaceDirection::Negative => face.indices_cw(start_index, diagonal),
        };

        chunk_mesh_data.indices.extend_from_slice(&quad_indices);
    }

    fn calculate_face_ao(
        &self,
        input: &ChunkMeshGeneratorInput,
        pos: LocalVec3<IVec3>,
        direction: FaceDirection,
    ) -> [u8; 4] {
        let offset_d = match direction {
            FaceDirection::Positive => 0,
            FaceDirection::Negative => -1,
        };

        let get_neighbor_voxel = |offset_u: i32, offset_v: i32| {
            let local_pos = pos.offset(offset_u, offset_v, offset_d);
            let chunk_relative_world_pos = local_pos.to_world();
            self.get_voxel(input, chunk_relative_world_pos)
        };

        let top_neighbor = get_neighbor_voxel(0, 1);
        let bottom_neighbor = get_neighbor_voxel(0, -1);
        let left_neighbor = get_neighbor_voxel(-1, 0);
        let right_neighbor = get_neighbor_voxel(1, 0);
        let top_left_neighbor = get_neighbor_voxel(-1, 1);
        let top_right_neighbor = get_neighbor_voxel(1, 1);
        let bottom_left_neighbor = get_neighbor_voxel(-1, -1);
        let bottom_right_neighbor = get_neighbor_voxel(1, -1);

        let corners = [
            (left_neighbor, bottom_neighbor, bottom_left_neighbor),
            (right_neighbor, bottom_neighbor, bottom_right_neighbor),
            (right_neighbor, top_neighbor, top_right_neighbor),
            (left_neighbor, top_neighbor, top_left_neighbor),
        ];

        let mut ao = [0u8; 4];
        for (i, (side1, side2, corner)) in corners.iter().enumerate() {
            let occlusion = match (side1, side2, corner) {
                // Both sides are occupied, max occlusion
                (Some(_), Some(_), _) => 2,
                // One side occupied, take corner into account
                (Some(_), None, corner) | (None, Some(_), corner) => {
                    1 + corner.map(|_| 1).unwrap_or(0)
                }
                // No sides occupied, no occlusion
                (None, None, _) => 0,
            };
            ao[i] = occlusion;
        }
        ao
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::{
        assets::blocks::{BlockDatabase, BlockDefinition, BlockTextureDefinition, TextureIndices},
        voxels::{coord::ChunkPos, voxel::Voxel},
    };

    fn create_test_block_database() -> Arc<BlockDatabase> {
        let mut db = BlockDatabase::new();
        db.add_block_from_definition(BlockDefinition {
            id: 1,
            name: "test_block".to_string(),
            textures: BlockTextureDefinition::Invisible,
        })
        .unwrap();

        if let Some(entry) = db.get_by_id(engine::assets::blocks::BlockTypeId(1)) {
            entry.set_texture_indices(TextureIndices::new_single(0));
        }

        Arc::new(db)
    }

    #[test]
    fn test_single_voxel() {
        let db = create_test_block_database();
        let mut mesher = GreedyMesher::new(db);
        let mut input = ChunkMeshGeneratorInput::new_empty(ChunkPos::new(0, 0, 0));

        // Place a single voxel in the center
        let voxel = Voxel::from_type(1);
        let center_pos = WorldPos::new(8, 8, 8);
        input.set_voxel(center_pos, voxel);

        let mesh = mesher.generate_mesh(&input);

        // Should have 6 faces (quads)
        // Each quad has 4 vertices -> 24 vertices
        // Each quad has 6 indices -> 36 indices
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn test_greedy_meshing_simple() {
        let db = create_test_block_database();
        let mut mesher = GreedyMesher::new(db);
        let mut input = ChunkMeshGeneratorInput::new_empty(ChunkPos::new(0, 0, 0));

        // Place two voxels next to each other along X axis
        let voxel = Voxel::from_type(1);
        let p1 = WorldPos::new(8, 8, 8);
        let p2 = WorldPos::new(9, 8, 8);

        input.set_voxel(p1, voxel);
        input.set_voxel(p2, voxel);

        let mesh = mesher.generate_mesh(&input);

        // Should have same number of faces as a single voxel, since adjacent voxels are merged
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn test_chunk_boundary_culling() {
        let db = create_test_block_database();
        let mut mesher = GreedyMesher::new(db);
        let mut input = ChunkMeshGeneratorInput::new_empty(ChunkPos::new(0, 0, 0));

        let voxel = Voxel::from_type(1);

        // Place voxel at right boundary (x=15)
        let p1 = WorldPos::new(15, 8, 8);
        input.set_voxel(p1, voxel);

        // Case 1: Neighbor is empty
        let mesh = mesher.generate_mesh(&input);
        // Should generate Right face
        let has_right_face = mesh.vertices.iter().any(|v| {
            // Face ID is stored in w component of position (last byte)
            v.position.w == Face::Right as u8
        });
        assert!(
            has_right_face,
            "Should generate right face when neighbor is empty"
        );

        // Case 2: Neighbor has solid block
        // Update neighbor border (Right neighbor, so we update the Right border)
        let neighbor_pos = WorldPos::new(16, 8, 8);
        input.set_voxel(neighbor_pos, voxel);

        let mesh = mesher.generate_mesh(&input);
        let has_right_face = mesh
            .vertices
            .iter()
            .any(|v| v.position.w == Face::Right as u8);
        assert!(
            !has_right_face,
            "Should NOT generate right face when neighbor is solid"
        );
    }

    #[test]
    fn test_chunk_boundary_no_overlap() {
        let db = create_test_block_database();
        let mut mesher = GreedyMesher::new(db);
        let mut input = ChunkMeshGeneratorInput::new_empty(ChunkPos::new(0, 0, 0));

        let voxel = Voxel::from_type(1);

        // Place voxel in neighbor's border (Right neighbor)
        let neighbor_pos = WorldPos::new(16, 8, 8);
        input.set_voxel(neighbor_pos, voxel);

        // Center chunk is empty.
        // The mesher should NOT generate any faces for the neighbor's voxel.

        let mesh = mesher.generate_mesh(&input);
        assert_eq!(
            mesh.vertices.len(),
            0,
            "Should not generate faces for neighbor voxels"
        );
    }
}
