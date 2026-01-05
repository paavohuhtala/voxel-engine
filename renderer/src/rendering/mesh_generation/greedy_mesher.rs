use std::sync::Arc;

use engine::{
    assets::blocks::BlockDatabaseSlim,
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
    chunk_mesh::ChunkMeshData,
    chunk_mesh::{PackedVoxelFace, VoxelFace},
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
    block_database: Arc<BlockDatabaseSlim>,
    mask: Vec<MaskEntry>,
}

impl GreedyMesher {
    pub fn new(block_database: Arc<BlockDatabaseSlim>) -> Self {
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

                let depth = match direction {
                    FaceDirection::Positive => depth - 1,
                    FaceDirection::Negative => depth,
                };

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
            .get_texture_indices(voxel.block_type_id())
            .expect("Expected to find block definition")
            .get_face_index(face);

        let diagonal = if (ao[0] + ao[2]) < (ao[1] + ao[3]) {
            FaceDiagonal::BottomLeftToTopRight
        } else {
            FaceDiagonal::TopLeftToBottomRight
        };

        chunk_mesh_data.faces.push(PackedVoxelFace::from(VoxelFace {
            position: origin.to_world(),
            face_direction: face,
            size,
            ambient_occlusion: ao,
            flip_diagonal: diagonal == FaceDiagonal::TopLeftToBottomRight,
            texture_index,
        }))
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

        let get_neighbor_voxel = |offset_u: i32, offset_v: i32| -> bool {
            let local_pos = pos.offset(offset_u, offset_v, offset_d);
            let chunk_relative_world_pos = local_pos.to_world();
            self.get_voxel(input, chunk_relative_world_pos).is_some()
        };

        // Pack 8 neighbor samples into a single byte index
        // Bit layout: [top_left, top_right, bottom_right, bottom_left, right, left, top, bottom]
        let index = (get_neighbor_voxel(0, -1) as usize)
            | (get_neighbor_voxel(0, 1) as usize) << 1
            | (get_neighbor_voxel(-1, 0) as usize) << 2
            | (get_neighbor_voxel(1, 0) as usize) << 3
            | (get_neighbor_voxel(-1, -1) as usize) << 4
            | (get_neighbor_voxel(1, -1) as usize) << 5
            | (get_neighbor_voxel(1, 1) as usize) << 6
            | (get_neighbor_voxel(-1, 1) as usize) << 7;
        AO_LOOKUP_TABLE[index]
    }
}

/// Computes the AO value for a single corner given its two adjacent sides and diagonal neighbor.
const fn compute_corner_ao(side1: bool, side2: bool, corner: bool) -> u8 {
    if side1 && side2 {
        3
    } else {
        (side1 as u8) + (side2 as u8) + (corner as u8)
    }
}

/// Generates the full 256-entry AO lookup table at compile time.
/// Input index bit layout: [top_left, top_right, bottom_right, bottom_left, right, left, top, bottom]
/// Output: [ao_bl, ao_br, ao_tr, ao_tl] for each of the 256 combinations.
const fn generate_ao_lookup_table() -> [[u8; 4]; 256] {
    let mut table = [[0u8; 4]; 256];
    let mut i = 0usize;
    while i < 256 {
        let bottom = (i & (1 << 0)) != 0;
        let top = (i & (1 << 1)) != 0;
        let left = (i & (1 << 2)) != 0;
        let right = (i & (1 << 3)) != 0;
        let bottom_left = (i & (1 << 4)) != 0;
        let bottom_right = (i & (1 << 5)) != 0;
        let top_right = (i & (1 << 6)) != 0;
        let top_left = (i & (1 << 7)) != 0;

        // Corner AO values in order: BL(0,0), BR(1,0), TR(1,1), TL(0,1)
        table[i] = [
            compute_corner_ao(left, bottom, bottom_left), // Corner 0: Bottom-Left
            compute_corner_ao(right, bottom, bottom_right), // Corner 1: Bottom-Right
            compute_corner_ao(right, top, top_right),     // Corner 2: Top-Right
            compute_corner_ao(left, top, top_left),       // Corner 3: Top-Left
        ];
        i += 1;
    }
    table
}

static AO_LOOKUP_TABLE: [[u8; 4]; 256] = generate_ao_lookup_table();

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use engine::{
        assets::blocks::TextureIndices,
        voxels::{coord::ChunkPos, voxel::Voxel},
    };
    use glam::{IVec3, U8Vec2, U8Vec3};

    fn create_test_block_database() -> Arc<BlockDatabaseSlim> {
        let mut db = BlockDatabaseSlim::new();
        db.add_block(TextureIndices::new_single(0));
        db.add_block(TextureIndices::new_single(1));
        Arc::new(db)
    }

    #[test]
    fn test_single_voxel() {
        let db = create_test_block_database();
        let mut mesher = GreedyMesher::new(db);
        let center_pos = ChunkPos::new(0, 0, 0);
        let mut input = ChunkMeshGeneratorInput::new_empty(center_pos);

        // Place a single voxel at (1, 1, 1)
        let voxel = Voxel::from_type(1);
        input.set_voxel(
            center_pos.origin() + WorldPos::from(IVec3::new(1, 1, 1)),
            voxel,
        );

        let mesh = mesher.generate_mesh(&input);

        // A single voxel should have 6 faces
        assert_eq!(mesh.faces.len(), 6);

        // Check faces
        // All faces should be at (1,1,1) with size (1,1), with texture index 1 and no AO
        // There should be one face for each direction
        let mut directions_found = [false; 6];
        for face in &mesh.faces {
            let unpacked = face.unpack();
            assert_eq!(unpacked.position, U8Vec3::new(1, 1, 1));
            assert_eq!(unpacked.size, U8Vec2::new(1, 1));
            assert_eq!(unpacked.texture_index, 1);
            assert_eq!(unpacked.ambient_occlusion, [0, 0, 0, 0]);
            directions_found[unpacked.face_direction as usize] = true;
        }
        assert!(directions_found.iter().all(|&found| found));
    }

    #[test]
    fn test_greedy_merging() {
        let db = create_test_block_database();
        let mut mesher = GreedyMesher::new(db);
        let center_pos = ChunkPos::new(0, 0, 0);
        let mut input = ChunkMeshGeneratorInput::new_empty(center_pos);

        // Place two voxels next to each other along X axis
        let voxel = Voxel::from_type(1);
        input.set_voxel(
            center_pos.origin() + WorldPos::from(IVec3::new(1, 1, 1)),
            voxel,
        );
        input.set_voxel(
            center_pos.origin() + WorldPos::from(IVec3::new(2, 1, 1)),
            voxel,
        );

        let mesh = mesher.generate_mesh(&input);
        let all_faces = mesh
            .faces
            .iter()
            .map(|f| {
                let unpacked = f.unpack();
                (unpacked.face_direction, unpacked)
            })
            .collect::<HashMap<Face, _>>();

        // There should be 6 faces total
        assert_eq!(mesh.faces.len(), 6);

        let left_face = all_faces.get(&Face::Left).expect("Expected left face");
        let right_face = all_faces.get(&Face::Right).expect("Expected right face");
        assert_eq!(left_face.position, U8Vec3::new(1, 1, 1));
        assert_eq!(right_face.position, U8Vec3::new(2, 1, 1));
        // Left and right faces at the ends, so each should be 1x1
        assert_eq!(right_face.size, U8Vec2::new(1, 1));
        assert_eq!(left_face.size, U8Vec2::new(1, 1));
        // Top, bottom, front, back faces should be merged into 2x1/1x2 quads
        let top_face = all_faces.get(&Face::Top).expect("Expected top face");
        let bottom_face = all_faces.get(&Face::Bottom).expect("Expected bottom face");
        let front_face = all_faces.get(&Face::Front).expect("Expected front face");
        let back_face = all_faces.get(&Face::Back).expect("Expected back face");
        assert_eq!(top_face.position, U8Vec3::new(1, 1, 1));
        assert_eq!(bottom_face.position, U8Vec3::new(1, 1, 1));
        assert_eq!(front_face.position, U8Vec3::new(1, 1, 1));
        assert_eq!(back_face.position, U8Vec3::new(1, 1, 1));
        assert_eq!(top_face.size, U8Vec2::new(1, 2));
        assert_eq!(bottom_face.size, U8Vec2::new(1, 2));
        assert_eq!(front_face.size, U8Vec2::new(2, 1));
        assert_eq!(back_face.size, U8Vec2::new(2, 1));
    }

    // TODO: Add more tests for AO correctness and complex shapes
}
