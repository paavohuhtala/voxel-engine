use crate::{
    assets::blocks::BlockDatabase,
    rendering::{
        chunk_mesh::{ChunkMeshData, ChunkVertex},
        mesh_generations::greedy_mesher::GreedyMesher,
    },
    voxels::{
        chunk::{CHUNK_SIZE, Chunk},
        coord::ChunkPos,
        face::{Face, FaceDiagonal},
        voxel::Voxel,
        world::World,
    },
};

pub fn generate_chunk_mesh_data(
    block_database: &BlockDatabase,
    pos: ChunkPos,
    data: &Chunk,
    world: &World,
) -> ChunkMeshData {
    match data {
        Chunk::Solid(solid_voxel) => generate_solid_chunk_mesh(block_database, pos, *solid_voxel),
        Chunk::Packed(packed) => {
            // TODO: Reuse instance of GreedyMesher to avoid reallocating the mask and voxel buffer every time
            let mut mesher = GreedyMesher::new(block_database);
            mesher.generate_mesh(world, pos, packed)
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

fn generate_solid_chunk_mesh(
    block_database: &BlockDatabase,
    pos: ChunkPos,
    voxel: Voxel,
) -> ChunkMeshData {
    let mut mesh_data = ChunkMeshData::from_position(pos);

    if voxel == Voxel::AIR {
        return mesh_data;
    }

    let block = block_database.get_by_id(voxel.block_type_id());

    let Some(block) = block else {
        log::error!(
            "Block type ID {:?} not found in database when generating solid chunk mesh. Treating as air.",
            voxel.block_type_id()
        );
        return mesh_data;
    };

    let Some(texture_indices) = block.get_texture_indices() else {
        log::error!(
            "Block type ID {:?} has no texture indices when generating solid chunk mesh. Treating as air.",
            voxel.block_type_id()
        );
        return mesh_data;
    };

    for face in FACES {
        let texture_index = texture_indices.get_face_index(face);
        for vertex in face.vertices() {
            // Add other metadata here if needed
            let position = (vertex * CHUNK_SIZE as u8).extend(face as u8);

            mesh_data.vertices.push(ChunkVertex {
                position,
                // TODO: Block type and material index do not necessarily match 1:1
                texture_index,
                // TODO: Ambient occlusion can still affect solid chunks
                ambient_occlusion: 0,
            });
        }

        let start_index = (face as u16 * 4) as u16;
        mesh_data
            .indices
            .extend_from_slice(&face.indices_ccw(start_index, FaceDiagonal::TopLeftToBottomRight));
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
