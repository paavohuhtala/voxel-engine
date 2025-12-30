use crate::{
    assets::blocks::BlockDatabase,
    math::axis::{Axis, Vector3AxisExt},
    rendering::chunk_mesh::{ChunkMeshData, ChunkVertex},
    voxels::{
        chunk::{CHUNK_SIZE, PackedChunk},
        coord::{ChunkPos, WorldPos},
        face::{Face, FaceDiagonal},
        voxel::Voxel,
        world::World,
    },
};
use glam::{IVec3, U8Vec3};

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

pub struct GreedyMesher<'a> {
    block_database: &'a BlockDatabase,
    mask: Vec<MaskEntry>,
    voxels: Vec<Voxel>,
}

impl<'a> GreedyMesher<'a> {
    pub fn new(block_database: &'a BlockDatabase) -> Self {
        const CHUNK_USIZE: usize = CHUNK_SIZE as usize;
        GreedyMesher {
            block_database,
            mask: vec![MaskEntry::Empty; CHUNK_USIZE.pow(2)],
            voxels: Vec::with_capacity(CHUNK_USIZE.pow(3)),
        }
    }

    pub fn generate_mesh(
        &mut self,
        world: &World,
        chunk_pos: ChunkPos,
        chunk: &PackedChunk,
    ) -> ChunkMeshData {
        self.reset();

        let mut chunk_mesh_data = ChunkMeshData::from_position(chunk_pos);
        // Unpack the chunk for faster access
        // Data is in Y->Z->X order
        chunk.unpack(&mut self.voxels);

        const AXES: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];
        // For each principal axis, construct a 2D mask and generate quads for exposed faces
        for d_axis in AXES {
            self.create_faces_for_axis(world, chunk_pos, &mut chunk_mesh_data, d_axis);
        }
        // TODO: Calculate real min_y and max_y
        chunk_mesh_data.set_y_range(0, 15);

        chunk_mesh_data
    }

    fn reset(&mut self) {
        // We don't need to clear the mask here, because generate_mesh fills it completely for each slice
        self.voxels.clear();
    }

    /// Gets the voxel at the given position. The position is provided as chunk-relative coordinates.
    /// If the position is out of bounds for the current chunk, it queries the world for the voxel instead.
    fn get_voxel(&self, world: &World, chunk_pos: ChunkPos, pos: IVec3) -> Option<Voxel> {
        const N: i32 = CHUNK_SIZE as i32;
        let voxel = if pos.x < 0 || pos.y < 0 || pos.z < 0 || pos.x >= N || pos.y >= N || pos.z >= N
        {
            let world_pos = chunk_pos.origin() + WorldPos::from(pos);
            world.get_voxel(world_pos)
        } else {
            // Voxel is inside the current chunk, get it from the voxel buffer
            let index = (pos.y * N * N) + (pos.z * N) + pos.x;
            Some(self.voxels[index as usize])
        };

        // Treat AIR as None
        match voxel {
            None | Some(Voxel::AIR) => None,
            Some(voxel) => Some(voxel),
        }
    }

    fn create_faces_for_axis(
        &mut self,
        world: &World,
        chunk_pos: ChunkPos,
        mesh_data: &mut ChunkMeshData,
        d_axis: Axis,
    ) {
        // Iterate across the depth of the chunk along the current axis
        // Note that this is an inclusive range to handle the back faces of the last layer
        for depth in 0..=(CHUNK_SIZE as i32) {
            self.create_mask_for_slice(d_axis, depth, world, chunk_pos);
            self.create_faces_at_depth(mesh_data, d_axis, depth);
        }
    }

    fn create_mask_for_slice(
        &mut self,
        d_axis: Axis,
        depth: i32,
        world: &World,
        chunk_pos: ChunkPos,
    ) {
        const N: i32 = CHUNK_SIZE as i32;

        // Clear the mask completely
        self.mask.fill(MaskEntry::Empty);

        let u_axis = d_axis.u_axis();
        let v_axis = d_axis.v_axis();

        let d_vec = d_axis.as_unit_vector();
        let u_vec = u_axis.as_unit_vector();
        let v_vec = v_axis.as_unit_vector();

        for v in 0..N {
            for u in 0..N {
                let pos = d_vec * depth + u_vec * u + v_vec * v;
                let voxel_front = self.get_voxel(world, chunk_pos, pos - d_vec);
                let voxel_back = self.get_voxel(world, chunk_pos, pos);

                let entry = match (voxel_front, voxel_back) {
                    // Voxel's front face is exposed
                    (Some(voxel), None) => {
                        let ao = self.calculate_face_ao(
                            world,
                            chunk_pos,
                            d_axis,
                            u_axis,
                            v_axis,
                            depth,
                            u,
                            v,
                            FaceDirection::Positive,
                        );
                        MaskEntry::VoxelFace {
                            voxel,
                            direction: FaceDirection::Positive,
                            ao,
                        }
                    }
                    // Voxel's back face is exposed
                    (None, Some(voxel)) => {
                        let ao = self.calculate_face_ao(
                            world,
                            chunk_pos,
                            d_axis,
                            u_axis,
                            v_axis,
                            depth,
                            u,
                            v,
                            FaceDirection::Negative,
                        );
                        MaskEntry::VoxelFace {
                            voxel,
                            direction: FaceDirection::Negative,
                            ao,
                        }
                    }
                    // Both faces of the voxel are either exposed or hidden, skip
                    _ => MaskEntry::Empty,
                };
                self.mask[(v * N + u) as usize] = entry;
            }
        }
    }

    fn create_faces_at_depth(&mut self, mesh_data: &mut ChunkMeshData, d_axis: Axis, depth: i32) {
        const N: usize = CHUNK_SIZE as usize;

        let u_axis = d_axis.u_axis();
        let v_axis = d_axis.v_axis();

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

                self.add_quad(
                    mesh_data,
                    d_axis,
                    u_axis,
                    v_axis,
                    depth as u8,
                    u as u8,
                    v as u8,
                    width as u8,
                    height as u8,
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
        d_axis: Axis,
        u_axis: Axis,
        v_axis: Axis,
        d: u8,
        u: u8,
        v: u8,
        width: u8,
        height: u8,
        voxel: Voxel,
        direction: FaceDirection,
        ao: [u8; 4],
    ) {
        let start_index = chunk_mesh_data.vertices.len() as u16;

        let uv_coords = [
            (u, v),
            (u + width, v),
            (u + width, v + height),
            (u, v + height),
        ];

        let face = match (d_axis, direction) {
            (Axis::Y, FaceDirection::Positive) => Face::Top,
            (Axis::Y, FaceDirection::Negative) => Face::Bottom,
            (Axis::X, FaceDirection::Negative) => Face::Left,
            (Axis::X, FaceDirection::Positive) => Face::Right,
            (Axis::Z, FaceDirection::Positive) => Face::Front,
            (Axis::Z, FaceDirection::Negative) => Face::Back,
        };

        for (i, (curr_u, curr_v)) in uv_coords.iter().enumerate() {
            let pos = U8Vec3::from_axis_values([(d_axis, d), (u_axis, *curr_u), (v_axis, *curr_v)]);
            chunk_mesh_data.vertices.push(ChunkVertex {
                position: pos.extend(face as u8),
                texture_index: voxel.block_type(),
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
        world: &World,
        chunk_pos: ChunkPos,
        d_axis: Axis,
        u_axis: Axis,
        v_axis: Axis,
        d: i32,
        u: i32,
        v: i32,
        direction: FaceDirection,
    ) -> [u8; 4] {
        let neighbor_d = match direction {
            FaceDirection::Positive => d,
            FaceDirection::Negative => d - 1,
        };

        let get_neighbor_voxel = |offset_u: i32, offset_v: i32| {
            let chunk_relative_position = IVec3::from_axis_values([
                (d_axis, neighbor_d),
                (u_axis, u + offset_u),
                (v_axis, v + offset_v),
            ]);
            self.get_voxel(world, chunk_pos, chunk_relative_position)
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
