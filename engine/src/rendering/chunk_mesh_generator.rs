// TODO: Replace with real implementation

use glam::U8Vec3;

use crate::{
    rendering::chunk_mesh::{ChunkMeshData, ChunkVertex},
    voxels::{
        chunk::{CHUNK_SIZE, Chunk},
        coord::ChunkPos,
        voxel::Voxel,
    },
};

pub fn generate_chunk_mesh_data(pos: ChunkPos, data: &Chunk) -> ChunkMeshData {
    match data {
        Chunk::Solid(solid_voxel) => generate_solid_chunk_mesh(pos, *solid_voxel),
        Chunk::Packed(_packed) => unimplemented!(),
    }
}

const CUBE_VERTICES: [[U8Vec3; 4]; 6] = [
    // In order of the Face enum: Top, Bottom, Left, Right, Front, Back
    [
        // Top (Y+)
        U8Vec3::new(0, 1, 0),
        U8Vec3::new(0, 1, 1),
        U8Vec3::new(1, 1, 1),
        U8Vec3::new(1, 1, 0),
    ],
    [
        // Bottom (Y-)
        U8Vec3::new(0, 0, 0),
        U8Vec3::new(1, 0, 0),
        U8Vec3::new(1, 0, 1),
        U8Vec3::new(0, 0, 1),
    ],
    [
        // Left (X-)
        U8Vec3::new(0, 0, 0),
        U8Vec3::new(0, 0, 1),
        U8Vec3::new(0, 1, 1),
        U8Vec3::new(0, 1, 0),
    ],
    [
        // Right (X+)
        U8Vec3::new(1, 0, 1),
        U8Vec3::new(1, 0, 0),
        U8Vec3::new(1, 1, 0),
        U8Vec3::new(1, 1, 1),
    ],
    [
        // Front (Z+)
        U8Vec3::new(0, 0, 1),
        U8Vec3::new(1, 0, 1),
        U8Vec3::new(1, 1, 1),
        U8Vec3::new(0, 1, 1),
    ],
    [
        // Back (Z-)
        U8Vec3::new(1, 0, 0),
        U8Vec3::new(0, 0, 0),
        U8Vec3::new(0, 1, 0),
        U8Vec3::new(1, 1, 0),
    ],
];

const FACE_INDICES: [u16; 6] = [0, 1, 2, 2, 3, 0];

fn generate_solid_chunk_mesh(pos: ChunkPos, voxel: Voxel) -> ChunkMeshData {
    // If the chunk is solid, create a cube mesh filled with that voxel type
    if voxel == Voxel::AIR {
        // A chunk full of air does not need any mesh data
        return ChunkMeshData {
            position_and_y_range: pos.0.extend(0),
            vertices: Vec::new(),
            indices: Vec::new(),
        };
    }

    let mut vertices = Vec::with_capacity(8);
    let mut indices = Vec::with_capacity(36);

    for (face_id, face_vertices) in CUBE_VERTICES.iter().enumerate() {
        for &vertex in face_vertices.iter() {
            // Add other metadata here if needed
            let position = (vertex * CHUNK_SIZE as u8).extend(face_id as u8);

            vertices.push(ChunkVertex {
                position,
                // TODO: Block type and material index do not necessarily match 1:1
                material_index: voxel.block_type(),
                _padding: 0,
            });
        }

        let start_index = (face_id * 4) as u16;
        for &index in FACE_INDICES.iter() {
            indices.push(start_index + index);
        }
    }

    // This is a full chunk, so y range is 0 to CHUNK_SIZE
    let y_range = create_packed_y_range(0, CHUNK_SIZE as u8);

    ChunkMeshData {
        position_and_y_range: pos.0.extend(y_range as i32),
        vertices,
        indices,
    }
}

/// Packs minimum and maximum y values into a single u8
fn create_packed_y_range(min_y: u8, max_y: u8) -> u8 {
    (min_y & 0x0F) | ((max_y & 0x0F) << 4)
}
