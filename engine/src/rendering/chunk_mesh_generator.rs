use crate::{
    rendering::{
        chunk_mesh::{ChunkMeshData, ChunkVertex},
        mesh_generations::greedy_mesher::GreedyMesher,
    },
    voxels::{
        chunk::{CHUNK_SIZE, Chunk},
        coord::ChunkPos,
        face::Face,
        voxel::Voxel,
    },
};

pub fn generate_chunk_mesh_data(pos: ChunkPos, data: &Chunk) -> ChunkMeshData {
    match data {
        Chunk::Solid(solid_voxel) => generate_solid_chunk_mesh(pos, *solid_voxel),
        Chunk::Packed(packed) => {
            // TODO: Reuse instance of GreedyMesher to avoid reallocating the mask and voxel buffer every time
            let mut mesher = GreedyMesher::new();
            mesher.generate_mesh(pos, packed)
        }
    }
}

const FACES: [Face; 6] = [
    Face::Top,
    Face::Bottom,
    Face::Left,
    Face::Right,
    Face::Front,
    Face::Back,
];

fn generate_solid_chunk_mesh(pos: ChunkPos, voxel: Voxel) -> ChunkMeshData {
    let mut mesh_data = ChunkMeshData::from_position(pos);

    if voxel == Voxel::AIR {
        return mesh_data;
    }

    for face in FACES {
        for vertex in face.vertices() {
            // Add other metadata here if needed
            let position = (vertex * CHUNK_SIZE as u8).extend(face as u8);

            mesh_data.vertices.push(ChunkVertex {
                position,
                // TODO: Block type and material index do not necessarily match 1:1
                material_index: voxel.block_type(),
                _padding: 0,
            });
        }

        let start_index = (face as u16 * 4) as u16;
        mesh_data
            .indices
            .extend_from_slice(&face.indices_ccw(start_index));
    }

    // This is a full chunk, so y range is 0 to CHUNK_SIZE
    let y_range = create_packed_y_range(0, CHUNK_SIZE as u8);

    mesh_data.position_and_y_range = pos.0.extend(y_range as i32);
    mesh_data
}

/// Packs minimum and maximum y values into a single u8
fn create_packed_y_range(min_y: u8, max_y: u8) -> u8 {
    (min_y & 0x0F) | ((max_y & 0x0F) << 4)
}
